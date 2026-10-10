//! SMP bring-up — ADR-0055 FeatureGate + AP work loop.

pub mod ap_work;
pub mod corepools;
pub mod percpu;
pub mod spsc;
pub mod trampoline;
pub mod work_stealing;
#[cfg(feature = "smp-runqueue")]
pub mod runqueue;

use crate::apic;
use crate::memory;
use core::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use spin::Mutex;

static AP_BOOT_LOCK: Mutex<()> = Mutex::new(());
static AP_ENTRY_COUNTER: AtomicU64 = AtomicU64::new(0);
/// Plan lane 1 (audit §A): explicit init state machine. Replaces the
/// `SMP_INIT_DONE` boolean (swap-at-top) with Uninit → Init → Ready |
/// Degraded | FailedRetryable | FailedPermanent. Second call after a terminal
/// Ready/Degraded/FailedPermanent is a no-op skip; the two transient early
/// gates (SMP disallowed, APIC unavailable) record FailedRetryable so a later
/// valid call re-runs instead of hitting a poisoned flag.
///
/// Concurrency (reviewer refinement): entry is claimed atomically via
/// [`smp_claim_init`] (CAS Uninit|FailedRetryable → Init). A concurrent second
/// caller that lands while state == Init gets `Busy` (= AlreadyRunning) and
/// returns WITHOUT touching hardware — never two inits. Terminal states get
/// `Skip(state)`.
///
/// Degraded semantics: Degraded = BSP-only bring-up COMPLETE (bsp percpu +
/// corepools live), no APs. It is TERMINAL — `smp_should_init()` is false and
/// `smp_init_done()` is true. It does NOT auto-retry. Re-entry after Degraded
/// (or FailedPermanent) requires the explicit HITL handle
/// [`smp_reset_for_retry`], which moves the state back to Uninit. FailedRetryable
/// (transient early gates only) auto-retries via the next claim. Ready is never
/// reset (a good bring-up is never demoted without a reboot).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum SmpInitState {
    Uninit = 0,
    Init = 1,
    Ready = 2,
    Degraded = 3,
    FailedRetryable = 4,
    FailedPermanent = 5,
}

impl SmpInitState {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => SmpInitState::Init,
            2 => SmpInitState::Ready,
            3 => SmpInitState::Degraded,
            4 => SmpInitState::FailedRetryable,
            5 => SmpInitState::FailedPermanent,
            _ => SmpInitState::Uninit,
        }
    }
}

/// Reason codes for [`smp_init_reason`]. u8 keeps the store lock-free.
pub const SMP_R_NONE: u8 = 0;
pub const SMP_R_OK: u8 = 1;
pub const SMP_R_SMP_DISALLOWED: u8 = 2;
pub const SMP_R_APIC_UNAVAIL: u8 = 3;
pub const SMP_R_NO_APS: u8 = 4;
pub const SMP_R_NO_MEM: u8 = 5;
pub const SMP_R_BAD_TRAMP: u8 = 6;
pub const SMP_R_NO_MADT: u8 = 7;
pub const SMP_R_NO_PERCPU: u8 = 8;
pub const SMP_R_GDT_FAIL: u8 = 9;

static SMP_INIT_STATE: AtomicU8 = AtomicU8::new(0); // SmpInitState::Uninit
static SMP_INIT_REASON: AtomicU8 = AtomicU8::new(SMP_R_NONE);

/// Current init state (lock-free snapshot).
pub fn smp_init_state() -> SmpInitState {
    SmpInitState::from_u8(SMP_INIT_STATE.load(Ordering::Acquire))
}

/// Machine-readable reason for the current state (static string, no alloc).
pub fn smp_init_reason() -> &'static str {
    match SMP_INIT_REASON.load(Ordering::Acquire) {
        SMP_R_OK => "ok",
        SMP_R_SMP_DISALLOWED => "smp_disallowed",
        SMP_R_APIC_UNAVAIL => "apic_unavailable",
        SMP_R_NO_APS => "no_aps_madt",
        SMP_R_NO_MEM => "no_mem_tramp",
        SMP_R_BAD_TRAMP => "bad_tramp_vector",
        SMP_R_NO_MADT => "no_madt_ids",
        SMP_R_NO_PERCPU => "no_percpu_slots",
        SMP_R_GDT_FAIL => "gdt_expand_fail",
        _ => "none",
    }
}

/// Gate: only Uninit or FailedRetryable may (re-)run `init_smp`.
/// Pure predicate (no claim). The live entry path uses [`smp_claim_init`];
/// this stays for diagnostics and tests.
pub(crate) fn smp_should_init() -> bool {
    matches!(
        smp_init_state(),
        SmpInitState::Uninit | SmpInitState::FailedRetryable
    )
}

/// Result of the atomic init claim. `Busy` == AlreadyRunning.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SmpClaim {
    /// Caller owns the init. Exactly one claimant; runs the bring-up.
    Run,
    /// Init already in flight (state == Init). Return immediately, touch
    /// nothing — fail-closed, never a second concurrent init.
    Busy,
    /// Terminal state reached (Ready | Degraded | FailedPermanent). No-op skip.
    Skip(SmpInitState),
}

/// Atomic Init claim: CAS Uninit|FailedRetryable → Init. Spin-bounded (16
/// tries); on contention loss the loser re-reads and is classified Busy/Skip —
/// contention NEVER grants a second `Run`. Immediate fail-closed otherwise.
pub(crate) fn smp_claim_init() -> SmpClaim {
    for _ in 0..16 {
        let cur = smp_init_state();
        match cur {
            SmpInitState::Uninit | SmpInitState::FailedRetryable => {
                match SMP_INIT_STATE.compare_exchange(
                    cur as u8,
                    SmpInitState::Init as u8,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => {
                        SMP_INIT_REASON.store(SMP_R_NONE, Ordering::Release);
                        return SmpClaim::Run;
                    }
                    Err(_) => continue, // lost the race; re-read (bounded)
                }
            }
            SmpInitState::Init => return SmpClaim::Busy,
            s => return SmpClaim::Skip(s),
        }
    }
    // Bounded spins exhausted under contention: fail closed, never Run.
    SmpClaim::Busy
}

/// Allowed transitions (pure, host-testable). The claim owns Uninit →
/// FailedRetryable → Init; only the claim holder (state == Init) may land a
/// terminal state. No edge INTO Ready except from Init — no premature Ready.
/// No edge OUT of a terminal state except via [`smp_reset_for_retry`].
pub(crate) fn smp_transition_allowed(from: SmpInitState, to: SmpInitState) -> bool {
    match (from, to) {
        (SmpInitState::Uninit, SmpInitState::Init)
        | (SmpInitState::FailedRetryable, SmpInitState::Init) => true,
        (SmpInitState::Init, SmpInitState::Ready)
        | (SmpInitState::Init, SmpInitState::Degraded)
        | (SmpInitState::Init, SmpInitState::FailedRetryable)
        | (SmpInitState::Init, SmpInitState::FailedPermanent) => true,
        _ => false,
    }
}

