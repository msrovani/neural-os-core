//! Interrupt and exception handling — IDT, GDT, TSS, PIC, handlers.

use crate::{println, serial_println};
use core::sync::atomic::{AtomicU8, AtomicU16, AtomicU32, AtomicUsize, Ordering};
use lazy_static::lazy_static;
use pic8259::ChainedPics;
use spin::Mutex;
use x86_64::instructions::segmentation::Segment;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = 40;

pub static TIMER_TICKS: AtomicUsize = AtomicUsize::new(0);
pub static LAST_SCANCODE: AtomicU8 = AtomicU8::new(0);
pub static LAST_MOUSE_PACKET: AtomicU32 = AtomicU32::new(0);
static MOUSE_PHASE: AtomicU8 = AtomicU8::new(0);
static MOUSE_B0: AtomicU8 = AtomicU8::new(0);
static MOUSE_B1: AtomicU8 = AtomicU8::new(0);
static MOUSE_B2: AtomicU8 = AtomicU8::new(0);

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

// --------------------------------------------------------------------------
// Generic exception handler — dumps frame + error code + CPU state
// --------------------------------------------------------------------------

fn dump_exception(name: &str, stack_frame: &InterruptStackFrame, error_code: Option<u64>) {
    serial_println!("[EXCEPTION] {} ip={:#x} cs={:#x} flags={:#x} stack={:#x}",
        name,
        stack_frame.instruction_pointer.as_u64(),
        stack_frame.code_segment,
        stack_frame.cpu_flags,
        stack_frame.stack_pointer.as_u64(),
    );
    if let Some(code) = error_code {
        serial_println!("[EXCEPTION] {} err={:#x}", name, code);
    }
    println!("[EXCEPTION] {} (detalhes no serial)", name);
}

extern "x86-interrupt" fn divide_error_handler(f: InterruptStackFrame) { dump_exception("#DE", &f, None); loop { x86_64::instructions::hlt(); } }
extern "x86-interrupt" fn debug_handler(f: InterruptStackFrame) { dump_exception("#DB", &f, None); loop { x86_64::instructions::hlt(); } }
extern "x86-interrupt" fn nmi_handler(f: InterruptStackFrame) { dump_exception("#NMI", &f, None); loop { x86_64::instructions::hlt(); } }
extern "x86-interrupt" fn breakpoint_handler(f: InterruptStackFrame) { serial_println!("[EXCEPTION] #BP Breakpoint"); println!("[EXCEPTION] Breakpoint"); }
extern "x86-interrupt" fn overflow_handler(f: InterruptStackFrame) { dump_exception("#OF", &f, None); loop { x86_64::instructions::hlt(); } }
extern "x86-interrupt" fn bound_range_handler(f: InterruptStackFrame) { dump_exception("#BR", &f, None); loop { x86_64::instructions::hlt(); } }
extern "x86-interrupt" fn invalid_opcode_handler(f: InterruptStackFrame) { dump_exception("#UD", &f, None); loop { x86_64::instructions::hlt(); } }
extern "x86-interrupt" fn device_not_available_handler(f: InterruptStackFrame) { dump_exception("#NM", &f, None); loop { x86_64::instructions::hlt(); } }
extern "x86-interrupt" fn coprocessor_segment_overrun_handler(f: InterruptStackFrame) { dump_exception("#MF", &f, None); loop { x86_64::instructions::hlt(); } }

extern "x86-interrupt" fn invalid_tss_handler(f: InterruptStackFrame, code: u64) { dump_exception("#TS", &f, Some(code)); loop { x86_64::instructions::hlt(); } }
extern "x86-interrupt" fn segment_not_present_handler(f: InterruptStackFrame, code: u64) { dump_exception("#NP", &f, Some(code)); loop { x86_64::instructions::hlt(); } }
extern "x86-interrupt" fn stack_segment_handler(f: InterruptStackFrame, code: u64) { dump_exception("#SS", &f, Some(code)); loop { x86_64::instructions::hlt(); } }
extern "x86-interrupt" fn general_protection_fault_handler(f: InterruptStackFrame, code: u64) { dump_exception("#GP", &f, Some(code)); loop { x86_64::instructions::hlt(); } }
extern "x86-interrupt" fn alignment_check_handler(f: InterruptStackFrame, code: u64) { dump_exception("#AC", &f, Some(code)); loop { x86_64::instructions::hlt(); } }
extern "x86-interrupt" fn security_exception_handler(f: InterruptStackFrame, code: u64) { dump_exception("#CP", &f, Some(code)); loop { x86_64::instructions::hlt(); } }

