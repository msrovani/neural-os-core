//! SMP bring-up — ADR-0055; trampoline via k_nano quando FeatureGate.allow_smp.

pub mod percpu;
pub mod trampoline;
pub mod spsc;
pub mod work_stealing;
pub mod parallel_matmul;

use crate::apic;
use crate::memory;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;
use x86_64::structures::paging::{
    Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB,
};
use x86_64::{PhysAddr, VirtAddr};

static AP_BOOT_LOCK: Mutex<()> = Mutex::new(());
static AP_ENTRY_COUNTER: AtomicU64 = AtomicU64::new(0);

pub static AP_COUNT: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

#[allow(dead_code)]
const AP_STACK_SIZE: u64 = 16384;

extern "C" fn ap_entry(_cpu_id: u64) -> ! {
    let _lock = AP_BOOT_LOCK.lock();
    let cpu_id = percpu::CPU_COUNT.fetch_add(1, Ordering::SeqCst);
    k_nano::slog_nano!("SMP", "info", "AP {} entrou em modo 64-bit Rust!", cpu_id);
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

    AP_ENTRY_COUNTER.fetch_add(1, Ordering::SeqCst);
    k_nano::smp::ap_work::ap_idle_loop(cpu_id as usize);
}

pub fn ap_entry_count() -> u64 {
    AP_ENTRY_COUNTER.load(Ordering::Relaxed)
}

fn busy_wait_us(us: u64) {
    for _ in 0..us * 40 {
        core::hint::spin_loop();
    }
}

fn finish_bsp_only(bsp_lapic_id: u8, reason: &str) {
    k_nano::slog_nano!("SMP", "info", "BSP-only ({})", reason);
    crate::display::fb::boot_ckpt(23, reason);
    k_nano::smp::corepools::init_from_boot(bsp_lapic_id, 0);
}

