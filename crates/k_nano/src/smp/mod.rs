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
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use spin::Mutex;
use x86_64::structures::paging::{
    Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB,
};
use x86_64::{PhysAddr, VirtAddr};

static AP_BOOT_LOCK: Mutex<()> = Mutex::new(());
static AP_ENTRY_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Set by PlatformAgent before calling init_smp().
pub static AP_COUNT: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

/// ADR-0057 WS-F / ADR-0065 FASE 3.1 P13: Barrier for APs that have loaded IDT.
static AP_IDT_READY: AtomicU8 = AtomicU8::new(0);
/// Total APs expected to load IDT (set before wake).
static AP_EXPECTED: AtomicU8 = AtomicU8::new(0);

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
    // Store TSS selector in PerCpu for later use
    unsafe {
        let pcpu = percpu::AP_PCPU.0[ap_index].get();
        (*pcpu).tss_ptr = &ap_tss.tss as *const _ as u64;
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
    // SESSION integration: TSC calibrado (HPET→PIT→CPUID) em vez de spin fixo us*40.
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
    ap_lapic_ids: &[u8],
    send_init_to: unsafe fn(u8),
    send_init_deassert_to: unsafe fn(u8),
    send_sipi_to: unsafe fn(u8, u8),
    wait_delivery: unsafe fn(),
) -> u64 {
    let mut woke = 0u64;
    for (i, &dest) in ap_lapic_ids.iter().enumerate() {
        if i >= percpu::MAX_APS {
            break;
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
        'attempts: for attempt in 0..3 {
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

            // Espera ESTE AP sinalizar antes de acordar o próximo (~250 ms/try).
            for _ in 0..5000 {
                if AP_ENTRY_COUNTER.load(Ordering::Acquire) > before {
                    ok = true;
                    break 'attempts;
                }
                busy_wait_us(50);
            }
            if !ok {
                crate::boot_logger::log(&alloc::format!(
                    "SMP: AP {:#04x} tentativa {} timeout (counter={})",
                    dest, attempt, AP_ENTRY_COUNTER.load(Ordering::Acquire)
                ));
            }
        }
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
                "SMP: AP {:#04x} online ({}/{})",
                dest, woke, ap_lapic_ids.len()
            ));
        } else {
            crate::slog_nano!("SMP", "warn", "AP LAPIC {} timeout (nao subiu)", dest);
            // SESSÃO_262: distingue "trampoline não executa" (AP_ONLINE=0) de
            // "ap_entry trava no meio" (AP_ONLINE>0 mas counter=0). O AP_ONLINE
            // é incrementado na 1ª linha do ap_entry, antes de qualquer passo
            // que possa travar (TSS/IDT/EOI).
            crate::boot_logger::log(&alloc::format!(
                "SMP: AP {:#04x} timeout — AP_ONLINE={} counter={}",
                dest,
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
    if max_aps < 255 && ap_expected > max_aps {
        crate::slog_nano!(
            "SMP",
            "info",
            "cap APs {} → {} (FeatureGate.max_aps)",
            ap_expected,
            max_aps
        );
        ap_expected = max_aps;
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

    // ADR-0057 WS-A: stack por-AP (não mais um único `stack_64_top`).
    let stack_per_ap: u64 = AP_STACK_SIZE * 4;

    {
        let phys_offset = memory::PHYS_MEM_OFFSET.load(Ordering::Acquire);
        let (l4_frame, _) = x86_64::registers::control::Cr3::read();
        let phys = l4_frame.start_address();
        let virt = VirtAddr::new(phys_offset) + phys.as_u64();
        let page_table_ptr: *mut PageTable = virt.as_mut_ptr();
        let page_table = &mut *page_table_ptr;
        let mut mapper = OffsetPageTable::new(page_table, VirtAddr::new(phys_offset));

        // Identity-map do físico real (não 0x40000 fixo).
        let page = Page::<Size4KiB>::containing_address(VirtAddr::new(tramp_phys));
        let frame = PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(tramp_phys));
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

        let mut guard = crate::memory::GLOBAL_ALLOCATOR.lock();
        if let Some(allocator) = guard.as_mut() {
            match mapper.map_to(page, frame, flags, &mut *allocator) {
                Ok(flush) => flush.flush(),
                Err(_) => {
                    crate::slog_nano!(
                        "SMP",
                        "warn",
                        "map_to tramp 0x{:x} falhou/ja-existe",
                        tramp_phys
                    );
                }
            }
        }
    }

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
    let heap_top2 = crate::allocator::HEAP_START as u64 + crate::allocator::HEAP_SIZE as u64;
    let n_aps = (ap_expected as usize).min(percpu::MAX_APS);
    let region_base = heap_top2 - ((n_aps as u64) + 1) * stack_per_ap;
    // HW-5: Usa IDs reais do MADT em vez de guess sequencial
    let mut ap_ids = [0u8; percpu::MAX_APS];
    {
        let ids = crate::acpi::BOOT_APIC_IDS.lock();
        if ids.len() > 0 {
            let mut idx = 0;
            for &id in ids.iter() {
                if id as u8 != bsp_lapic_id && idx < n_aps {
                    ap_ids[idx] = id as u8;
                    idx += 1;
                }
            }
            // Se MADT deu menos IDs que esperado, complete com guess
            while idx < n_aps {
                ap_ids[idx] = bsp_lapic_id.wrapping_add((idx as u8) + 1);
                idx += 1;
            }
        } else {
            // Sem MADT — fallback guess sequencial (original)
            for i in 0..n_aps {
                ap_ids[i] = bsp_lapic_id.wrapping_add((i as u8) + 1);
            }
        }
    }

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
        for (i, &id) in ap_ids[..n_aps].iter().enumerate() {
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

    let ap_woke = wake_aps_sequential(
        tramp_phys,
        cr3_val,
        region_base,
        stack_per_ap,
        tramp_vector,
        &ap_ids[..n_aps],
        apic::send_init_ipi_to,
        apic::send_init_deassert_ipi_to,
        apic::send_sipi_to,
        apic::wait_for_ipi_delivery,
    );

    crate::slog_nano!("SMP", "info", "APs acordados: {}", ap_woke);
    println!("[SMP] INIT-SIPI-SIPI concluido. APs={}", ap_woke);

    corepools::init_from_boot(bsp_lapic_id, ap_woke.min(255) as u8);
    let workers = (ap_woke as usize).saturating_add(1).min(8);
    work_stealing::init_global_pool(workers);
}