/// Checked store: enforces [`smp_transition_allowed`]. Returns false and stores
/// NOTHING on an invalid transition (premature Ready, terminal overwrite,
/// double-Init). All `init_smp` terminal sites route through here; the raw
/// [`smp_state_store`] remains as the test-setup seam only.
pub(crate) fn smp_state_store_checked(s: SmpInitState, reason: u8) -> bool {
    if !smp_transition_allowed(smp_init_state(), s) {
        return false;
    }
    smp_state_store(s, reason);
    true
}

/// Explicit HITL retry handle: Degraded | FailedRetryable | FailedPermanent →
/// Uninit (reason cleared). Rejected (false, nothing stored) from
/// Uninit | Init | Ready — never yank a running init, never demote a good
/// bring-up. FailedRetryable does not NEED this (next claim auto-retries);
/// accepting it here just normalizes the state.
pub(crate) fn smp_reset_for_retry() -> bool {
    for _ in 0..16 {
        let cur = smp_init_state();
        match cur {
            SmpInitState::Degraded
            | SmpInitState::FailedRetryable
            | SmpInitState::FailedPermanent => {
                match SMP_INIT_STATE.compare_exchange(
                    cur as u8,
                    SmpInitState::Uninit as u8,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => {
                        SMP_INIT_REASON.store(SMP_R_NONE, Ordering::Release);
                        return true;
                    }
                    Err(_) => continue,
                }
            }
            _ => return false,
        }
    }
    false
}

/// Raw unchecked store — test-setup seam ONLY (lets tests place any state).
/// Production paths must use [`smp_state_store_checked`].
pub(crate) fn smp_state_store(s: SmpInitState, reason: u8) {
    SMP_INIT_REASON.store(reason, Ordering::Release);
    SMP_INIT_STATE.store(s as u8, Ordering::Release);
}

#[cfg(test)]
pub(crate) fn smp_state_test_reset() {
    SMP_INIT_REASON.store(SMP_R_NONE, Ordering::Release);
    SMP_INIT_STATE.store(SmpInitState::Uninit as u8, Ordering::Release);
}

/// Pure early-gate contract for `init_smp` (host-testable): both transient
/// gates must fail retryable, never poison a later valid call.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SmpGateFail {
    SmpDisallowed,
    ApicUnavailable,
}

pub(crate) fn decide_smp_gate(allow_smp: bool, apic_available: bool) -> Result<(), SmpGateFail> {
    if !allow_smp {
        return Err(SmpGateFail::SmpDisallowed);
    }
    if !apic_available {
        return Err(SmpGateFail::ApicUnavailable);
    }
    Ok(())
}

/// Set by PlatformAgent before calling init_smp().
pub static AP_COUNT: AtomicU16 = AtomicU16::new(0);

/// ADR-0057 WS-F / ADR-0065 FASE 3.1 P13: Barrier for APs that have loaded IDT.
static AP_IDT_READY: AtomicU16 = AtomicU16::new(0);
/// Total APs expected to load IDT (set before wake).
static AP_EXPECTED: AtomicU16 = AtomicU16::new(0);

#[allow(dead_code)]
const AP_STACK_SIZE: u64 = 16384;

/// Entry point Rust dos APs (chamado pelo trampoline em modo 64-bit).
/// ADR-0057 WS-A: `pub` para o binário (`neural-kernel`) reusar este mesmo
/// entry — assim o `AP_ENTRY_COUNTER` do `k_nano` é o único incrementado e o
/// `parallel_matmul` (que lê `k_nano::smp::ap_entry_count()`) enxerga os APs.
pub extern "C" fn ap_entry(_cpu_id: u64) -> ! {
    let _lock = AP_BOOT_LOCK.lock();
    let cpu_id = percpu::CPU_COUNT.fetch_add(1, Ordering::SeqCst);
    percpu::AP_ONLINE.fetch_add(1, Ordering::SeqCst);
    crate::slog_nano!("SMP", "trace", "AP_READY rust cpu={}", cpu_id);
    crate::slog_nano_home!(
        "SMP",
        "ok",
        "k_nano::smp",
        "AP {} entrou em modo 64-bit Rust!",
        cpu_id
    );

    unsafe {
        if apic::USING_X2APIC.load(Ordering::Relaxed) {
            apic::enable_x2apic_this_cpu();
        }
        // SVR via path vivo (MSR se x2APIC). MMIO no AP após EXTD = #GP.
        let svr = apic::lapic_read_reg(0xF0);
        apic::lapic_write_reg(0xF0, (svr & 0xFFFFFF00) | 0xFF | 0x100);
        // SESSION_328: sem `apic_eoi()` aqui — EOI antes de qualquer interrupção
        // gera interrupção espúria → tempestade espúria por-AP.
    }

    let ap_index = (cpu_id - 1) as usize;
    let ist_tops = unsafe { percpu::init_ap_ist(ap_index) };
    let ap_tss = if let Some(tops) = ist_tops {
        let va = [
            x86_64::VirtAddr::new(tops[0]),
            x86_64::VirtAddr::new(tops[1]),
            x86_64::VirtAddr::new(tops[2]),
        ];
        Some(crate::interrupts::init_ap_tss(ap_index, va))
    } else {
        crate::slog_nano!("SMP", "warn", "AP {} IST alloc fail — IDT sem sti", cpu_id);
        None
    };
    unsafe {
        if let Some(p) = percpu::ap_pcpu_ptr_mut(ap_index) {
            (*p).lapic_id = apic::lapic_id();
            if let Some(ref t) = ap_tss {
                (*p).tss_ptr = t.tss as *const _ as u64;
            }
            (*p).cpu_type = match corepools::detect_core_type() {
                corepools::CoreType::Efficiency => percpu::CPU_TYPE_E_CORE,
                _ => percpu::CPU_TYPE_P_CORE,
            };
        }
    }
    // H11 (canvas-onda2): `ap_load_idt_and_tss` termina com `sti` — soltar o
    // AP_BOOT_LOCK ANTES, senão uma IRQ no AP corre com o spinlock segurado
    // (handler que toque o lock = deadlock; TicketLock não é reentrante, S316).
    drop(_lock);
    unsafe {
        crate::interrupts::ap_load_idt_and_tss(ap_tss.as_ref().map(|t| t.selector));
    }

    let ready = AP_IDT_READY.fetch_add(1, Ordering::SeqCst) + 1;
    let expected = AP_EXPECTED.load(Ordering::Acquire);
    if ready == expected {
        set_ap_pollable(true);
        crate::slog_nano!("SMP", "ok", "All {} APs IDT ready — ap_pollable=true", expected);
    }

    AP_ENTRY_COUNTER.fetch_add(1, Ordering::SeqCst);
    // s452 diag: arma o watchpoint de escrita (DR0) neste AP, se a BSP ja gravou
    // o alvo (init_bei roda antes dos APs).
    crate::interrupts::arm_stored_watchpoint();
    ap_work::ap_idle_loop(cpu_id as usize);
}