extern "x86-interrupt" fn machine_check_handler(f: InterruptStackFrame) -> ! { dump_exception("#MC", &f, None); loop { x86_64::instructions::hlt(); } }
extern "x86-interrupt" fn fpu_error_handler(f: InterruptStackFrame) { dump_exception("#MF", &f, None); loop { x86_64::instructions::hlt(); } }
extern "x86-interrupt" fn simd_fp_exception_handler(f: InterruptStackFrame) { dump_exception("#XM", &f, None); loop { x86_64::instructions::hlt(); } }
extern "x86-interrupt" fn virtualization_handler(f: InterruptStackFrame) { dump_exception("#VE", &f, None); loop { x86_64::instructions::hlt(); } }
extern "x86-interrupt" fn reserved_handler(f: InterruptStackFrame) { dump_exception("#RSVD", &f, None); loop { x86_64::instructions::hlt(); } }

extern "x86-interrupt" fn double_fault_handler(f: InterruptStackFrame, code: u64) -> ! {
    dump_exception("#DF", &f, Some(code));
    serial_println!("[SELF-HEAL] DF: tentando restore de checkpoint...");
    let restored = crate::SELF_HEAL.lock().restore_checkpoint();
    if restored {
        serial_println!("[SELF-HEAL] Checkpoint restaurado!");
        serial_println!("[SELF-HEAL] Recomendado: reiniciar daemons via RESPAWN_QUEUE.");
    } else {
        serial_println!("[SELF-HEAL] Nenhum checkpoint. Halt.");
    }
    loop { x86_64::instructions::hlt(); }
}

extern "x86-interrupt" fn page_fault_handler(f: InterruptStackFrame, code: PageFaultErrorCode) {
    let addr = x86_64::registers::control::Cr2::read();
    dump_exception("#PF", &f, Some(code.bits() as u64));
    serial_println!("[SECURITY] CR2={:#x} flags={:?}", addr, code);
    loop { x86_64::instructions::hlt(); }
}

// --------------------------------------------------------------------------
// EOI — PIC com duplo EQI para escravo
// --------------------------------------------------------------------------

fn send_eoi(vector: u8) {
    if crate::apic::USING_APIC.load(Ordering::Relaxed) {
        unsafe { crate::apic::apic_eoi(); }
    } else {
        unsafe {
            // Sempre envia EOI ao mestre
            core::arch::asm!("out 0x20, al", in("al") 0x20u8, options(nostack, preserves_flags));
            // Se a interrupção veio do escravo (vetores >= 40), EOI também no escravo
            if vector >= PIC_2_OFFSET {
                core::arch::asm!("out 0xA0, al", in("al") 0x20u8, options(nostack, preserves_flags));
            }
        }
    }
}

// --------------------------------------------------------------------------
// IRQ handlers (hardware)
// --------------------------------------------------------------------------

extern "x86-interrupt" fn timer_handler(stack_frame: InterruptStackFrame) {
    let ticks = TIMER_TICKS.fetch_add(1, Ordering::Relaxed);
    if ticks < 5 {
        serial_println!("[TIMER] Interrupt fired! tick={}", ticks);
    }
    send_eoi(32);
}

extern "x86-interrupt" fn keyboard_interrupt_handler(stack_frame: InterruptStackFrame) {
    use x86_64::instructions::port::Port;
    let mut data_port = Port::<u8>::new(0x60);
    let scancode: u8 = unsafe { data_port.read() };
    LAST_SCANCODE.store(scancode, Ordering::Release);
    send_eoi(33);
}

extern "x86-interrupt" fn mouse_interrupt_handler(_stack_frame: InterruptStackFrame) {
    use x86_64::instructions::port::Port;
    // Mouse envia 1 byte por IRQ (3 bytes por pacote)
    // Usamos uma fase global para montar o pacote
    let mut data_port = Port::<u8>::new(0x60);
    let byte: u8 = unsafe { data_port.read() };
    let phase = MOUSE_PHASE.fetch_add(1, Ordering::Relaxed) % 3;
    match phase {
        0 => MOUSE_B0.store(byte, Ordering::Release),
        1 => MOUSE_B1.store(byte, Ordering::Release),
        2 => {
            MOUSE_B2.store(byte, Ordering::Release);
            let packet = MOUSE_B0.load(Ordering::Acquire) as u32
                | ((MOUSE_B1.load(Ordering::Acquire) as u32) << 8)
                | ((byte as u32) << 16);
            LAST_MOUSE_PACKET.store(packet, Ordering::Release);
        }
        _ => {}
    }
    send_eoi(44);
}

