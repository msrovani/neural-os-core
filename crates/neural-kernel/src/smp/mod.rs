pub mod percpu;
pub mod trampoline;

use crate::apic;
use crate::memory;
use crate::{println, serial_println};
use core::sync::atomic::{AtomicU64, Ordering};

static AP_ENTRY_COUNTER: AtomicU64 = AtomicU64::new(0);
const AP_STACK_SIZE: u64 = 16384;

extern "C" fn ap_entry(_cpu_id: u64) -> ! {
    let cpu_id = percpu::CPU_COUNT.load(Ordering::Relaxed);
    serial_println!("[SMP] AP {} entrou em modo 64-bit Rust!", cpu_id);
    println!("[SMP] AP {} entrou em modo 64-bit Rust!", cpu_id);

    unsafe { apic::apic_eoi(); }

    percpu::CPU_COUNT.fetch_add(1, Ordering::SeqCst);
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
    serial_println!("[SMP] Inicializando SMP...");
    println!("[SMP] Inicializando SMP...");

    if !apic::USING_APIC.load(Ordering::Relaxed) {
        serial_println!("[SMP] APIC nao disponivel — SMP ignorado.");
        println!("[SMP] APIC nao disponivel — SMP ignorado.");
        return;
    }

    let cr3_val = {
        let (frame, _) = x86_64::registers::control::Cr3::read();
        frame.start_address().as_u64()
    };

    let bsp_lapic_id = apic::lapic_id();
    percpu::init_bsp_percpu(bsp_lapic_id);
    serial_println!("[SMP] BSP PerCpu inicializado. LAPIC ID: {}", bsp_lapic_id);
    println!("[SMP] BSP PerCpu inicializado.");

    let tramp_phys = {
        let mut guard = memory::GLOBAL_ALLOCATOR.lock();
        let alloc = guard.as_mut().expect("Frame allocator not initialized");
        let frame = alloc
            .allocate_below_1mb()
            .expect("Failed to allocate trampoline page below 1 MB");
        frame.start_address().as_u64()
    };
    serial_println!("[SMP] Trampoline page em 0x{:x}", tramp_phys);
    println!("[SMP] Trampoline page em low memory.");

    let stack_top = {
        let mut guard = memory::GLOBAL_ALLOCATOR.lock();
        let alloc = guard.as_mut().unwrap();
        let num_frames = (AP_STACK_SIZE + 4095) / 4096;
        let first = alloc.allocate_frame().expect("Failed to alloc AP stack");
        for _ in 1..num_frames {
            alloc.allocate_frame().expect("Failed to alloc AP stack fragment");
        }
        first.start_address().as_u64() + AP_STACK_SIZE
    };

    // Identity-map trampoline page via offset page table
    {
        use x86_64::structures::paging::{Mapper, Page, PageTableFlags, Size4KiB};
        use x86_64::VirtAddr;
        let phys_offset = memory::PHYS_MEM_OFFSET.load(Ordering::Acquire);
        let (l4_frame, _) = x86_64::registers::control::Cr3::read();
        let l4_virt = VirtAddr::new(phys_offset) + l4_frame.start_address().as_u64();
        let pt = &mut *(l4_virt.as_mut_ptr());
        let mut mapper = x86_64::structures::paging::OffsetPageTable::new(pt, VirtAddr::new(phys_offset));

        let tramp_page = Page::containing_address(VirtAddr::new(tramp_phys));
        let tramp_frame = x86_64::structures::paging::PhysFrame::containing_address(
            x86_64::PhysAddr::new(tramp_phys),
        );

        if mapper.translate_page(tramp_page).is_err() {
            let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
            let mut guard = memory::GLOBAL_ALLOCATOR.lock();
            let alloc = guard.as_mut().unwrap();
            mapper
                .map_to(tramp_page, tramp_frame, flags, alloc)
                .expect("identity-map trampoline")
                .flush();
            serial_println!("[SMP] Trampoline page 0x{:x} identity-mapped.", tramp_phys);
        }
    }

    let percpu_addr = &percpu::BSP_PCPU as *const _ as u64;

    trampoline::init_trampoline(tramp_phys, cr3_val, stack_top, percpu_addr, ap_entry);
    serial_println!(
        "[SMP] Trampoline em 0x{:x} ({} bytes).",
        tramp_phys,
        trampoline::trampoline_size()
    );
    println!("[SMP] Trampoline configurado.");

    let tramp_vector = (tramp_phys >> 12) as u8;
    serial_println!(
        "[SMP] INIT-SIPI-SIPI (vetor={:#04x})...",
        tramp_vector
    );
    println!("[SMP] INIT-SIPI-SIPI...");

    apic::send_init_ipi();
    apic::wait_for_ipi_delivery();
    busy_wait_us(10000);

    apic::send_sipi(tramp_vector);
    apic::wait_for_ipi_delivery();
    busy_wait_us(200);

    apic::send_sipi(tramp_vector);
    apic::wait_for_ipi_delivery();

    serial_println!(
        "[SMP] APs acordados: {}",
        AP_ENTRY_COUNTER.load(Ordering::Relaxed)
    );
    println!("[SMP] INIT-SIPI-SIPI concluido.");
}