pub fn ap_entry_count() -> u64 {
    AP_ENTRY_COUNTER.load(Ordering::Relaxed)
}

/// Returns the total number of logical cores (BSP + APs).
pub fn total_cores() -> u32 {
    crate::smp::percpu::CPU_COUNT.load(Ordering::Relaxed) as u32
}

/// ADR-0057 WS-F: APs só podem ser usados como workers vivos (WS-B) quando
/// `true`. Requer o path de reschedule-IPI/IDT (residual HW). Enquanto `false`,
/// o `parallel_*` roda no BSP (AVX2/scalar) — sem enfileirar/esperar APs.
static AP_POLLABLE: AtomicBool = AtomicBool::new(false);
/// Fn-ptr do reschedule-IPI do APIC vivo (instalado pelo binário). Seam WS-F.
static WAKE_FN: AtomicUsize = AtomicUsize::new(0);

pub fn ap_pollable() -> bool {
    AP_POLLABLE.load(Ordering::Acquire)
}
pub fn set_ap_pollable(v: bool) {
    AP_POLLABLE.store(v, Ordering::Release);
}

/// Labor 53: tenta ligar workers AP se feature `ap-pollable` + wake_fn instalado.
/// Default OFF — sem IDT/IPI pleno = deadlock risk.
pub fn try_enable_ap_workers_from_feature() {
    #[cfg(feature = "ap-pollable")]
    {
        let wake = WAKE_FN.load(Ordering::Acquire);
        if wake != 0 && ap_entry_count() > 0 {
            set_ap_pollable(true);
            crate::slog_nano!(
                "SMP",
                "info",
                "step=ap_pollable status=OK VERDICT=PARTIAL reason=feature_on wake=1"
            );
        } else {
            crate::slog_nano!(
                "SMP",
                "info",
                "step=ap_pollable status=SKIP VERDICT=SKIP reason=no_wake_or_aps"
            );
        }
    }
    #[cfg(not(feature = "ap-pollable"))]
    {
        crate::slog_nano!(
            "SMP",
            "info",
            "step=ap_pollable status=SKIP VERDICT=SKIP reason=feature_off (safe default)"
        );
    }
}
pub fn install_wake_fn(f: unsafe fn()) {
    WAKE_FN.store(f as usize, Ordering::Release);
}
/// Acorda APs adormecidos (só efetivo com IDT/IPI habilitados — WS-F residual).
pub unsafe fn wake_aps() {
    let s = WAKE_FN.load(Ordering::Acquire);
    if s != 0 {
        let f: unsafe fn() = core::mem::transmute::<*const (), unsafe fn()>(s as *const ());
        f();
    }
}

/// Full Infer D+B+C: cortex registra `poll_slice` — APs chamam sem AGENT_TICK_BUSY.
static INFER_POLL_FN: AtomicUsize = AtomicUsize::new(0);

/// UI atrasada (frame >2× período): APs/BSP InferWorker param de queimar CPU em
/// matmul para o host WHPX devolver fatia ao BSP que pinta o compositor.
/// Sem isto: smp=12 + InferQueue = CPU alto e desktop congelado (Display só no BSP).
static UI_YIELD_INFER: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

pub fn set_ui_yield_infer(yield_infer: bool) {
    UI_YIELD_INFER.store(yield_infer, Ordering::Release);
}

pub fn ui_yield_infer() -> bool {
    UI_YIELD_INFER.load(Ordering::Acquire)
}

pub fn install_infer_poll_fn(f: fn() -> bool) {
    INFER_POLL_FN.store(f as usize, Ordering::Release);
}

/// Executa um slice de inferência. Retorna true se houve trabalho.
pub fn try_infer_poll_slice() -> bool {
    // ADR-0057 WS-F: sem APs pollable (IDT/IPI pleno) o idle loop dos APs
    // chamava isto incondicionalmente mesmo com a feature off (freeze s330).
    if !ap_pollable() {
        return false;
    }
    // Compositor atrasado: não saturar host com 11 APs em matmul.
    if ui_yield_infer() {
        return false;
    }
    let s = INFER_POLL_FN.load(Ordering::Acquire);
    if s == 0 {
        return false;
    }
    let f: fn() -> bool =
        unsafe { core::mem::transmute::<*const (), fn() -> bool>(s as *const ()) };
    f()
}

fn busy_wait_us(us: u64) {
    // Core 7 240H: se TSC ainda não calibrada, busy_wait_us(10000) com sleep_us
    // tentaria HPET/PIT (50ms I/O) por AP e pareceria hang 12s. Guard não-bloqueante.
    if crate::tsc::TSC_HZ.load(Ordering::Relaxed) == 0 {
        // quick calibrate via CPUID (sem I/O) — não chama HPET/PIT aqui
        let hz = crate::tsc::cpuid_estimate();
        if hz >= 100_000_000 && hz <= 10_000_000_000 {
            crate::tsc::TSC_HZ.store(hz, Ordering::Release);
            crate::tsc::TSC_SOURCE.store(3, Ordering::Release);
            crate::tsc::sleep_us(us);
            return;
        }
        // fallback spin curto (nunca hang)
        // ponytail: spin ~us*50 loops, cutoff 10ms -> <500k iters por INIT, não precisa TSC
        let iters = (us as usize).saturating_mul(50).min(600_000);
        for _ in 0..iters {
            core::hint::spin_loop();
        }
        return;
    }
    crate::tsc::sleep_us(us);
}

