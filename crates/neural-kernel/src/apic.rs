use crate::acpi::AcpiInfo;
use crate::{println, serial_println};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use core::ptr::{read_volatile, write_volatile};
use x86_64::structures::paging::{PageTable, PageTableFlags};
use x86_64::VirtAddr;

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

        self.write(LAPIC_DIVIDE_CONFIG, 0b1011);
        self.write(LAPIC_INIT_COUNT, 0);

        serial_println!("[APIC] LAPIC inicializado. Base: 0x{:x}", self.base);
        println!("[APIC] LAPIC inicializado.");
    }

    unsafe fn start_timer(&self) {
        self.write(LAPIC_LVT_TIMER, 32 | 0x20000);
        self.write(LAPIC_DIVIDE_CONFIG, 0b1011);
        self.write(LAPIC_INIT_COUNT, 0x800000);

        serial_println!("[APIC] LAPIC timer iniciado: vetor 32, count=8388608, div=1.");
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

    unsafe fn redirect_gsi(&self, gsi: u8, vector: u8, delivery_mode: u8) {
        let redir_low = (vector as u32) | ((delivery_mode as u32) << 8);
        let redir_high = 0u32;
        let reg_index = 0x10 + gsi * 2;
        self.write(reg_index, redir_low);
        self.write(reg_index + 1, redir_high);
    }

    unsafe fn init(&self, iso_overrides: &[(u8, u32)]) {
        let max_redirect = (self.read(0x01) >> 16) & 0xFF;
        serial_println!("[APIC] IOAPIC em 0x{:x}. Max redirecionamentos: {}", self.base, max_redirect);
        println!("[APIC] IOAPIC encontrado. Max redirecionamentos: {}", max_redirect);

        let any_unmasked = (0..=max_redirect as u8).any(|gsi| {
            let low = self.read(0x10 + gsi * 2);
            (low & 0x10000) == 0
        });
        serial_println!("[APIC] IOAPIC dump: {} redirects, all_masked={}", max_redirect + 1, !any_unmasked);

        let kbd_gsi = iso_overrides.iter()
            .find(|(source, _)| *source == 1)
            .map(|(_, gsi)| *gsi as u8)
            .unwrap_or(1);

        self.redirect_gsi(kbd_gsi, 33, 0);

        let v1_reg = 0x10 + kbd_gsi * 2;
        let v1_low = self.read(v1_reg);
        let v1_high = self.read(v1_reg + 1);
        serial_println!("[APIC] IOAPIC verificado: kbd GSI {} (0x{:02x}:0x{:08x})",
            kbd_gsi, v1_high, v1_low);
        serial_println!("[APIC] Teclado (IRQ1) redirecionado para vetor 33.");
        println!("[APIC] IOAPIC configurado: keyboard→vec33.");
    }
}

    unsafe fn disable_pic() {
        core::arch::asm!("out dx, al", in("dx") PIC_MASTER_DATA, in("al") 0xFFu8, options(nostack, preserves_flags));
        core::arch::asm!("out dx, al", in("dx") PIC_SLAVE_DATA, in("al") 0xFFu8, options(nostack, preserves_flags));
        serial_println!("[APIC] PIC 8259 desabilitado (mascara todos IRQs).");
        println!("[APIC] PIC 8259 desabilitado.");
    }

    pub unsafe fn pit_init() {
        core::arch::asm!("out 0x43, al", in("al") 0x36u8, options(nostack, preserves_flags));
        core::arch::asm!("out 0x40, al", in("al") 0x00u8, options(nostack, preserves_flags));
        core::arch::asm!("out 0x40, al", in("al") 0x00u8, options(nostack, preserves_flags));
        serial_println!("[PIT] Canal 0 programado: modo 3, divisor 65536 (18.2 Hz).");
    }

unsafe fn read_lapic_base_msr() -> u64 {
    let msr_value = x86_64::registers::model_specific::Msr::new(IA32_APIC_BASE_MSR).read();
    let base = msr_value & 0xFFFF_FFFF_FFFF_F000;
    serial_println!("[APIC] LAPIC base via MSR: 0x{:x}", base);
    base
}

