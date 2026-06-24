use crate::acpi::AcpiInfo;
use crate::{println, serial_println};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use core::ptr::{read_volatile, write_volatile};

pub static USING_APIC: AtomicBool = AtomicBool::new(false);
static LAPIC_VIRT_BASE: AtomicU64 = AtomicU64::new(0);

const IA32_APIC_BASE_MSR: u32 = 0x1B;

const LAPIC_SVR: u64 = 0xF0;
const LAPIC_TPR: u64 = 0x80;
const LAPIC_EOI: u64 = 0xB0;
const LAPIC_ICR_LOW: u64 = 0x300;
const LAPIC_ICR_HIGH: u64 = 0x310;
const LAPIC_LVT_TIMER: u64 = 0x320;
const LAPIC_INIT_COUNT: u64 = 0x380;
const LAPIC_DIVIDE_CONFIG: u64 = 0x3E0;

const IOAPIC_IOREGSEL: u64 = 0x00;
const IOAPIC_IOWIN: u64 = 0x10;

const PIC_MASTER_DATA: u16 = 0x21;
const PIC_SLAVE_DATA: u16 = 0xA1;

struct Lapic {
    base: u64,
}

impl Lapic {
    unsafe fn new(base: u64) -> Self {
        Lapic { base }
    }

    unsafe fn read(&self, reg: u64) -> u32 {
        let addr = (self.base + reg) as *const u32;
        read_volatile(addr)
    }

    unsafe fn write(&self, reg: u64, value: u32) {
        let addr = (self.base + reg) as *mut u32;
        write_volatile(addr, value);
    }

    unsafe fn eoi(&self) {
        self.write(LAPIC_EOI, 0);
    }

    unsafe fn init(&self) {
        let svr = self.read(LAPIC_SVR);
        self.write(LAPIC_SVR, svr | 0x100);
        self.write(LAPIC_TPR, 0);

        self.write(LAPIC_LVT_TIMER, 0x10000);
        self.write(LAPIC_DIVIDE_CONFIG, 0b1011);
        self.write(LAPIC_INIT_COUNT, 0);

        serial_println!("[APIC] LAPIC inicializado. Base: 0x{:x}", self.base);
        println!("[APIC] LAPIC inicializado.");
    }
}

struct IoApic {
    base: u64,
}

impl IoApic {
    unsafe fn new(base: u64) -> Self {
        IoApic { base }
    }

    unsafe fn read(&self, reg: u8) -> u32 {
        let select_addr = (self.base + IOAPIC_IOREGSEL) as *mut u32;
        let window_addr = (self.base + IOAPIC_IOWIN) as *const u32;
        write_volatile(select_addr, reg as u32);
        read_volatile(window_addr)
    }

    unsafe fn write(&self, reg: u8, value: u32) {
        let select_addr = (self.base + IOAPIC_IOREGSEL) as *mut u32;
        let window_addr = (self.base + IOAPIC_IOWIN) as *mut u32;
        write_volatile(select_addr, reg as u32);
        write_volatile(window_addr, value);
    }

    unsafe fn redirect_irq(&self, irq: u8, vector: u8, delivery_mode: u8) {
        let redir_low = (vector as u32) | ((delivery_mode as u32) << 8) | (1u32 << 16);
        let redir_high = 0u32;
        let reg_index = 0x10 + (irq as u8) * 2;
        self.write(reg_index, redir_low);
        self.write(reg_index + 1, redir_high);
    }

    unsafe fn init(&self) {
        let max_redirect = (self.read(0x01) >> 16) & 0xFF;
        serial_println!("[APIC] IOAPIC em 0x{:x}. Max redirecionamentos: {}", self.base, max_redirect);
        println!("[APIC] IOAPIC encontrado. Max redirecionamentos: {}", max_redirect);

        for irq in 0..=max_redirect as u8 {
            let reg = 0x10 + irq * 2;
            let low = self.read(reg);
            let high = self.read(reg + 1);
            serial_println!("[APIC] IOAPIC redirection[{}]: low=0x{:08x}, high=0x{:08x}", irq, low, high);
        }

        self.redirect_irq(0, 32, 0);
        self.redirect_irq(1, 33, 0);
        serial_println!("[APIC] Timer (IRQ0) redirecionado para vetor 32.");
        serial_println!("[APIC] Teclado (IRQ1) redirecionado para vetor 33.");
        println!("[APIC] IOAPIC configurado: timer→vec32, keyboard→vec33.");
    }
}

unsafe fn disable_pic() {
    core::arch::asm!("out dx, al", in("dx") PIC_MASTER_DATA, in("al") 0xFFu8, options(nostack, preserves_flags));
    core::arch::asm!("out dx, al", in("dx") PIC_SLAVE_DATA, in("al") 0xFFu8, options(nostack, preserves_flags));
    serial_println!("[APIC] PIC 8259 desabilitado (mascara todos IRQs).");
    println!("[APIC] PIC 8259 desabilitado.");
}