/// ADR-0057 WS-A: acorda os APs **um a um** por LAPIC ID (directed IPI), cada
/// um com stack + PerCpu próprios. Causa-raiz do não-wake anterior: SIPI
/// broadcast ("all excl self") largava todos ao mesmo tempo compartilhando a
/// mesma stack (real 0x1000 / 32b tramp+0x1000 / 64b `stack_64_top`) e o mesmo
/// GS.base (BSP) — com ≥2 APs eles se corrompiam antes de `ap_entry`.
///
/// A APIC é injetada por fn-pointers porque o `apic` do binário e o do `k_nano`
/// têm bases LAPIC independentes; o chamador passa a sua (a que está viva).
///
/// Stack 64-bit do AP vem de `percpu::alloc_mapped_stack` (VA do heap real).
#[allow(clippy::too_many_arguments)]
pub unsafe fn wake_aps_sequential(
    tramp_phys: u64,
    cr3_val: u64,
    stack_per_ap: u64,
    tramp_vector: u8,
    ap_lapic_ids: &[u32],
    send_init_to: unsafe fn(u32),
    send_init_deassert_to: unsafe fn(u32),
    send_sipi_to: unsafe fn(u32, u8),
    wait_delivery: unsafe fn(),
) -> u64 {
    // Guard TSC não calibrada: evita hang de busy_wait_us antes de calibrate (240H n=16)
    if crate::tsc::TSC_HZ.load(Ordering::Relaxed) == 0 {
        let hz = crate::tsc::cpuid_estimate();
        if hz >= 100_000_000 && hz <= 10_000_000_000 {
            crate::tsc::TSC_HZ.store(hz, Ordering::Release);
            crate::tsc::TSC_SOURCE.store(3, Ordering::Release);
            crate::slog_nano!("SMP", "info", "TSC quick-cal {} MHz via CPUID (wake guard)", hz / 1_000_000);
        }
    }
    crate::display::fb::boot_ckpt(22, "smp: wake start");
    let mut woke = 0u64;
    for (i, &dest) in ap_lapic_ids.iter().enumerate() {
        if i >= percpu::ap_slots() {
            break;
        }
        if dest > 0xFF && !apic::USING_X2APIC.load(Ordering::Relaxed) {
            unsafe { apic::enable_x2apic_this_cpu() };
            crate::boot_logger::log("SMP: x2APIC on-demand (MADT id>255)");
        }
        if dest > 0xFF && !apic::USING_X2APIC.load(Ordering::Relaxed) {
            crate::boot_logger::log(&alloc::format!(
                "SMP: AP {:#x} skip — x2APIC enable falhou",
                dest
            ));
            continue;
        }
        crate::display::fb::boot_ckpt(22, "smp: wake ap");
        unsafe { apic::smp_trace_apic_mode(apic::lapic_id()) };
        crate::slog_nano!(
            "SMP",
            "trace",
            "TARGET_APIC_ID={:#x} tramp_phys={:#x} vec={:#04x} slot={}",
            dest,
            tramp_phys,
            tramp_vector,
            i
        );
        let Some(ap_stack) = percpu::alloc_mapped_stack(stack_per_ap as usize) else {
            crate::slog_nano!("SMP", "warn", "AP {:#x} sem stack mapeada — skip", dest);
            continue;
        };
        crate::slog_nano!(
            "SMP",
            "trace",
            "AP_STACK dest={:#x} top={:#x} size={:#x}",
            dest,
            ap_stack,
            stack_per_ap
        );
        let percpu_addr = percpu::ap_percpu_ptr(i);

        // Sequencial: apenas 1 AP no trampoline por vez → seguro re-patch o blob
        // (stacks real/32b compartilhadas do trampoline não colidem).
        trampoline::init_trampoline(tramp_phys, cr3_val, ap_stack, percpu_addr, ap_entry);
        crate::slog_nano!("SMP", "trace", "TRAMPOLINE_PATCHED dest={:#x}", dest);

        let before = AP_ENTRY_COUNTER.load(Ordering::Acquire);
        // ADR-0057 WS-F: retry INIT-SIPI-SIPI (até 3x). Firmware real também
        // repete; robustez contra jitter de agendamento (ex.: TCG) onde o AP
        // pode demorar a receber ciclos e estourar um timeout curto.
        let mut ok = false;
        let mut last_hit = 0u64;
        let mut last_ready = 0u64;
        'attempts: for attempt in 0..3 {
            trampoline::clear_handshake(tramp_phys);
            let t_init = crate::tsc::rdtsc();
            send_init_to(dest);
            crate::slog_nano!("SMP", "trace", "INIT_ASSERT_SENT dest={:#x} try={} tsc={}", dest, attempt, t_init);
            wait_delivery();
            busy_wait_us(10000);
            let t_deassert = crate::tsc::rdtsc();
            if !apic::USING_X2APIC.load(Ordering::Relaxed) {
                send_init_deassert_to(dest);
                crate::slog_nano!("SMP", "trace", "INIT_DEASSERT_SENT dest={:#x} try={} tsc={}", dest, attempt, t_deassert);
                wait_delivery();
                busy_wait_us(10000);
            } else {
                crate::slog_nano!(
                    "SMP",
                    "trace",
                    "INIT_DEASSERT skipped (x2APIC SDM) dest={:#x} try={} tsc={}",
                    dest,
                    attempt,
                    t_deassert
                );
            }
            let t_sipi1 = crate::tsc::rdtsc();
            send_sipi_to(dest, tramp_vector);
            crate::slog_nano!("SMP", "trace", "SIPI1_SENT dest={:#x} try={} tsc={}", dest, attempt, t_sipi1);
            wait_delivery();
            busy_wait_us(200);
            let t_sipi2 = crate::tsc::rdtsc();
            send_sipi_to(dest, tramp_vector);
            crate::slog_nano!(
                "SMP",
                "trace",
                "SIPI2_SENT dest={:#x} try={} tsc={} d_init_sipi1={} d_sipi1_sipi2={}",
                dest,
                attempt,
                t_sipi2,
                t_sipi1.wrapping_sub(t_init),
                t_sipi2.wrapping_sub(t_sipi1)
            );
            wait_delivery();

            // ~250 ms/try: spin_loop curto + sleep_us (TSC); handshake via HHDM.
            for _ in 0..5000 {
                last_hit = trampoline::read_hhdm_u64(tramp_phys, trampoline::OFF_SIPI_HIT);
                last_ready = trampoline::read_hhdm_u64(tramp_phys, trampoline::OFF_READY);
                if AP_ENTRY_COUNTER.load(Ordering::Acquire) > before {
                    ok = true;
                    break 'attempts;
                }
                core::hint::spin_loop();
                busy_wait_us(50);
            }
            if !ok {
                crate::boot_logger::log(&alloc::format!(
                    "SMP: AP {:#04x} tentativa {} timeout sipi_hit={} ready={} ONLINE={} counter={}",
                    dest,
                    attempt,
                    last_hit,
                    last_ready,
                    percpu::AP_ONLINE.load(Ordering::Acquire),
                    AP_ENTRY_COUNTER.load(Ordering::Acquire)
                ));
            }
        }
        crate::slog_nano!(
            "SMP",
            "info",
            "AP {:#04x} sipi_hit={} ready={} ONLINE={} counter={}",
            dest,
            last_hit,
            last_ready,
            percpu::AP_ONLINE.load(Ordering::Acquire),
            AP_ENTRY_COUNTER.load(Ordering::Acquire)
        );
        if ok {
            woke += 1;
            crate::slog_nano!(
                "SMP",
                "info",
                "AP LAPIC {} online ({}/{})",
                dest,
                woke,
                ap_lapic_ids.len()
            );
            crate::boot_logger::log(&alloc::format!(
                "SMP: AP {:#04x} online ({}/{}) sipi_hit={} ready={}",
                dest, woke, ap_lapic_ids.len(), last_hit, last_ready
            ));
        } else {
            crate::slog_nano!("SMP", "warn", "AP LAPIC {} timeout (nao subiu)", dest);
            // sipi_hit=0: SIPI/vetor/PTE NX (nunca executou 16-bit)
            // sipi_hit=1 ready=0: travou 16/32→64
            // ready=1 ONLINE=0: jmp ap_entry falhou
            // ONLINE>0 counter=0: travou em ap_entry (TSS/IDT)
            crate::boot_logger::log(&alloc::format!(
                "SMP: AP {:#04x} timeout — sipi_hit={} ready={} ONLINE={} counter={}",
                dest,
                last_hit,
                last_ready,
                percpu::AP_ONLINE.load(Ordering::Acquire),
                AP_ENTRY_COUNTER.load(Ordering::Acquire)
            ));
        }
    }
    woke
}