/// Mapeia uma página MMIO como uncacheable e presente.
/// Diferente de set_page_uc (que só modifica flags se a entrada existir),
/// esta função CRIA a entrada se ela não existir, apontando para o frame físico.
pub(crate) unsafe fn map_mmio_page(phys_addr: u64, phys_mem_offset: u64) {
    use x86_64::structures::paging::{FrameAllocator, Page, PhysFrame, Size4KiB, PageTable, PageTableFlags};
    use x86_64::VirtAddr;
    use x86_64::registers::control::Cr3;

    let virt = VirtAddr::new(phys_addr + phys_mem_offset);
    let page = Page::<Size4KiB>::containing_address(virt);
    let frame = PhysFrame::<Size4KiB>::containing_address(x86_64::PhysAddr::new(phys_addr));

    let (l4_frame, _) = Cr3::read();
    let base = VirtAddr::new(phys_mem_offset);
    let l4_virt = base + l4_frame.start_address().as_u64();
    let l4_table = &mut *(l4_virt.as_mut_ptr::<PageTable>());
    let l3_idx = page.p4_index();

    // L3
    let l3_entry = &mut l4_table[l3_idx];
    if !l3_entry.flags().contains(PageTableFlags::PRESENT) {
        // Allocate a page for L3 table
        let new_frame = { let mut g = crate::memory::GLOBAL_ALLOCATOR.lock(); (*g).as_mut().unwrap().allocate_frame().unwrap() };
        let new_virt = base + new_frame.start_address().as_u64();
        core::ptr::write_bytes(new_virt.as_mut_ptr::<u8>(), 0, 4096);
        l3_entry.set_addr(new_frame.start_address(), PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_CACHE | PageTableFlags::WRITE_THROUGH);
    }
    let l3_table = &mut *( (base + l3_entry.addr().as_u64()).as_mut_ptr::<PageTable>() );

    // L2
    let l2_idx = page.p3_index();
    let l2_entry = &mut l3_table[l2_idx];
    if !l2_entry.flags().contains(PageTableFlags::PRESENT) {
        let new_frame = { let mut g = crate::memory::GLOBAL_ALLOCATOR.lock(); (*g).as_mut().unwrap().allocate_frame().unwrap() };
        let new_virt = base + new_frame.start_address().as_u64();
        core::ptr::write_bytes(new_virt.as_mut_ptr::<u8>(), 0, 4096);
        l2_entry.set_addr(new_frame.start_address(), PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_CACHE | PageTableFlags::WRITE_THROUGH);
    }
    let l2_table = &mut *( (base + l2_entry.addr().as_u64()).as_mut_ptr::<PageTable>() );

    // L1 (final level) — map the MMIO page
    let l1_idx = page.p2_index();
    let l1_entry = &mut l2_table[l1_idx];
    l1_entry.set_addr(frame.start_address(), PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_CACHE | PageTableFlags::WRITE_THROUGH);

    x86_64::instructions::tlb::flush(virt);
}

pub(crate) unsafe fn set_page_uc(phys_addr: u64, phys_mem_offset: u64) {
    let virt = VirtAddr::new(phys_addr + phys_mem_offset);

    let (l4_frame, _) = x86_64::registers::control::Cr3::read();
    let base = VirtAddr::new(phys_mem_offset);

    let l4_virt = base + l4_frame.start_address().as_u64();
    let l4_table = &mut *(l4_virt.as_mut_ptr::<PageTable>());
    let l3_entry = &mut l4_table[usize::from(virt.p4_index())];
    if !l3_entry.flags().contains(PageTableFlags::PRESENT) { return; }

    if l3_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
        let mut flags = l3_entry.flags();
        flags |= PageTableFlags::NO_CACHE | PageTableFlags::WRITE_THROUGH;
        l3_entry.set_flags(flags);
        x86_64::instructions::tlb::flush(virt);
        return;
    }

    let l3_virt = base + l3_entry.addr().as_u64();
    let l3_table = &mut *(l3_virt.as_mut_ptr::<PageTable>());
    let l2_entry = &mut l3_table[usize::from(virt.p3_index())];
    if !l2_entry.flags().contains(PageTableFlags::PRESENT) { return; }

    if l2_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
        let mut flags = l2_entry.flags();
        flags |= PageTableFlags::NO_CACHE | PageTableFlags::WRITE_THROUGH;
        l2_entry.set_flags(flags);
        x86_64::instructions::tlb::flush(virt);
        return;
    }

    let l2_virt = base + l2_entry.addr().as_u64();
    let l2_table = &mut *(l2_virt.as_mut_ptr::<PageTable>());
    let l1_entry = &mut l2_table[usize::from(virt.p2_index())];
    if !l1_entry.flags().contains(PageTableFlags::PRESENT) { return; }

    if l1_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
        let mut flags = l1_entry.flags();
        flags |= PageTableFlags::NO_CACHE | PageTableFlags::WRITE_THROUGH;
        l1_entry.set_flags(flags);
        x86_64::instructions::tlb::flush(virt);
        return;
    }

    let l1_virt = base + l1_entry.addr().as_u64();
    let l1_table = &mut *(l1_virt.as_mut_ptr::<PageTable>());
    let pte = &mut l1_table[usize::from(virt.p1_index())];

    let mut flags = pte.flags();
    flags |= PageTableFlags::NO_CACHE | PageTableFlags::WRITE_THROUGH;
    pte.set_flags(flags);

    x86_64::instructions::tlb::flush(virt);
}

pub unsafe fn init_apic(info: &AcpiInfo) {
    serial_println!("[APIC] Inicializando APIC...");
    println!("[APIC] Inicializando APIC...");

    set_page_uc(0xFEC0_0000, info.phys_mem_offset);
    set_page_uc(0xFEE0_0000, info.phys_mem_offset);
    serial_println!("[APIC] IOAPIC/LAPIC pages mapped uncacheable.");

    let _msr_base = read_lapic_base_msr();

    let lapic_virt_base = info.lapic_base + info.phys_mem_offset;
    LAPIC_VIRT_BASE.store(lapic_virt_base, Ordering::Release);
    let lapic = Lapic::new(lapic_virt_base);
    lapic.init();

    disable_pic();
    pit_init();

    let ioapic_virt_base = info.ioapic_base + info.phys_mem_offset;
    let ioapic = IoApic::new(ioapic_virt_base);
    ioapic.init(&info.iso_overrides);

    lapic.start_timer();

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
