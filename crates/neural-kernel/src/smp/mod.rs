pub mod percpu;
pub mod trampoline;

use crate::apic;
use crate::memory;
use crate::{println, serial_println};
use core::sync::atomic::{AtomicU64, Ordering};

use spin::Mutex;

static AP_BOOT_LOCK: Mutex<()> = Mutex::new(());
static AP_ENTRY_COUNTER: AtomicU64 = AtomicU64::new(0);
#[allow(dead_code)]
const AP_STACK_SIZE: u64 = 16384;

extern "C" fn ap_entry(_cpu_id: u64) -> ! {
    let _lock = AP_BOOT_LOCK.lock();
    let cpu_id = percpu::CPU_COUNT.fetch_add(1, Ordering::SeqCst);
    serial_println!("[SMP] AP {} entrou em modo 64-bit Rust!", cpu_id);
    println!("[SMP] AP {} entrou em modo 64-bit Rust!", cpu_id);
    drop(_lock);

    unsafe { apic::apic_eoi(); }

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

    // 32-bit stack: top of trampoline page (identity-mapped low 2MB)
    // 64-bit stack: top of heap (mapped via PML4 offset)
    let _stack_32_top = tramp_phys + 4096;
    let stack_64_top = crate::allocator::HEAP_START as u64 + crate::allocator::HEAP_SIZE as u64 - 0x1000;

    // Identity-map trampoline page (VA 0x40000 -> PA 0x40000)
    {
        let phys_offset = memory::PHYS_MEM_OFFSET.load(Ordering::Acquire);
        let (l4_frame, _) = x86_64::registers::control::Cr3::read();
        let l4_virt = (l4_frame.start_address().as_u64() + phys_offset) as *const u64;
        let pml4e = unsafe { *l4_virt.add(0) };
        let pdpt_base = (pml4e & 0x000ffffffffff000) + phys_offset;
        let pdpte = unsafe { *(pdpt_base as *const u64) };
        let pd_base = (pdpte & 0x000ffffffffff000) + phys_offset;
        let pd0 = unsafe { *(pd_base as *const u64) };
        let pt_base = (pd0 & 0x000ffffffffff000) + phys_offset;
        let pte_ptr = (pt_base + 0x200) as *mut u64;
        let pte_val: u64 = (tramp_phys as u64) | 0x003;
        unsafe { core::ptr::write_volatile(pte_ptr, pte_val); }
    }

    let percpu_addr = &percpu::BSP_PCPU as *const _ as u64;

    trampoline::init_trampoline(tramp_phys, cr3_val, stack_64_top, percpu_addr, ap_entry);
    let tsize = unsafe { trampoline::trampoline_size() };
    serial_println!(
        "[SMP] Trampoline em 0x{:x} ({} bytes).",
        tramp_phys,
        tsize
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

    // De-assert INIT (necessário para liberar o AP do reset)
    apic::send_init_deassert_ipi();
    apic::wait_for_ipi_delivery();
    busy_wait_us(200);

    apic::send_sipi(tramp_vector);
    apic::wait_for_ipi_delivery();
    busy_wait_us(200);

    apic::send_sipi(tramp_vector);
    apic::wait_for_ipi_delivery();

    // Wait ~50ms for APs to finish booting
    busy_wait_us(50000);

    let ap_count = AP_ENTRY_COUNTER.load(Ordering::Relaxed);
    serial_println!(
        "[SMP] APs acordados: {}",
        ap_count
    );
    println!("[SMP] INIT-SIPI-SIPI concluido.");
}