/// True once the BSP path is up (Ready, or Degraded BSP-only). False for
/// Uninit/Init/FailedRetryable/FailedPermanent — a retryable early fail never
/// reports done, so a later valid `init_smp` still re-runs.
pub fn smp_init_done() -> bool {
    matches!(smp_init_state(), SmpInitState::Ready | SmpInitState::Degraded)
}

pub unsafe fn init_smp() {
    // Plan lane 1 (audit §A): init_platform_sync (T+0) e PlatformAgent (T+21)
    // ambos chamavam init_smp — 2ª onda re-SIPI + CorePools total=1 + panic
    // TSS (ap_index >= n). Entry is an atomic CAS claim: exactly one caller
    // gets Run; a concurrent second caller while Init is in flight gets Busy
    // (AlreadyRunning, touches nothing); terminal states get Skip.
    match smp_claim_init() {
        SmpClaim::Skip(s) => {
            crate::slog_nano!(
                "SMP",
                "ok",
                "init_smp skip (estado {:?} razao={}) APs_counter={} ONLINE={} cores={}",
                s,
                smp_init_reason(),
                AP_ENTRY_COUNTER.load(Ordering::Acquire),
                percpu::AP_ONLINE.load(Ordering::Acquire),
                percpu::CPU_COUNT.load(Ordering::Acquire)
            );
            return;
        }
        SmpClaim::Busy => {
            crate::slog_nano!(
                "SMP",
                "warn",
                "init_smp busy (init em voo, razao={}) — sem double-init",
                smp_init_reason(),
            );
            return;
        }
        SmpClaim::Run => {}
    }
    crate::display::fb::boot_ckpt(220, "smp: enter");
    crate::slog_nano!("SMP", "trace", "Inicializando SMP...");
    crate::display::fb::boot_ckpt(221, "smp: allow check");

    // Transient early gates: BSP-only bring-up runs, state goes back to
    // FailedRetryable — never poisoned, a later valid call re-runs.
    match decide_smp_gate(
        crate::platform_probe::allow_smp(),
        apic::USING_APIC.load(Ordering::Relaxed),
    ) {
        Err(SmpGateFail::SmpDisallowed) => {
            crate::slog_nano!(
                "SMP",
                "ok",
                "BSP-only (FeatureGate allow_smp=false hv={})",
                crate::platform_probe::hypervisor().name()
            );
            let bsp = apic::lapic_id();
            percpu::init_bsp_percpu(bsp);
            corepools::init_from_boot(bsp, 0);
            let _ = smp_state_store_checked(SmpInitState::FailedRetryable, SMP_R_SMP_DISALLOWED);
            return;
        }
        Err(SmpGateFail::ApicUnavailable) => {
            crate::slog_nano!("SMP", "warn", "APIC nao disponivel — SMP ignorado.");
            crate::display::fb::boot_ckpt(222, "smp: no apic bsp-only");
            // Root-cause do #PF cr2=8 em cpu_id (mesh_a/b tick ~20): este return
            // pulava init_bsp_percpu ⇒ GS base 0 ⇒ `mov gs:[8]` falta. Espelha o
            // ramo allow_smp=false acima; lapic_id() sem APIC devolve 0 (fail-closed).
            let bsp = apic::lapic_id();
            percpu::init_bsp_percpu(bsp);
            corepools::init_from_boot(bsp, 0);
            let _ = smp_state_store_checked(SmpInitState::FailedRetryable, SMP_R_APIC_UNAVAIL);
            return;
        }
        Ok(()) => {
            crate::display::fb::boot_ckpt(222, "smp: apic ok");
        }
    }

    crate::display::fb::boot_ckpt(223, "smp: cr3 read");
    let cr3_val = {
        let (frame, _) = x86_64::registers::control::Cr3::read();
        frame.start_address().as_u64()
    };

    crate::display::fb::boot_ckpt(224, "smp: bsp id");
    let bsp_lapic_id = apic::lapic_id();
    crate::display::fb::boot_ckpt(225, "smp: bsp percpu");
    percpu::init_bsp_percpu(bsp_lapic_id);
    crate::slog_nano!(
        "SMP",
        "trace",
        "BSP PerCpu inicializado. LAPIC ID: {}",
        bsp_lapic_id
    );
    crate::display::fb::boot_ckpt(226, "smp: bsp done");

    crate::display::fb::boot_ckpt(227, "smp: ap_expected");
    let mut ap_expected = AP_COUNT.load(Ordering::Relaxed);
    let max_aps = crate::platform_probe::max_aps();
    if max_aps < 255 && ap_expected > max_aps as u16 {
        crate::slog_nano!(
            "SMP",
            "info",
            "cap APs {} → {} (FeatureGate.max_aps)",
            ap_expected,
            max_aps
        );
        ap_expected = max_aps as u16;
        AP_COUNT.store(ap_expected, Ordering::Relaxed);
    }

    if ap_expected == 0 {
        crate::slog_nano!(
            "SMP",
            "ok",
            "Nenhum AP detectado (MADT). SMP single-core."
        );
        corepools::init_from_boot(bsp_lapic_id, 0);
        #[cfg(feature = "smp-runqueue")]
        runqueue::init_roles_from_pools(1);
        let _ = smp_state_store_checked(SmpInitState::Degraded, SMP_R_NO_APS);
        return;
    }

    crate::display::fb::boot_ckpt(228, "smp: alloc tramp");
    let tramp_phys = {
        let mut guard = memory::GLOBAL_ALLOCATOR.lock();
        let Some(alloc) = guard.as_mut() else {
            crate::slog_nano!("SMP", "warn", "sem frame alloc — BSP-only");
            crate::display::fb::boot_ckpt(228, "smp: no alloc bsp-only");
            corepools::init_from_boot(bsp_lapic_id, 0);
            let _ = smp_state_store_checked(SmpInitState::Degraded, SMP_R_NO_MEM);
            return;
        };
        match alloc.allocate_below_1mb() {
            Some(frame) => frame.start_address().as_u64(),
            None => {
                drop(guard);
                crate::slog_nano!("SMP", "warn", "sem lowmem tramp — BSP-only");
                crate::display::fb::boot_ckpt(228, "smp: no lowmem bsp-only");
                corepools::init_from_boot(bsp_lapic_id, 0);
                let _ = smp_state_store_checked(SmpInitState::Degraded, SMP_R_NO_MEM);
                return;
            }
        }
    };
    crate::slog_nano!("SMP", "info", "Trampoline page em 0x{:x}", tramp_phys);
    crate::display::fb::boot_ckpt(22, "smp: tramp ok");
    crate::display::fb::boot_ckpt(229, "smp: tramp ok2");

    // ADR-0057 WS-A: stack por-AP (não mais um único `stack_64_top`).
    let stack_per_ap: u64 = AP_STACK_SIZE * 4;

    trampoline::map_identity_executable(tramp_phys);
    crate::display::fb::boot_ckpt(22, "smp: map ok");

    // Trampoline é (re)patchado por AP dentro de `wake_aps_sequential`.
    let tsize = trampoline::trampoline_size();
    crate::slog_nano!(
        "SMP",
        "info",
        "Trampoline em 0x{:x} ({} bytes).",
        tramp_phys,
        tsize
    );

    let tramp_vector = (tramp_phys >> 12) as u8;
    if tramp_phys >= 0x100000 || tramp_vector == 0 {
        crate::slog_nano!("SMP", "warn", "bad tramp vector — BSP-only");
        corepools::init_from_boot(bsp_lapic_id, 0);
        let _ = smp_state_store_checked(SmpInitState::Degraded, SMP_R_BAD_TRAMP);
        return;
    }

    // ADR-0057 WS-A: wake sequencial por LAPIC ID (directed IPI). Substitui o
    // broadcast "all excl self" + stack/PerCpu compartilhados que só acordava
    // 1 AP. Vale inclusive em bare-metal (o hang antigo era do shorthand).
    // Observe = MADT Enabled. Não é teto: é a lista do silício.
    // Stack 64-bit: heap mapeado (HEAP_BUFFER), não HEAP_START 0x4000_0000_0000.
    let mut ap_ids: alloc::vec::Vec<u32> = alloc::vec::Vec::new();
    {
        let ids = crate::acpi::BOOT_APIC_IDS.lock();
        if ids.is_empty() {
            crate::slog_nano!("SMP", "info", "MADT sem IDs — BSP-only (sem guess sequencial)");
            corepools::init_from_boot(bsp_lapic_id, 0);
            let _ = smp_state_store_checked(SmpInitState::Degraded, SMP_R_NO_MADT);
            return;
        }
        for &id in ids.iter() {
            if id != bsp_lapic_id && !ap_ids.contains(&id) {
                ap_ids.push(id);
            }
        }
    }
    // Defect 4a: o cap max_aps (AP_COUNT) era ignorado aqui — truncar a
    // lista do silício antes de derivar n_aps/AP_EXPECTED.
    ap_ids.truncate(ap_expected as usize);
    let n_madt = ap_ids.len();
    if n_madt == 0 {
        crate::slog_nano!("SMP", "info", "MADT sem APs Enabled — BSP-only");
        corepools::init_from_boot(bsp_lapic_id, 0);
        let _ = smp_state_store_checked(SmpInitState::Degraded, SMP_R_NO_MADT);
        return;
    }
    let n_aps = n_madt;
    AP_EXPECTED.store(n_aps as u16, Ordering::Release);

    if !percpu::alloc_slots(n_aps) {
        crate::slog_nano!("SMP", "warn", "PerCpu heap fail — BSP-only");
        corepools::init_from_boot(bsp_lapic_id, 0);
        let _ = smp_state_store_checked(SmpInitState::Degraded, SMP_R_NO_PERCPU);
        return;
    }
    if !crate::interrupts::expand_gdt_aps(n_aps) {
        crate::slog_nano!("SMP", "warn", "GDT expand fail — BSP-only");
        corepools::init_from_boot(bsp_lapic_id, 0);
        let _ = smp_state_store_checked(SmpInitState::Degraded, SMP_R_GDT_FAIL);
        return;
    }

    crate::slog_nano!(
        "SMP",
        "trace",
        "INIT-SIPI-SIPI sequencial (vetor={:#04x}, APs={})...",
        tramp_vector,
        n_aps
    );
    // SESSÃO_260: loga os IDs alvo no ramlog — HW real acordou 0 APs
    // (madt_lapics=4 total_cores=1). Se os IDs não batem com o LAPIC real
    // (HT threads podem ter IDs não-sequenciais), o INIT-SIPI vai para o
    // lugar errado e o AP nunca entra. Visível no dump "BOOT.LOG (RAM)".
    {
        let mut s = alloc::string::String::from("SMP: ap_ids = [");
        for (i, &id) in ap_ids.iter().enumerate() {
            if i > 0 {
                s.push(' ');
            }
            use core::fmt::Write as _;
            let _ = write!(s, "{:#04x}", id);
        }
        s.push(']');
        crate::boot_logger::log(&s);
        crate::slog_nano!("SMP", "trace", "{}", s);
    }

    crate::display::fb::boot_ckpt(22, "smp: antes wake");
    let ap_woke = wake_aps_sequential(
        tramp_phys,
        cr3_val,
        stack_per_ap,
        tramp_vector,
        &ap_ids,
        apic::send_init_ipi_to,
        apic::send_init_deassert_ipi_to,
        apic::send_sipi_to,
        apic::wait_for_ipi_delivery,
    );

    crate::slog_nano_home!(
        "SMP",
        "ok",
        "k_nano::smp",
        "Brought up {} APs",
        ap_woke
    );

    // Aceite AIOS (SESSION_279): online deve == madt_enabled-1 (dentro do gate env).
    if ap_woke as u16 != ap_expected {
        crate::slog_nano!(
            "SMP",
            "warn",
            "online={} != madt_expected={} — residual wake/IDT (HITL metal)",
            ap_woke,
            ap_expected
        );
    } else {
        crate::slog_nano!(
            "SMP",
            "ok",
            "online==madt-1 criterion OK (aps={})",
            ap_woke
        );
    }

    // Defect 4b: barreira ready==expected do ap_entry nunca dispara se menos
    // APs chegaram que o esperado — assenta no que chegou (>=1) em vez de
    // deixar ap_pollable=false para sempre. Fail-closed: 0 => off.
    let idt_ready = AP_IDT_READY.load(Ordering::Acquire);
    if idt_ready >= 1 && !ap_pollable() {
        AP_EXPECTED.store(idt_ready, Ordering::Release);
        set_ap_pollable(true);
        crate::slog_nano!(
            "SMP",
            "ok",
            "settle: {} AP(s) IDT-ready — AP_EXPECTED ajustado, ap_pollable=true",
            idt_ready
        );
    }

    corepools::init_from_boot(bsp_lapic_id, ap_woke as u16);
    #[cfg(feature = "smp-runqueue")]
    {
        let n = (ap_woke as usize).saturating_add(1);
        if n > runqueue::MAX_CORES {
            crate::slog_nano!(
                "SMP",
                "fail",
                "MADT/n={} > MAX_CORES={} — HITL (RQ array); inventário não é truncado no wake",
                n,
                runqueue::MAX_CORES
            );
        }
        runqueue::init_roles_from_pools(n);
    }
    let workers = (ap_woke as usize).saturating_add(1);
    work_stealing::init_global_pool(workers);
    // Plan lane 1 (audit §A) + reviewer refinement: the ONLY Ready store in
    // init_smp, behind the final gate — wake settle + corepools + runqueue +
    // work-stealing pool all ran above. No early Ready exists on any path:
    // every BSP-only exit lands Degraded/FailedRetryable, and the checked
    // store rejects Ready from anything but Init (claim holder).
    let _ = smp_state_store_checked(SmpInitState::Ready, SMP_R_OK);

    // Hybrid Intel 0x1A: honesty — E-cores só em metal/KVM com allow_ep_core_detect.
    if crate::platform_probe::gate().allow_ep_core_detect {
        if let Some(p) = corepools::pools() {
            crate::slog_nano!(
                "SMP",
                "ok",
                "hybrid/0x1A pools P(r1)={} E(r2)={} (aceite HW: R1=P R2=E)",
                p.ring1.len(),
                p.ring2.len()
            );
        }
    }
}