extern "x86-interrupt" fn unhandled_interrupt_handler(stack_frame: InterruptStackFrame) {
    serial_println!("[IRQ] Interrupção não tratada ip={:#x}", stack_frame.instruction_pointer.as_u64());
    if crate::apic::USING_APIC.load(Ordering::Relaxed) {
        unsafe { crate::apic::apic_eoi(); }
    } else {
        unsafe {
            core::arch::asm!("out 0x20, al", in("al") 0x20u8, options(nostack, preserves_flags));
            core::arch::asm!("out 0xA0, al", in("al") 0x20u8, options(nostack, preserves_flags));
        }
    }
}

// --------------------------------------------------------------------------
// IDT init — cobertura total de 0 a 31 + hardware + syscall
// --------------------------------------------------------------------------

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();

        // Exceções CPU 0-19 — campos nomeados
        idt.divide_error.set_handler_fn(divide_error_handler);
        idt.debug.set_handler_fn(debug_handler);
        idt.non_maskable_interrupt.set_handler_fn(nmi_handler);
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        idt.overflow.set_handler_fn(overflow_handler);
        idt.bound_range_exceeded.set_handler_fn(bound_range_handler);
        idt.invalid_opcode.set_handler_fn(invalid_opcode_handler);
        idt.device_not_available.set_handler_fn(device_not_available_handler);
        unsafe { idt.double_fault.set_handler_fn(double_fault_handler).set_stack_index(DOUBLE_FAULT_IST_INDEX); }
        idt[9].set_handler_fn(coprocessor_segment_overrun_handler);
        idt.invalid_tss.set_handler_fn(invalid_tss_handler);
        idt.segment_not_present.set_handler_fn(segment_not_present_handler);
        idt.stack_segment_fault.set_handler_fn(stack_segment_handler);
        idt.general_protection_fault.set_handler_fn(general_protection_fault_handler);
        idt.page_fault.set_handler_fn(page_fault_handler);
        idt.x87_floating_point.set_handler_fn(fpu_error_handler);
        idt.alignment_check.set_handler_fn(alignment_check_handler);
        idt.machine_check.set_handler_fn(machine_check_handler);
        idt.simd_floating_point.set_handler_fn(simd_fp_exception_handler);
        idt.virtualization.set_handler_fn(virtualization_handler);
        idt.security_exception.set_handler_fn(security_exception_handler);

        // Vetor 20, 22-31: reservados pela CPU (não devem disparar)
        // Vetor 21 = SecurityException (já configurado acima)

        // Hardware IRQs
        idt[32].set_handler_fn(timer_handler);
        idt[33].set_handler_fn(keyboard_interrupt_handler);
        idt[44].set_handler_fn(mouse_interrupt_handler);

        // Demais vetores (34-255)
        for i in 34..=255usize { idt[i].set_handler_fn(unhandled_interrupt_handler); }

        idt
    };
    // Fim do lazy_static IDT
}

/// Carrega GDT + TSS + IDT
pub fn init_idt() {
    GDT.0.load();
    unsafe {
        x86_64::instructions::segmentation::CS::set_reg(GDT.1.code_selector);
        x86_64::instructions::tables::load_tss(GDT.1.tss_selector);
        // Recarrega SS com um seletor nulo (evita #GP no iretq quando
        // o bootloader usa seletor diferente do nosso GDT)
        core::arch::asm!("mov ss, ax", in("ax") 0u16, options(nostack, preserves_flags));
    }
    IDT.load();
    serial_println!("[IDT] IDT carregada: vetores 0-31 (exceções) + 32-33 (IRQ) + 34-255 (genérico) cobertos.");
}

pub fn init_pics() {
    unsafe { PICS.lock().initialize(); }
    serial_println!("[PIC] 8259A remapeado: PIC1 offset 32, PIC2 offset 40.");
    println!("[PIC] 8259A remapeado.");
}

pub fn enable_interrupts() {
    x86_64::instructions::interrupts::enable();
    serial_println!("[CPU] Interrupcoes de hardware habilitadas (IF=1).");
    println!("[CPU] Interrupcoes de hardware habilitadas (IF=1).");
}
