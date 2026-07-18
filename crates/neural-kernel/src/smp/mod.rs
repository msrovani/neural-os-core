pub mod percpu;

pub mod trampoline;

pub mod spsc;

pub mod work_stealing;

pub mod parallel_matmul;



use crate::apic;

use crate::memory;

use core::sync::atomic::{AtomicU64, Ordering};

use x86_64::structures::paging::{OffsetPageTable, PageTable, Mapper, Size4KiB, Page, PhysFrame, PageTableFlags};

use x86_64::VirtAddr;

use x86_64::PhysAddr;



use spin::Mutex;



static AP_BOOT_LOCK: Mutex<()> = Mutex::new(());

static AP_ENTRY_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Set by PlatformAgent before calling init_smp().

pub static AP_COUNT: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

#[allow(dead_code)]

const AP_STACK_SIZE: u64 = 16384;



extern "C" fn ap_entry(_cpu_id: u64) -> ! {

    let _lock = AP_BOOT_LOCK.lock();

    let cpu_id = percpu::CPU_COUNT.fetch_add(1, Ordering::SeqCst);

    k_nano::slog_nano!("SMP", "info", "AP {} entrou em modo 64-bit Rust!", cpu_id);

    drop(_lock);



    // Initialize AP's LAPIC — necessário para evitar GPF em spurious interrupts

    unsafe {

        let base = crate::apic::LAPIC_VIRT_BASE.load(Ordering::Acquire);

        if base > 0 {

            // SVR: vetor espúrio = 0xFF, bit 8 = enable

            let svr = core::ptr::read_volatile((base + 0xF0) as *const u32);

            core::ptr::write_volatile((base + 0xF0) as *mut u32, (svr & 0xFFFFFF00) | 0xFF | 0x100);

            core::ptr::write_volatile((base + 0x80) as *mut u32, 0u32); // TPR = 0

            apic::apic_eoi();

        }

    }



    AP_ENTRY_COUNTER.fetch_add(1, Ordering::SeqCst);



    loop {

        x86_64::instructions::hlt();

    }

}



pub fn ap_entry_count() -> u64 {

    AP_ENTRY_COUNTER.load(Ordering::Relaxed)

}



fn busy_wait_us(us: u64) {

    for _ in 0..us * 40 {

        core::hint::spin_loop();

    }

}



pub unsafe fn init_smp() {
    // Fast-path BSP-only ANTES de percpu/GS: trampoline e stub (SIPI morto).
    // Antes: init_bsp_percpu escrevia em `static` .rodata → #PF e tela parava em K20.
    crate::display::fb::boot_ckpt(22, "smp: check apic");

    if !apic::USING_APIC.load(Ordering::Relaxed) {
        k_nano::slog_nano!("SMP", "info", "APIC nao disponivel — SMP ignorado.");
        crate::display::fb::boot_ckpt(22, "smp: no apic");
        return;
    }

    let ap_expected = AP_COUNT.load(Ordering::Relaxed);
    if !trampoline::sipi_ready() {
        k_nano::slog_nano!("SMP", "info", "trampoline STUB — BSP only (MADT APs={})", ap_expected);
        crate::display::fb::boot_ckpt(23, "smp: stub bsp-only");
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
    k_nano::slog_nano!("SMP", "info", "BSP PerCpu inicializado. LAPIC ID: {}", bsp_lapic_id);

    if ap_expected == 0 {
        k_nano::slog_nano!("SMP", "info", "Nenhum AP detectado (MADT). SMP single-core.");
        return;
    }

    let tramp_phys = {
        let mut guard = memory::GLOBAL_ALLOCATOR.lock();
        let alloc = guard.as_mut().expect("Frame allocator not initialized");
        let frame = alloc
            .allocate_below_1mb()
            .expect("Failed to allocate trampoline page below 1 MB");
        frame.start_address().as_u64()
    };
    k_nano::slog_nano!("SMP", "info", "Trampoline page em 0x{:x}", tramp_phys);

    let heap_top = crate::allocator::HEAP_START as u64 + crate::allocator::HEAP_SIZE as u64;
    let cpu_total = percpu::CPU_COUNT.load(Ordering::Relaxed) as u64 + 1;
    let stack_per_ap: u64 = AP_STACK_SIZE * 4;
    let ap_base = heap_top - (cpu_total * stack_per_ap);
    let stack_64_top = ap_base + (percpu::CPU_COUNT.load(Ordering::Relaxed) as u64) * stack_per_ap;

    {
        let phys_offset = memory::PHYS_MEM_OFFSET.load(Ordering::Acquire);
        let (l4_frame, _) = x86_64::registers::control::Cr3::read();
        let phys = l4_frame.start_address();
        let virt = VirtAddr::new(phys_offset) + phys.as_u64();
        let page_table_ptr: *mut PageTable = virt.as_mut_ptr();
        let page_table = unsafe { &mut *page_table_ptr };
        let mut mapper = unsafe { OffsetPageTable::new(page_table, VirtAddr::new(phys_offset)) };

        let page = Page::<Size4KiB>::containing_address(VirtAddr::new(0x40000));
        let frame = PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(tramp_phys));
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

        let mut guard = crate::memory::GLOBAL_ALLOCATOR.lock();
        let allocator = guard.as_mut().unwrap();
        unsafe {
            mapper
                .map_to(page, frame, flags, &mut *allocator)
                .unwrap()
                .flush();
        }
    }

    let percpu_addr = percpu::BSP_PCPU.get() as u64;

    trampoline::init_trampoline(tramp_phys, cr3_val, stack_64_top, percpu_addr, ap_entry);
    let tsize = unsafe { trampoline::trampoline_size() };
    k_nano::slog_nano!("SMP", "info", "Trampoline em 0x{:x} ({} bytes).",
        tramp_phys,
        tsize);

    let tramp_vector = (tramp_phys >> 12) as u8;
    k_nano::slog_nano!("SMP", "info", "INIT-SIPI-SIPI (vetor={:#04x})...", tramp_vector);

    apic::send_init_ipi();
    apic::wait_for_ipi_delivery();
    busy_wait_us(10000);

    apic::send_init_deassert_ipi();
    apic::wait_for_ipi_delivery();
    busy_wait_us(200);

    apic::send_sipi(tramp_vector);
    apic::wait_for_ipi_delivery();
    busy_wait_us(200);

    apic::send_sipi(tramp_vector);
    apic::wait_for_ipi_delivery();

    busy_wait_us(50000);

    let ap_woke = AP_ENTRY_COUNTER.load(Ordering::Relaxed);
    k_nano::slog_nano!("SMP", "info", "APs acordados: {}", ap_woke);
}