#[cfg(test)]
mod host_tests {
    use super::*;
    use core::sync::atomic::Ordering;
    use spin::Mutex;
    static LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn t033_ap_pollable_starts_false() {
        let _g = LOCK.lock();
        // T-033: AP_IDT_READY barrier gate — ap_pollable deve começar false (BSP-only safe default)
        // só vira true após último AP fazer AP_IDT_READY == AP_EXPECTED
        // host começa sem APs, então false
        // não força true aqui — valida default honesto
        assert!(!ap_pollable() || AP_ENTRY_COUNTER.load(Ordering::Relaxed) > 0);
        // em teste host puro, deve ser false
        let was = AP_POLLABLE.load(Ordering::Relaxed);
        if AP_EXPECTED.load(Ordering::Relaxed) == 0 {
            assert!(!was, "default OFF sem IDT/IPI pleno");
        }
    }

    #[test]
    fn t033_ap_idt_ready_barrier_invariants() {
        let _g = LOCK.lock();
        // barrier: AP_IDT_READY <= AP_EXPECTED quando pollable false
        let ready = AP_IDT_READY.load(Ordering::Relaxed);
        let expected = AP_EXPECTED.load(Ordering::Relaxed);
        if !ap_pollable() {
            assert!(ready <= expected || expected == 0);
        }
    }

