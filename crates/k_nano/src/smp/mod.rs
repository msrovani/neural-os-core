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
use crate::println;
use core::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, AtomicUsize, Ordering};
use spin::Mutex;

static AP_BOOT_LOCK: Mutex<()> = Mutex::new(());
static AP_ENTRY_COUNTER: AtomicU64 = AtomicU64::new(0);

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
    crate::slog_nano!("SMP", "info", "AP {} entrou em modo 64-bit Rust!", cpu_id);
    println!("[SMP] AP {} entrou em modo 64-bit Rust!", cpu_id);
    drop(_lock);

    unsafe {
        let base = crate::apic::LAPIC_VIRT_BASE.load(Ordering::Acquire);
        if base > 0 {
            let svr = core::ptr::read_volatile((base + 0xF0) as *const u32);
            core::ptr::write_volatile(
                (base + 0xF0) as *mut u32,
                (svr & 0xFFFFFF00) | 0xFF | 0x100,
            );
            core::ptr::write_volatile((base + 0x80) as *mut u32, 0u32);
            apic::apic_eoi();
        }
    }

// P13: Initialize per-AP TSS/IST and load IDT
    let ap_index = (cpu_id - 1) as usize;
    let ist_tops = unsafe { percpu::init_ap_ist(ap_index) };
    // Convert u64 stack tops to VirtAddr
    let ist_tops_va = [
        x86_64::VirtAddr::new(ist_tops[0]),
        x86_64::VirtAddr::new(ist_tops[1]),
        x86_64::VirtAddr::new(ist_tops[2]),
    ];
    let ap_tss = crate::interrupts::init_ap_tss(ap_index, ist_tops_va);
    unsafe {
        if ap_index < percpu::MAX_APS {
            let pcpu = percpu::AP_PCPU.0[ap_index].get();
            (*pcpu).tss_ptr = &ap_tss.tss as *const _ as u64;
            (*pcpu).lapic_id = apic::lapic_id();
            (*pcpu).cpu_type = match corepools::detect_core_type() {
                corepools::CoreType::Efficiency => percpu::CPU_TYPE_E_CORE,
                _ => percpu::CPU_TYPE_P_CORE,
            };
        }
    }
    // Load IDT + TSS + enable interrupts
    unsafe { crate::interrupts::ap_load_idt_and_tss(ap_tss.selector); }

    // Signal IDT ready
    let ready = AP_IDT_READY.fetch_add(1, Ordering::SeqCst) + 1;
    let expected = AP_EXPECTED.load(Ordering::Acquire);
    if ready == expected {
        // Last AP — enable AP workers
        set_ap_pollable(true);
        crate::slog_nano!("SMP", "info", "All {} APs IDT ready — ap_pollable=true", expected);
    }

    AP_ENTRY_COUNTER.fetch_add(1, Ordering::SeqCst);
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
/// `ap_stack_base` = base da região reservada; o AP `i` recebe topo
/// `ap_stack_base + (i+1) * stack_per_ap`. Retorna quantos APs subiram.
#[allow(clippy::too_many_arguments)]
pub unsafe fn wake_aps_sequential(
    tramp_phys: u64,
    cr3_val: u64,
    ap_stack_base: u64,
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
        if i >= percpu::MAX_APS {
            break;
        }
        if dest > 0xFF && !apic::USING_X2APIC.load(Ordering::Relaxed) {
            crate::boot_logger::log(&alloc::format!(
                "SMP: AP {:#x} skip — precisa x2APIC (id>255)",
                dest
            ));
            continue;
        }
        let ap_stack = ap_stack_base + ((i as u64) + 1) * stack_per_ap;
        let percpu_addr = percpu::ap_percpu_ptr(i);

        // Sequencial: apenas 1 AP no trampoline por vez → seguro re-patch o blob
        // (stacks real/32b compartilhadas do trampoline não colidem).
        trampoline::init_trampoline(tramp_phys, cr3_val, ap_stack, percpu_addr, ap_entry);

        let before = AP_ENTRY_COUNTER.load(Ordering::Acquire);
        // ADR-0057 WS-F: retry INIT-SIPI-SIPI (até 3x). Firmware real também
        // repete; robustez contra jitter de agendamento (ex.: TCG) onde o AP
        // pode demorar a receber ciclos e estourar um timeout curto.
        let mut ok = false;
        let mut last_hit = 0u64;
        let mut last_ready = 0u64;
        'attempts: for attempt in 0..3 {
            trampoline::clear_handshake(tramp_phys);
            // SESSÃO_262: sequência canônica Linux — INIT assert → ~10ms →
            // INIT deassert → ~10ms → SIPI → 200µs → SIPI. Sem o deassert,
            // CPUs reais (Kaby Lake) mantêm o AP em wait-for-SIPI travado.
            send_init_to(dest);
            wait_delivery();
            busy_wait_us(10000);
            send_init_deassert_to(dest);
            wait_delivery();
            busy_wait_us(10000);
            send_sipi_to(dest, tramp_vector);
            wait_delivery();
            busy_wait_us(200);
            send_sipi_to(dest, tramp_vector);
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

pub unsafe fn init_smp() {
    crate::slog_nano!("SMP", "info", "Inicializando SMP...");
    println!("[SMP] Inicializando SMP...");

    if !crate::platform_probe::allow_smp() {
        crate::slog_nano!(
            "SMP",
            "info",
            "BSP-only (FeatureGate allow_smp=false hv={})",
            crate::platform_probe::hypervisor().name()
        );
        println!("[SMP] FeatureGate: SMP disabled for this environment.");
        let bsp = apic::lapic_id();
        percpu::init_bsp_percpu(bsp);
        corepools::init_from_boot(bsp, 0);
        return;
    }

    if !apic::USING_APIC.load(Ordering::Relaxed) {
        crate::slog_nano!("SMP", "info", "APIC nao disponivel — SMP ignorado.");
        println!("[SMP] APIC nao disponivel — SMP ignorado.");
        return;
    }

    let cr3_val = {
        let (frame, _) = x86_64::registers::control::Cr3::read();
        frame.start_address().as_u64()
    };

    let bsp_lapic_id = apic::lapic_id();
    percpu::init_bsp_percpu(bsp_lapic_id);
    crate::slog_nano!(
        "SMP",
        "info",
        "BSP PerCpu inicializado. LAPIC ID: {}",
        bsp_lapic_id
    );
    println!("[SMP] BSP PerCpu inicializado.");

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
            "info",
            "Nenhum AP detectado (MADT). SMP single-core."
        );
        println!("[SMP] Sem APs — modo single-core.");
        corepools::init_from_boot(bsp_lapic_id, 0);
        return;
    }

    let tramp_phys = {
        let mut guard = memory::GLOBAL_ALLOCATOR.lock();
        let Some(alloc) = guard.as_mut() else {
            crate::slog_nano!("SMP", "warn", "sem frame alloc — BSP-only");
            corepools::init_from_boot(bsp_lapic_id, 0);
            return;
        };
        match alloc.allocate_below_1mb() {
            Some(frame) => frame.start_address().as_u64(),
            None => {
                drop(guard);
                crate::slog_nano!("SMP", "warn", "sem lowmem tramp — BSP-only");
                corepools::init_from_boot(bsp_lapic_id, 0);
                return;
            }
        }
    };
    crate::slog_nano!("SMP", "info", "Trampoline page em 0x{:x}", tramp_phys);
    crate::display::fb::boot_ckpt(22, "smp: tramp ok");

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
        return;
    }

    // ADR-0057 WS-A: wake sequencial por LAPIC ID (directed IPI). Substitui o
    // broadcast "all excl self" + stack/PerCpu compartilhados que só acordava
    // 1 AP. Vale inclusive em bare-metal (o hang antigo era do shorthand).
    // Observe = MADT Enabled. Não é teto: é a lista do silício.
    // Sem cap de “fração do heap”: stack é custo por AP, não política de cores.
    let heap_top2 = crate::allocator::HEAP_START as u64 + crate::allocator::HEAP_SIZE as u64;
    let mut ap_ids: alloc::vec::Vec<u32> = alloc::vec::Vec::new();
    {
        let ids = crate::acpi::BOOT_APIC_IDS.lock();
        if ids.is_empty() {
            crate::slog_nano!("SMP", "info", "MADT sem IDs — BSP-only (sem guess sequencial)");
            corepools::init_from_boot(bsp_lapic_id, 0);
            return;
        }
        for &id in ids.iter() {
            if id != bsp_lapic_id {
                ap_ids.push(id);
            }
        }
    }
    let n_madt = ap_ids.len();
    if n_madt == 0 {
        crate::slog_nano!("SMP", "info", "MADT sem APs Enabled — BSP-only");
        corepools::init_from_boot(bsp_lapic_id, 0);
        return;
    }
    // Array estático PerCpu/TSS ainda é BSS (não é política de cores).
    if n_madt > percpu::MAX_APS {
        crate::slog_nano!(
            "SMP",
            "error",
            "MADT APs={} > BSS PerCpu[{}] — HITL: arrays devem virar Vec no boot",
            n_madt,
            percpu::MAX_APS
        );
        crate::slog_nano!("SMP", "warn", "truncando APs {} -> {} (BSS guard, HITL)", n_madt, percpu::MAX_APS);
        crate::display::fb::boot_ckpt(22, "smp: truncado HITL");
        ap_ids.truncate(percpu::MAX_APS);
    }
    let n_aps = ap_ids.len();
    AP_EXPECTED.store(n_aps as u16, Ordering::Release);
    // Guard: heap_top2 - (n+1)*stack pode colidir com FALCON3 989MB ou estourar (Core 7 hybrid n=16)
    // Usa checked_sub + fallback BSP-only não-bloqueante; evita wrap para 0x...ffff
    let needed = ((n_aps as u64) + 1) * stack_per_ap;
    let region_base = match heap_top2.checked_sub(needed) {
        Some(v) if v >= crate::allocator::HEAP_START as u64 + 64 * 1024 => v,
        _ => {
            crate::slog_nano!("SMP", "warn", "heap colisao: heap_top2={:#x} needed={:#x} n={} — truncando", heap_top2, needed, n_aps);
            crate::display::fb::boot_ckpt(22, "smp: heap colisao HITL");
            // tenta reduzir n para caber, senão BSP-only
            let avail = heap_top2.saturating_sub(crate::allocator::HEAP_START as u64 + 64 * 1024);
            let max_fit = (avail / stack_per_ap).saturating_sub(1) as usize;
            if max_fit == 0 || max_fit >= n_aps {
                crate::slog_nano!("SMP", "warn", "sem espaco para stacks — BSP-only");
                corepools::init_from_boot(bsp_lapic_id, 0);
                return;
            }
            crate::slog_nano!("SMP", "warn", "reduzindo APs {} -> {} para caber no heap", n_aps, max_fit);
            ap_ids.truncate(max_fit);
            let n2 = ap_ids.len();
            AP_EXPECTED.store(n2 as u16, Ordering::Release);
            heap_top2 - ((n2 as u64) + 1) * stack_per_ap
        }
    };

    crate::slog_nano!(
        "SMP",
        "info",
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
        crate::slog_nano!("SMP", "info", "{}", s);
    }

    crate::display::fb::boot_ckpt(22, "smp: antes wake");
    let ap_woke = wake_aps_sequential(
        tramp_phys,
        cr3_val,
        region_base,
        stack_per_ap,
        tramp_vector,
        &ap_ids,
        apic::send_init_ipi_to,
        apic::send_init_deassert_ipi_to,
        apic::send_sipi_to,
        apic::wait_for_ipi_delivery,
    );

    crate::slog_nano!("SMP", "info", "APs acordados: {}", ap_woke);
    println!("[SMP] INIT-SIPI-SIPI concluido. APs={}", ap_woke);

    corepools::init_from_boot(bsp_lapic_id, ap_woke as u16);
    let workers = (ap_woke as usize).saturating_add(1);
    work_stealing::init_global_pool(workers);
}