unsafe fn read_lapic_base_msr() -> u64 {
    let msr_value = x86_64::registers::model_specific::Msr::new(IA32_APIC_BASE_MSR).read();
    let base = msr_value & 0xFFFF_FFFF_FFFF_F000;
    serial_println!("[APIC] LAPIC base via MSR: 0x{:x}", base);
    base
}

pub unsafe fn init_apic(info: &AcpiInfo) {
    serial_println!("[APIC] Inicializando APIC...");
    println!("[APIC] Inicializando APIC...");

    let _msr_base = read_lapic_base_msr();

    let lapic_virt_base = info.lapic_base + info.phys_mem_offset;
    LAPIC_VIRT_BASE.store(lapic_virt_base, Ordering::Release);
    let lapic = Lapic::new(lapic_virt_base);
    lapic.init();

    disable_pic();

    let ioapic_virt_base = info.ioapic_base + info.phys_mem_offset;
    let ioapic = IoApic::new(ioapic_virt_base);
    ioapic.init();

    USING_APIC.store(true, Ordering::Release);

    x86_64::instructions::interrupts::enable();

    serial_println!("[APIC] APIC operacional. Interrupcoes via LAPIC/IOAPIC.");
    println!("[APIC] APIC operacional. Interrupcoes via LAPIC/IOAPIC.");
}

pub unsafe fn apic_eoi() {
    let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
    if base != 0 {
        let eoi_addr = (base + LAPIC_EOI) as *mut u32;
        write_volatile(eoi_addr, 0);
    }
}

pub unsafe fn send_init_ipi() {
    let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
    if base == 0 { return; }

    // Wait for ICR to be idle
    while (read_volatile((base + LAPIC_ICR_LOW) as *const u32) & (1 << 12)) != 0 {
        core::hint::spin_loop();
    }

    // INIT IPI: delivery=INIT(5), trigger=level(1), level=assert(1), shorthand=all_excl_self(3)
    let icr_val = (5u32 << 8) | (1 << 14) | (1 << 15) | (3 << 18);
    write_volatile((base + LAPIC_ICR_HIGH) as *mut u32, 0); // dest field = 0 (shorthand)
    write_volatile((base + LAPIC_ICR_LOW) as *mut u32, icr_val);

    serial_println!("[SMP] INIT IPI enviado (ICR=0x{:08x})", icr_val);
    println!("[SMP] INIT IPI enviado.");
}

pub unsafe fn send_init_deassert_ipi() {
    let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
    if base == 0 { return; }

    while (read_volatile((base + LAPIC_ICR_LOW) as *const u32) & (1 << 12)) != 0 {
        core::hint::spin_loop();
    }

    // INIT de-assert: delivery=INIT(5), trigger=level(1), level=de-assert(0), shorthand=all_excl_self(3)
    let icr_val = (5u32 << 8) | (3 << 18);
    write_volatile((base + LAPIC_ICR_HIGH) as *mut u32, 0);
    write_volatile((base + LAPIC_ICR_LOW) as *mut u32, icr_val);

    serial_println!("[SMP] INIT de-assert enviado (ICR=0x{:08x})", icr_val);
}

pub unsafe fn send_sipi(trampoline_vector: u8) {
    let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
    if base == 0 { return; }

    // Wait for ICR to be idle
    while (read_volatile((base + LAPIC_ICR_LOW) as *const u32) & (1 << 12)) != 0 {
        core::hint::spin_loop();
    }

    // SIPI: delivery=StartUp(6), vector=trampoline_vector, shorthand=all_excl_self(3)
    let icr_val = (6u32 << 8) | (3 << 18) | trampoline_vector as u32;
    write_volatile((base + LAPIC_ICR_HIGH) as *mut u32, 0);
    write_volatile((base + LAPIC_ICR_LOW) as *mut u32, icr_val);

    serial_println!("[SMP] SIPI enviado (ICR=0x{:08x}, vetor={:#04x})", icr_val, trampoline_vector);
    println!("[SMP] SIPI enviado (vetor={:#04x}).", trampoline_vector);
}

pub unsafe fn wait_for_ipi_delivery() {
    let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
    if base == 0 { return; }

    while (read_volatile((base + LAPIC_ICR_LOW) as *const u32) & (1 << 12)) != 0 {
        core::hint::spin_loop();
    }
}

pub fn lapic_id() -> u8 {
    let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
    if base == 0 { return 0; }
    unsafe {
        let id_reg = read_volatile((base + 0x20) as *const u32);
        (id_reg >> 24) as u8
    }
}