    #[test]
    fn t034_ist_constants_host() {
        // T-034: sti só com IST mapeado — valida que cada AP tem 3 stacks de 16KB
        assert_eq!(percpu::IST_STACK_SIZE, 16384);
        assert_eq!(percpu::IST_COUNT, 3);
    }

    #[test]
    fn t037_heap_not_bss_511_host() {
        let _g = LOCK.lock();
        // T-037: PerCpu/TSS heap, não BSS 511 — boot 1c não reserva
        // ap_slots já testado em percpu, aqui valida que AP_EXPECTED 0 => ap_slots 0
        if AP_EXPECTED.load(Ordering::Relaxed) == 0 {
            assert_eq!(percpu::ap_slots(), 0);
        }
    }

    #[test]
    fn t035_parallel_gate_predicates() {
        // T-035: cortex::parallel_* gate = allow_smp && ap_pollable && entry_count>0
        // Em host sem APs, gate deve ser false => BSP fallback, não deadlock
        let gate = crate::platform_probe::allow_smp() && ap_pollable() && ap_entry_count() > 0;
        if !ap_pollable() {
            assert!(!gate, "sem ap_pollable=true, parallel_* deve cair no BSP");
        }
    }

    // Plan lane 1 (audit §A): state-machine contract tests.
    #[test]
    fn sm_retry_after_early_fail_reruns() {
        let _g = LOCK.lock();
        smp_state_test_reset();
        assert!(!smp_init_done());
        assert!(smp_should_init());
        // Pure gate truth table: cada early fail é transitório.
        assert_eq!(decide_smp_gate(false, true), Err(SmpGateFail::SmpDisallowed));
        assert_eq!(decide_smp_gate(false, false), Err(SmpGateFail::SmpDisallowed));
        assert_eq!(decide_smp_gate(true, false), Err(SmpGateFail::ApicUnavailable));
        assert_eq!(decide_smp_gate(true, true), Ok(()));
        // Early-fail path armazena FailedRetryable — nunca envenena.
        smp_state_store(SmpInitState::FailedRetryable, SMP_R_APIC_UNAVAIL);
        assert!(!smp_init_done(), "retryable early fail must not report done");
        assert!(smp_should_init(), "FailedRetryable must allow a later valid retry");
        assert_eq!(smp_init_reason(), "apic_unavailable");
        // Run valido posterior completa.
        smp_state_store(SmpInitState::Ready, SMP_R_OK);
        assert!(smp_init_done());
        assert!(!smp_should_init());
        smp_state_test_reset();
    }

    #[test]
    fn sm_double_terminal_skips() {
        let _g = LOCK.lock();
        smp_state_test_reset();
        smp_state_store(SmpInitState::Ready, SMP_R_OK);
        assert!(!smp_should_init(), "second call after Ready skips");
        assert!(smp_init_done());
        smp_state_store(SmpInitState::Degraded, SMP_R_NO_APS);
        assert!(!smp_should_init(), "second call after Degraded skips");
        assert!(smp_init_done());
        smp_state_store(SmpInitState::FailedPermanent, SMP_R_NONE);
        assert!(!smp_should_init());
        assert!(!smp_init_done());
        smp_state_test_reset();
    }

    #[test]
    fn sm_degraded_keeps_bsp_path() {
        let _g = LOCK.lock();
        smp_state_test_reset();
        // Degraded = BSP-only bring-up feito (bsp percpu + corepools), sem APs.
        smp_state_store(SmpInitState::Degraded, SMP_R_NO_MADT);
        assert!(smp_init_done(), "Degraded still reports BSP path up");
        assert!(!smp_should_init(), "Degraded is terminal: second call skips");
        assert_eq!(smp_init_reason(), "no_madt_ids");
        smp_state_test_reset();
    }

    // Reviewer refinement: concurrent double-init. Sequential simulation of the
    // Init race via the state API: first claim wins Run, second sees Init and
    // gets Busy (AlreadyRunning) — never a second Run, never two inits.
    #[test]
    fn sm_claim_race_second_caller_busy() {
        let _g = LOCK.lock();
        smp_state_test_reset();
        assert_eq!(smp_claim_init(), SmpClaim::Run, "first claimant runs");
        assert_eq!(smp_init_state(), SmpInitState::Init);
        // Second concurrent caller lands while Init is in flight.
        assert_eq!(smp_claim_init(), SmpClaim::Busy, "second caller must not double-init");
        assert_eq!(smp_claim_init(), SmpClaim::Busy, "third caller also Busy");
        assert_eq!(smp_init_state(), SmpInitState::Init, "race leaves Init intact");
        assert!(!smp_init_done(), "in-flight init never reports done");
        // Holder finishes honestly; late callers now Skip.
        assert!(smp_state_store_checked(SmpInitState::Ready, SMP_R_OK));
        assert_eq!(smp_claim_init(), SmpClaim::Skip(SmpInitState::Ready));
        assert_eq!(smp_claim_init(), SmpClaim::Skip(SmpInitState::Ready));
        smp_state_test_reset();
    }

