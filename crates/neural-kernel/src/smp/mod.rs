//! SMP bring-up — ADR-0055; trampoline via k_nano quando FeatureGate.allow_smp.

pub mod percpu;
pub mod trampoline;
pub mod spsc;
pub mod work_stealing;
pub mod parallel_matmul;

use crate::apic;
use crate::memory;
use core::sync::atomic::Ordering;
use x86_64::structures::paging::{
    Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB,
};
use x86_64::{PhysAddr, VirtAddr};

pub static AP_COUNT: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

#[allow(dead_code)]
const AP_STACK_SIZE: u64 = 16384;

// ADR-0057 WS-A: `ap_entry` e o `AP_ENTRY_COUNTER` agora vivem SÓ em
// `k_nano::smp` (emagrece o bin + unifica o contador que o `parallel_matmul`
// lê). O bin apenas delega a leitura.
pub fn ap_entry_count() -> u64 {
    k_nano::smp::ap_entry_count()
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

    // ADR-0057 WS-A: stack por-AP (não mais um único `stack_64_top`).
    let heap_top = crate::allocator::HEAP_START as u64 + crate::allocator::HEAP_SIZE as u64;
    let stack_per_ap: u64 = AP_STACK_SIZE * 4;

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

    // Trampoline é (re)patchado por AP dentro de `wake_aps_sequential` (k_nano).
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

    // ADR-0057 WS-A: wake sequencial por LAPIC ID (directed IPI). Substitui o
    // broadcast "all excl self" + stack/PerCpu compartilhados que só acordava 1
    // AP. Delega a `k_nano::smp` (ecossistema) passando o APIC vivo do bin por
    // fn-pointer — assim `k_nano::smp::ap_entry` é o único entry e unifica o
    // contador que o `parallel_matmul` consulta.
    let n_aps = (ap_expected as usize).min(k_nano::smp::percpu::MAX_APS);
    let region_base = heap_top - ((n_aps as u64) + 1) * stack_per_ap;
    let mut ap_ids = [0u8; k_nano::smp::percpu::MAX_APS];
    for i in 0..n_aps {
        ap_ids[i] = bsp_lapic_id.wrapping_add((i as u8) + 1);
    }

    crate::display::fb::boot_ckpt(22, "smp: INIT-SIPI seq");
    k_nano::slog_nano!(
        "SMP",
        "info",
        "INIT-SIPI-SIPI sequencial (vetor={:#04x}, APs={})...",
        tramp_vector,
        n_aps
    );

    let ap_woke = k_nano::smp::wake_aps_sequential(
        tramp_phys,
        cr3_val,
        region_base,
        stack_per_ap,
        tramp_vector,
        &ap_ids[..n_aps],
        apic::send_init_ipi_to,
        apic::send_sipi_to,
        apic::wait_for_ipi_delivery,
    );

    crate::display::fb::boot_ckpt(23, "smp: sipi done");
    k_nano::slog_nano!("SMP", "info", "APs acordados: {}", ap_woke);
    k_nano::smp::corepools::init_from_boot(bsp_lapic_id, ap_woke.min(255) as u8);
    let workers = (ap_woke as usize).saturating_add(1).min(8);
    k_nano::smp::work_stealing::init_global_pool(workers);
}