pub unsafe fn init_smp() {
    crate::display::fb::boot_ckpt(22, "smp: check apic");

    if !k_nano::platform_probe::allow_smp() {
        k_nano::slog_nano!(
            "SMP",
            "info",
            "BSP-only (FeatureGate hv={})",
            k_nano::platform_probe::hypervisor().name()
        );
        if apic::USING_APIC.load(Ordering::Relaxed) {
            let bsp = apic::lapic_id();
            percpu::init_bsp_percpu(bsp);
            finish_bsp_only(bsp, "smp: gate bsp-only");
        } else {
            crate::display::fb::boot_ckpt(23, "smp: gate no-apic");
        }
        return;
    }

    if !apic::USING_APIC.load(Ordering::Relaxed) {
        k_nano::slog_nano!("SMP", "info", "APIC nao disponivel — SMP ignorado.");
        crate::display::fb::boot_ckpt(23, "smp: no apic");
        return;
    }

    let ap_expected = AP_COUNT.load(Ordering::Relaxed);
    if !trampoline::sipi_ready() {
        k_nano::slog_nano!(
            "SMP",
            "info",
            "trampoline not ready — BSP only (MADT APs={})",
            ap_expected
        );
        let bsp = apic::lapic_id();
        percpu::init_bsp_percpu(bsp);
        finish_bsp_only(bsp, "smp: stub bsp-only");
        return;
    }

    crate::display::fb::boot_ckpt(22, "smp: sipi path");
    k_nano::slog_nano!("SMP", "info", "Inicializando SMP (SIPI)...");

    let cr3_val = {
        let (frame, _) = x86_64::registers::control::Cr3::read();
        frame.start_address().as_u64()
    };

    let bsp_lapic_id = apic::lapic_id();
    percpu::init_bsp_percpu(bsp_lapic_id);
    k_nano::slog_nano!(
        "SMP",
        "info",
        "BSP PerCpu inicializado. LAPIC ID: {}",
        bsp_lapic_id
    );

    let mut ap_expected = AP_COUNT.load(Ordering::Relaxed);
    let max_aps = k_nano::platform_probe::max_aps();
    if max_aps < 255 && ap_expected > max_aps {
        ap_expected = max_aps;
        AP_COUNT.store(ap_expected, Ordering::Relaxed);
    }

    if ap_expected == 0 {
        finish_bsp_only(bsp_lapic_id, "smp: no MADT APs");
        return;
    }

    crate::display::fb::boot_ckpt(22, "smp: alloc tramp");
    let tramp_phys = {
        let mut guard = memory::GLOBAL_ALLOCATOR.lock();
        let Some(alloc) = guard.as_mut() else {
            finish_bsp_only(bsp_lapic_id, "smp: no frame alloc");
            return;
        };
        match alloc.allocate_below_1mb() {
            Some(frame) => frame.start_address().as_u64(),
            None => {
                drop(guard);
                finish_bsp_only(bsp_lapic_id, "smp: no lowmem tramp");
                return;
            }
        }
    };
    k_nano::slog_nano!("SMP", "info", "Trampoline page em 0x{:x}", tramp_phys);

    let heap_top = crate::allocator::HEAP_START as u64 + crate::allocator::HEAP_SIZE as u64;
    let cpu_total = percpu::CPU_COUNT.load(Ordering::Relaxed) as u64 + 1;
    let stack_per_ap: u64 = AP_STACK_SIZE * 4;
    let ap_base = heap_top - (cpu_total * stack_per_ap);
    let stack_64_top =
        ap_base + (percpu::CPU_COUNT.load(Ordering::Relaxed) as u64) * stack_per_ap;

    // Identity-map o físico do trampoline (não hardcode 0x40000 — UEFI reserva lowmem).
    crate::display::fb::boot_ckpt(22, "smp: map tramp");
    {
        let phys_offset = memory::PHYS_MEM_OFFSET.load(Ordering::Acquire);
        let (l4_frame, _) = x86_64::registers::control::Cr3::read();
        let phys = l4_frame.start_address();
        let virt = VirtAddr::new(phys_offset) + phys.as_u64();
        let page_table_ptr: *mut PageTable = virt.as_mut_ptr();
        let page_table = &mut *page_table_ptr;
        let mut mapper = OffsetPageTable::new(page_table, VirtAddr::new(phys_offset));

        let page = Page::<Size4KiB>::containing_address(VirtAddr::new(tramp_phys));
        let frame = PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(tramp_phys));
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

        let mut guard = crate::memory::GLOBAL_ALLOCATOR.lock();
        let Some(allocator) = guard.as_mut() else {
            drop(guard);
            finish_bsp_only(bsp_lapic_id, "smp: map no alloc");
            return;
        };
        match mapper.map_to(page, frame, flags, &mut *allocator) {
            Ok(flush) => flush.flush(),
            Err(e) => {
                // Já mapeado (identity UEFI) — ok; outro erro → BSP-only
                let _ = e;
                k_nano::slog_nano!(
                    "SMP",
                    "warn",
                    "map_to tramp 0x{:x} falhou/ja-existe — tenta SIPI mesmo assim",
                    tramp_phys
                );
            }
        }
    }

    crate::display::fb::boot_ckpt(22, "smp: init tramp");
    let percpu_addr = percpu::BSP_PCPU.get() as u64;
    trampoline::init_trampoline(tramp_phys, cr3_val, stack_64_top, percpu_addr, ap_entry);
    let tsize = trampoline::trampoline_size();
    k_nano::slog_nano!(
        "SMP",
        "info",
        "Trampoline em 0x{:x} ({} bytes).",
        tramp_phys,
        tsize
    );

    let tramp_vector = (tramp_phys >> 12) as u8;
    if tramp_phys >= 0x100000 || tramp_vector == 0 {
        finish_bsp_only(bsp_lapic_id, "smp: bad tramp vector");
        return;
    }

    // HW real (foto 2026-07-19): hang em shorthand INIT-SIPI (all excl self).
    // Trampoline alloc/map/init OK — APs ficam deferred até SIPI por LAPIC ID (MADT).
    // QEMU/TCG/KVM ainda exercitam o path completo abaixo.
    if matches!(
        k_nano::platform_probe::hypervisor(),
        k_nano::platform_probe::HypervisorKind::None
    ) {
        k_nano::slog_nano!(
            "SMP",
            "info",
            "BareMetal: skip INIT-SIPI (shorthand hang) tramp=0x{:x} APs_madt={} — BSP-only",
            tramp_phys,
            ap_expected
        );
        finish_bsp_only(bsp_lapic_id, "smp: sipi skip baremetal");
        return;
    }

    crate::display::fb::boot_ckpt(22, "smp: INIT-SIPI");
    k_nano::slog_nano!(
        "SMP",
        "info",
        "INIT-SIPI-SIPI (vetor={:#04x})...",
        tramp_vector
    );

    crate::display::fb::boot_ckpt(22, "smp: send INIT");
    apic::send_init_ipi();
    apic::wait_for_ipi_delivery();
    busy_wait_us(10000);

    crate::display::fb::boot_ckpt(22, "smp: deassert");
    apic::send_init_deassert_ipi();
    apic::wait_for_ipi_delivery();
    busy_wait_us(200);

    crate::display::fb::boot_ckpt(22, "smp: SIPI#1");
    apic::send_sipi(tramp_vector);
    apic::wait_for_ipi_delivery();
    busy_wait_us(200);

    crate::display::fb::boot_ckpt(22, "smp: SIPI#2");
    apic::send_sipi(tramp_vector);
    apic::wait_for_ipi_delivery();
    busy_wait_us(50000);

    let ap_woke = AP_ENTRY_COUNTER.load(Ordering::Relaxed);
    crate::display::fb::boot_ckpt(23, "smp: sipi done");
    k_nano::slog_nano!("SMP", "info", "APs acordados: {}", ap_woke);
    k_nano::smp::corepools::init_from_boot(bsp_lapic_id, ap_woke.min(255) as u8);
    let workers = (ap_woke as usize).saturating_add(1).min(8);
    k_nano::smp::work_stealing::init_global_pool(workers);
}