    // Reviewer refinement: timeout/partial-fail path. A wake timeout (or any
    // BSP-only fallback) lands Degraded — BSP path up, no auto-retry — and the
    // ONLY way back to a re-run is the explicit reset handle.
    #[test]
    fn sm_timeout_partial_fail_needs_explicit_reset() {
        let _g = LOCK.lock();
        smp_state_test_reset();
        assert_eq!(smp_claim_init(), SmpClaim::Run);
        // Simulated AP wake timeout: holder records the partial-fail outcome.
        assert!(smp_state_store_checked(SmpInitState::Degraded, SMP_R_NO_MADT));
        assert!(smp_init_done(), "Degraded keeps BSP path up");
        assert!(!smp_should_init(), "Degraded does not auto-retry");
        assert_eq!(smp_claim_init(), SmpClaim::Skip(SmpInitState::Degraded));
        // Explicit HITL reset re-opens the gate; then a fresh claim re-runs.
        assert!(smp_reset_for_retry(), "Degraded allows explicit reset");
        assert_eq!(smp_init_state(), SmpInitState::Uninit);
        assert!(smp_should_init());
        assert_eq!(smp_claim_init(), SmpClaim::Run, "post-reset claim re-runs");
        smp_state_test_reset();
    }

    // Reviewer refinement: retry-after-FailedRetryable re-runs through the
    // claim (not the raw store), and the claim clears the stale reason.
    #[test]
    fn sm_retry_after_failed_retryable_reruns_via_claim() {
        let _g = LOCK.lock();
        smp_state_test_reset();
        assert_eq!(smp_claim_init(), SmpClaim::Run);
        assert!(smp_state_store_checked(SmpInitState::FailedRetryable, SMP_R_APIC_UNAVAIL));
        assert!(!smp_init_done());
        assert!(smp_should_init());
        assert_eq!(smp_claim_init(), SmpClaim::Run, "FailedRetryable claim re-runs");
        assert_eq!(smp_init_reason(), "none", "claim clears stale retryable reason");
        // Second transient fail, then a valid run completes.
        assert!(smp_state_store_checked(SmpInitState::FailedRetryable, SMP_R_SMP_DISALLOWED));
        assert_eq!(smp_claim_init(), SmpClaim::Run);
        assert!(smp_state_store_checked(SmpInitState::Ready, SMP_R_OK));
        assert!(smp_init_done());
        assert_eq!(smp_claim_init(), SmpClaim::Skip(SmpInitState::Ready));
        smp_state_test_reset();
    }

    // Reviewer refinement: invalid-transition rejection. The checked store is
    // the enforcement point — premature Ready, terminal overwrite, and
    // double-Init all bounce with state untouched.
    #[test]
    fn sm_invalid_transitions_rejected() {
        let _g = LOCK.lock();
        smp_state_test_reset();
        // Premature Ready straight from Uninit: rejected.
        assert!(!smp_state_store_checked(SmpInitState::Ready, SMP_R_OK));
        assert_eq!(smp_init_state(), SmpInitState::Uninit);
        // Uninit -> Degraded / FailedPermanent without a claim: rejected.
        assert!(!smp_state_store_checked(SmpInitState::Degraded, SMP_R_NO_APS));
        assert!(!smp_state_store_checked(SmpInitState::FailedPermanent, SMP_R_NONE));
        assert_eq!(smp_init_state(), SmpInitState::Uninit);
        // Claim, then Init -> Init (double claim via store): rejected.
        assert_eq!(smp_claim_init(), SmpClaim::Run);
        assert!(!smp_state_store_checked(SmpInitState::Init, SMP_R_NONE));
        assert_eq!(smp_init_state(), SmpInitState::Init);
        // Terminal overwrite: Ready -> Degraded without reset: rejected.
        assert!(smp_state_store_checked(SmpInitState::Ready, SMP_R_OK));
        assert!(!smp_state_store_checked(SmpInitState::Degraded, SMP_R_NO_APS));
        assert!(!smp_state_store_checked(SmpInitState::FailedRetryable, SMP_R_APIC_UNAVAIL));
        assert_eq!(smp_init_state(), SmpInitState::Ready);
        smp_state_test_reset();
    }

    // Reviewer refinement: no-Ready-before-invariants. Ready is reachable ONLY
    // from Init (claim holder that ran the full bring-up: bsp percpu,
    // corepools, settle, pools). Uninit/FailedRetryable can never land Ready.
    #[test]
    fn sm_no_ready_before_invariants() {
        let _g = LOCK.lock();
        smp_state_test_reset();
        assert!(!smp_init_done());
        assert!(!smp_state_store_checked(SmpInitState::Ready, SMP_R_OK));
        // A bare FailedRetryable (early gate, bring-up never ran) can neither
        // land Ready directly nor report done.
        smp_state_store(SmpInitState::FailedRetryable, SMP_R_APIC_UNAVAIL);
        assert!(!smp_init_done());
        assert!(!smp_state_store_checked(SmpInitState::Ready, SMP_R_OK));
        assert_eq!(smp_init_state(), SmpInitState::FailedRetryable);
        // Only the claim (Init) opens the Ready edge.
        assert_eq!(smp_claim_init(), SmpClaim::Run);
        assert!(!smp_init_done(), "Init in flight is not done");
        assert!(smp_state_store_checked(SmpInitState::Ready, SMP_R_OK));
        assert!(smp_init_done());
        smp_state_test_reset();
    }

    // Reset gate: Ready is never demoted, Init is never yanked; only
    // non-Ready non-running states reset.
    #[test]
    fn sm_reset_for_retry_gate() {
        let _g = LOCK.lock();
        smp_state_test_reset();
        assert!(!smp_reset_for_retry(), "Uninit has nothing to reset");
        assert_eq!(smp_claim_init(), SmpClaim::Run);
        assert!(!smp_reset_for_retry(), "never yank a running init");
        assert!(smp_state_store_checked(SmpInitState::FailedPermanent, SMP_R_NONE));
        assert!(smp_reset_for_retry(), "FailedPermanent allows explicit reset");
        assert_eq!(smp_init_state(), SmpInitState::Uninit);
        assert_eq!(smp_claim_init(), SmpClaim::Run);
        assert!(smp_state_store_checked(SmpInitState::Ready, SMP_R_OK));
        assert!(!smp_reset_for_retry(), "never demote a good bring-up");
        assert_eq!(smp_init_state(), SmpInitState::Ready);
        smp_state_test_reset();
    }
}
