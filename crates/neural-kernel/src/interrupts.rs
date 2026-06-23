use crate::{println, serial_println};
use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use lazy_static::lazy_static;
use pic8259::ChainedPics;
use spin::Mutex;
use x86_64::instructions::segmentation::Segment;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = 40;

pub static TIMER_TICKS: AtomicUsize = AtomicUsize::new(0);
pub static LAST_SCANCODE: AtomicU8 = AtomicU8::new(0);

lazy_static! {
    static ref PICS: Mutex<ChainedPics> =
        Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });
}

const DOUBLE_FAULT_IST_INDEX: u16 = 0;

lazy_static! {
    static ref TSS: TaskStateSegment = {
        let mut tss = TaskStateSegment::new();
        tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
            const STACK_SIZE: usize = 4096 * 5;
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];
            let stack_start = VirtAddr::from_ptr(core::ptr::addr_of!(STACK));
            stack_start + STACK_SIZE
        };
        tss
    };
}

lazy_static! {
    static ref GDT: (GlobalDescriptorTable, Selectors) = {
        let mut gdt = GlobalDescriptorTable::new();
        let code_selector = gdt.add_entry(Descriptor::kernel_code_segment());
        let tss_selector = gdt.add_entry(Descriptor::tss_segment(&TSS));
        (gdt, Selectors { code_selector, tss_selector })
    };
}

struct Selectors {
    code_selector: SegmentSelector,
    tss_selector: SegmentSelector,
}

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        unsafe {
            idt.double_fault
                .set_handler_fn(double_fault_handler)
                .set_stack_index(DOUBLE_FAULT_IST_INDEX);
        }
        idt.page_fault.set_handler_fn(page_fault_handler);
        idt.general_protection_fault.set_handler_fn(gpf_handler);
        idt.stack_segment_fault.set_handler_fn(stack_segment_handler);
        idt.segment_not_present.set_handler_fn(segment_not_present_handler);
        idt.invalid_tss.set_handler_fn(invalid_tss_handler);
        idt.alignment_check.set_handler_fn(alignment_check_handler);
        idt[32].set_handler_fn(timer_handler);
        idt[33].set_handler_fn(keyboard_interrupt_handler);
        for i in 34..=255usize {
            idt[i].set_handler_fn(unhandled_interrupt_handler);
        }
        idt
    };
}

extern "x86-interrupt" fn breakpoint_handler(_stack_frame: InterruptStackFrame) {
    println!("[EXCEPTION] Breakpoint Detectado");
    serial_println!("[EXCEPTION] Breakpoint Detectado");
}

extern "x86-interrupt" fn double_fault_handler(
    _stack_frame: InterruptStackFrame,
    error_code: u64,
) -> ! {
    serial_println!("[EXCEPTION] DOUBLE FAULT (err={}) HALT", error_code);
    println!("[EXCEPTION] DOUBLE FAULT (err={}) HALT", error_code);
    loop { x86_64::instructions::hlt(); }
}

extern "x86-interrupt" fn page_fault_handler(
    _stack_frame: InterruptStackFrame,
    error_code: x86_64::structures::idt::PageFaultErrorCode,
) {
    let addr = x86_64::registers::control::Cr2::read();
    serial_println!("[SECURITY] Page Fault detectado em {:?}. Acesso negado. Error code: {:?}", addr, error_code);
    println!("[SECURITY] Page Fault detectado em {:?}. Acesso negado.", addr);
    loop {
        x86_64::instructions::hlt();
    }
}

extern "x86-interrupt" fn timer_handler(_stack_frame: InterruptStackFrame) {
    TIMER_TICKS.fetch_add(1, Ordering::Relaxed);
    unsafe {
        core::arch::asm!("out 0x20, al", in("al") 0x20u8, options(nostack, preserves_flags));
    }
}

extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    use x86_64::instructions::port::Port;
    let mut data_port = Port::<u8>::new(0x60);
    let scancode: u8 = unsafe { data_port.read() };
    LAST_SCANCODE.store(scancode, Ordering::Release);
    unsafe {
        core::arch::asm!("out 0x20, al", in("al") 0x20u8, options(nostack, preserves_flags));
    }
}

extern "x86-interrupt" fn gpf_handler(_stack_frame: InterruptStackFrame, error_code: u64) {
    serial_println!("[EXCEPTION] General Protection Fault (err={}) HALT", error_code);
    println!("[EXCEPTION] General Protection Fault (err={}) HALT", error_code);
    loop { x86_64::instructions::hlt(); }
}

extern "x86-interrupt" fn stack_segment_handler(_stack_frame: InterruptStackFrame, error_code: u64) {
    serial_println!("[EXCEPTION] Stack Segment Fault (err={}) HALT", error_code);
    println!("[EXCEPTION] Stack Segment Fault (err={}) HALT", error_code);
    loop { x86_64::instructions::hlt(); }
}

extern "x86-interrupt" fn segment_not_present_handler(_stack_frame: InterruptStackFrame, error_code: u64) {
    serial_println!("[EXCEPTION] Segment Not Present (err={}) HALT", error_code);
    println!("[EXCEPTION] Segment Not Present (err={}) HALT", error_code);
    loop { x86_64::instructions::hlt(); }
}

extern "x86-interrupt" fn invalid_tss_handler(_stack_frame: InterruptStackFrame, error_code: u64) {
    serial_println!("[EXCEPTION] Invalid TSS (err={}) HALT", error_code);
    println!("[EXCEPTION] Invalid TSS (err={}) HALT", error_code);
    loop { x86_64::instructions::hlt(); }
}

extern "x86-interrupt" fn alignment_check_handler(_stack_frame: InterruptStackFrame, error_code: u64) {
    serial_println!("[EXCEPTION] Alignment Check (err={}) HALT", error_code);
    println!("[EXCEPTION] Alignment Check (err={}) HALT", error_code);
    loop { x86_64::instructions::hlt(); }
}

extern "x86-interrupt" fn unhandled_interrupt_handler(_stack_frame: InterruptStackFrame) {
    unsafe {
        core::arch::asm!("out 0x20, al", in("al") 0x20u8, options(nostack, preserves_flags));
        core::arch::asm!("out 0xA0, al", in("al") 0x20u8, options(nostack, preserves_flags));
    }
}

pub fn init_idt() {
    GDT.0.load();
    unsafe {
        x86_64::instructions::segmentation::CS::set_reg(GDT.1.code_selector);
        x86_64::instructions::tables::load_tss(GDT.1.tss_selector);
    }
    IDT.load();
}

pub fn init_pics() {
    unsafe {
        PICS.lock().initialize();
    }
    serial_println!("[PIC] 8259A remapeado: PIC1 offset 32, PIC2 offset 40.");
    println!("[PIC] 8259A remapeado: PIC1 offset 32, PIC2 offset 40.");
}

pub fn enable_interrupts() {
    x86_64::instructions::interrupts::enable();
    serial_println!("[CPU] Interrupcoes de hardware habilitadas (IF=1).");
    println!("[CPU] Interrupcoes de hardware habilitadas (IF=1).");
}
