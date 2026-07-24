//! Interrupt and exception handling — IDT, GDT, TSS, PIC, handlers.

use crate::{println};
use core::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use lazy_static::lazy_static;
// ponytail: PICS/pic8259 removed — kernel só usa APIC
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
pub static PAGE_FAULT_COUNT: AtomicU32 = AtomicU32::new(0);
/// Posição absoluta atualizada no IRQ/poll (não depende do MouseAgent / Hermes).
pub static MOUSE_ABS_X: AtomicU32 = AtomicU32::new(640);
pub static MOUSE_ABS_Y: AtomicU32 = AtomicU32::new(360);
pub static MOUSE_ABS_BTN: AtomicU8 = AtomicU8::new(0);
/// Edge detect no DisplayAgent / flash visual de clique.
pub static MOUSE_PREV_BTN: AtomicU8 = AtomicU8::new(0);
pub static MOUSE_CLICK_FLASH: AtomicU32 = AtomicU32::new(0);
pub static MOUSE_MAX_X: AtomicU32 = AtomicU32::new(1279);
pub static MOUSE_MAX_Y: AtomicU32 = AtomicU32::new(719);
pub static MOUSE_BYTE_LOG: AtomicU32 = AtomicU32::new(0);
/// FB físico para cursor IRQ-safe (Hermes THINK não redesenha).
pub static FB_ADDR: AtomicU64 = AtomicU64::new(0);
pub static FB_STRIDE: AtomicU32 = AtomicU32::new(0);
pub static FB_BPP: AtomicU32 = AtomicU32::new(4);
pub static FB_W: AtomicU32 = AtomicU32::new(0);
pub static FB_H: AtomicU32 = AtomicU32::new(0);
static MOUSE_PHASE: AtomicU8 = AtomicU8::new(0);
static MOUSE_B0: AtomicU8 = AtomicU8::new(0);
static MOUSE_B1: AtomicU8 = AtomicU8::new(0);
static MOUSE_B2: AtomicU8 = AtomicU8::new(0);

// IPI handlers para SMP
pub static IPI_RESCHEDULE: AtomicUsize = AtomicUsize::new(0);
pub static IPI_HALT: AtomicUsize = AtomicUsize::new(0);
pub static IPI_CALL_FUNCTION: AtomicUsize = AtomicUsize::new(0);


const DOUBLE_FAULT_IST_INDEX: u16 = 0;
const PAGE_FAULT_IST_INDEX: u16 = 1;
const GENERAL_PROTECTION_IST_INDEX: u16 = 2;
const TIMER_IST_INDEX: u16 = 3;

lazy_static! {
    static ref TSS: TaskStateSegment = {
        let mut tss = TaskStateSegment::new();
        tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
            const STACK_SIZE: usize = 4096 * 5;
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];
            let stack_start = VirtAddr::from_ptr(core::ptr::addr_of!(STACK));
            stack_start + STACK_SIZE
        };
        tss.interrupt_stack_table[PAGE_FAULT_IST_INDEX as usize] = {
            const STACK_SIZE: usize = 4096 * 4;
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];
            let stack_start = VirtAddr::from_ptr(core::ptr::addr_of!(STACK));
            stack_start + STACK_SIZE
        };
        tss.interrupt_stack_table[GENERAL_PROTECTION_IST_INDEX as usize] = {
            const STACK_SIZE: usize = 4096 * 4;
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];
            let stack_start = VirtAddr::from_ptr(core::ptr::addr_of!(STACK));
            stack_start + STACK_SIZE
        };
        tss.interrupt_stack_table[TIMER_IST_INDEX as usize] = {
            const STACK_SIZE: usize = 4096 * 4;
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

// ponytail: lock-free serial write for exception context (avoids #DF cascade)
fn putc(c: u8) {
    unsafe { core::arch::asm!("out dx, al", in("dx") 0x3F8u16, in("al") c, options(nostack, preserves_flags)); }
}
fn puts(s: &[u8]) { for &c in s { putc(c); } }
fn puthex(mut n: u64) {
    putc(b'0'); putc(b'x');
    for _ in 0..16 {
        let d = (n >> 60) as u8;
        putc(if d < 10 { b'0' + d } else { b'a' + d - 10 });
        n <<= 4;
    }
}

fn dump_exception(name: &str, stack_frame: &InterruptStackFrame, error_code: Option<u64>) {
    puts(b"[EXC] ");
    puts(name.as_bytes());
    puts(b" ip="); puthex(stack_frame.instruction_pointer.as_u64());
    puts(b" fl="); puthex(stack_frame.cpu_flags as u64);
    puts(b" sp="); puthex(stack_frame.stack_pointer.as_u64());
    if let Some(code) = error_code { puts(b" err="); puthex(code); }
    putc(b'\n');
}

extern "x86-interrupt" fn divide_error_handler(f: InterruptStackFrame) { dump_exception("#DE", &f, None); loop { x86_64::instructions::hlt(); } }
extern "x86-interrupt" fn debug_handler(f: InterruptStackFrame) { dump_exception("#DB", &f, None); loop { x86_64::instructions::hlt(); } }
extern "x86-interrupt" fn nmi_handler(f: InterruptStackFrame) { dump_exception("#NMI", &f, None); loop { x86_64::instructions::hlt(); } }
extern "x86-interrupt" fn breakpoint_handler(_f: InterruptStackFrame) { crate::slog_nano!("EXCEPTION", "info", "#BP Breakpoint"); println!("[EXCEPTION] Breakpoint"); }
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
    puts(b"[SELF-HEAL] Halt (lock-free).\n");
    loop { x86_64::instructions::hlt(); }
}

extern "x86-interrupt" fn page_fault_handler(f: InterruptStackFrame, code: PageFaultErrorCode) {
    let cr2 = x86_64::registers::control::Cr2::read();
    dump_exception("#PF", &f, Some(code.bits() as u64));
    puts(b" CR2="); puthex(cr2.as_u64()); putc(b'\n');
    let count = PAGE_FAULT_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    if count <= 10 {
        return;
    }
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

extern "x86-interrupt" fn timer_handler(_stack_frame: InterruptStackFrame) {
    let ticks = TIMER_TICKS.fetch_add(1, Ordering::Relaxed);
    if ticks < 5 {
        crate::slog_nano!("TIMER", "info", "Interrupt fired! tick={}", ticks);
    }
    send_eoi(32);
}

extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    use x86_64::instructions::port::Port;
    let status: u8 = unsafe { Port::<u8>::new(0x64).read() };
    // Bit5=1 → dado do mouse no 0x60. Não roubar (desync do pacote PS/2).
    if status & 0x20 != 0 {
        send_eoi(33);
        return;
    }
    if status & 0x01 == 0 {
        send_eoi(33);
        return;
    }
    let mut data_port = Port::<u8>::new(0x60);
    let scancode: u8 = unsafe { data_port.read() };
    LAST_SCANCODE.store(scancode, Ordering::Release);
    send_eoi(33);
}

/// ADR-0062 P24b: report HID boot mouse → mesmo caminho que PS/2 (ABS + LAST_MOUSE_PACKET).
/// `buttons`: bits 0..2; `dx`/`dy`: relativos (HID: +Y = up → tela inverte como PS/2).
pub fn mouse_inject_hid_boot(buttons: u8, dx: i8, dy: i8) {
    let b0 = (buttons & 0x07) | 0x08;
    let b1 = dx as u8;
    let b2 = dy as u8;
    let packet = b0 as u32 | ((b1 as u32) << 8) | ((b2 as u32) << 16);
    LAST_MOUSE_PACKET.store(packet, Ordering::Release);

    let dx_i = dx as i32;
    let dy_i = -(dy as i32);
    let max_x = MOUSE_MAX_X.load(Ordering::Relaxed) as i32;
    let max_y = MOUSE_MAX_Y.load(Ordering::Relaxed) as i32;
    let nx = (MOUSE_ABS_X.load(Ordering::Relaxed) as i32 + dx_i).clamp(0, max_x);
    let ny = (MOUSE_ABS_Y.load(Ordering::Relaxed) as i32 + dy_i).clamp(0, max_y);
    MOUSE_ABS_X.store(nx as u32, Ordering::Release);
    MOUSE_ABS_Y.store(ny as u32, Ordering::Release);
    let btn = buttons & 0x07;
    let prev = MOUSE_ABS_BTN.swap(btn, Ordering::AcqRel);
    mouse_paint_irq_cursor(nx as u32, ny as u32);
    let pressed = btn & !prev;
    if pressed != 0 {
        MOUSE_CLICK_FLASH.store(12, Ordering::Release);
    }
}

/// Alimenta a máquina de estados do pacote PS/2 (3 bytes). Usado por IRQ e poll.
pub fn mouse_feed_byte(byte: u8) {
    let n = MOUSE_BYTE_LOG.fetch_add(1, Ordering::Relaxed);
    if n < 64 {
        crate::slog_nano!(
            "MOUSE",
            "info",
            "byte[{}]={:#04x} status_phase={} aux",
            n,
            byte,
            MOUSE_PHASE.load(Ordering::Relaxed)
        );
    }

    let phase = MOUSE_PHASE.load(Ordering::Relaxed);
    if phase == 0 && (byte & 0x08) == 0 {
        if n < 64 {
            crate::slog_nano!("MOUSE", "info", "byte[{}] discard (no sync bit3)", n);
        }
        return;
    }
    let phase = MOUSE_PHASE.fetch_add(1, Ordering::Relaxed) % 3;
    match phase {
        0 => MOUSE_B0.store(byte, Ordering::Release),
        1 => MOUSE_B1.store(byte, Ordering::Release),
        2 => {
            let b0 = MOUSE_B0.load(Ordering::Acquire);
            let b1 = MOUSE_B1.load(Ordering::Acquire);
            let b2 = byte;
            let packet = b0 as u32 | ((b1 as u32) << 8) | ((b2 as u32) << 16);
            LAST_MOUSE_PACKET.store(packet, Ordering::Release);
            MOUSE_PHASE.store(0, Ordering::Release);

            // Aplica delta já no IRQ — sobrevive ao Hermes THINK bloqueante.
            let dx = b1 as i8 as i32;
            let dy = -(b2 as i8 as i32);
            let max_x = MOUSE_MAX_X.load(Ordering::Relaxed) as i32;
            let max_y = MOUSE_MAX_Y.load(Ordering::Relaxed) as i32;
            let nx = (MOUSE_ABS_X.load(Ordering::Relaxed) as i32 + dx).clamp(0, max_x);
            let ny = (MOUSE_ABS_Y.load(Ordering::Relaxed) as i32 + dy).clamp(0, max_y);
            MOUSE_ABS_X.store(nx as u32, Ordering::Release);
            MOUSE_ABS_Y.store(ny as u32, Ordering::Release);
            let btn = b0 & 0x07;
            let prev = MOUSE_ABS_BTN.swap(btn, Ordering::AcqRel);
            mouse_paint_irq_cursor(nx as u32, ny as u32);

            let pressed = btn & !prev;
            if pressed != 0 {
                MOUSE_CLICK_FLASH.store(12, Ordering::Release);
                crate::slog_nano!(
                    "MOUSE",
                    "info",
                    "CLICK press={:#x} release_edge={:#x} @{}x{} (irq)",
                    pressed,
                    prev & !btn,
                    nx,
                    ny
                );
            } else if (prev & !btn) != 0 {
                crate::slog_nano!(
                    "MOUSE",
                    "info",
                    "CLICK release={:#x} @{}x{} (irq)",
                    prev & !btn,
                    nx,
                    ny
                );
            }

            let pkt_n = n / 3;
            if pkt_n < 32 || pressed != 0 {
                crate::slog_nano!(
                    "MOUSE",
                    "info",
                    "pkt #{} raw={:02x}{:02x}{:02x} d={},{} pos={}x{} btn={:#x}",
                    pkt_n,
                    b0,
                    b1,
                    b2,
                    dx,
                    dy,
                    nx,
                    ny,
                    btn
                );
            }
        }
        _ => {}
    }
}

/// Pinta seta mínima no FB físico (visível mesmo com Hermes bloqueado).
fn mouse_paint_irq_cursor(x: u32, y: u32) {
    let addr = FB_ADDR.load(Ordering::Relaxed);
    if addr == 0 {
        return;
    }
    let stride = FB_STRIDE.load(Ordering::Relaxed) as usize;
    let bpp = FB_BPP.load(Ordering::Relaxed) as usize;
    if stride == 0 || bpp == 0 {
        return;
    }
    let w = FB_W.load(Ordering::Relaxed) as usize;
    let h = FB_H.load(Ordering::Relaxed) as usize;
    let ptr = addr as *mut u8;
    // Bloco 8×12 branco com borda preta — barato no IRQ
    for row in 0..12u32 {
        for col in 0..8u32 {
            let px = x.saturating_add(col) as usize;
            let py = y.saturating_add(row) as usize;
            if px >= w || py >= h {
                continue;
            }
            let edge = row == 0 || col == 0 || row == 11 || col == 7 || col == row;
            let (r, g, b) = if edge { (0u8, 0u8, 0u8) } else { (255, 255, 255) };
            let off = py * stride + px * bpp;
            unsafe {
                core::ptr::write_volatile(ptr.add(off), b);
                if bpp > 1 {
                    core::ptr::write_volatile(ptr.add(off + 1), g);
                }
                if bpp > 2 {
                    core::ptr::write_volatile(ptr.add(off + 2), r);
                }
            }
        }
    }
}

/// Poll do buffer aux (bit5) — fallback se IRQ12 falhar no QEMU/WHPX.
pub fn mouse_poll_bytes() {
    use x86_64::instructions::port::Port;
    for _ in 0..16 {
        let status: u8 = unsafe { Port::<u8>::new(0x64).read() };
        if status & 0x01 == 0 {
            break;
        }
        if status & 0x20 == 0 {
            // Teclado — não consumir aqui
            break;
        }
        let byte: u8 = unsafe { Port::<u8>::new(0x60).read() };
        mouse_feed_byte(byte);
    }
}

/// Log periódico do status 8042 (diagnóstico QEMU grab / IRQ).
pub fn mouse_log_status(tag: &str) {
    use x86_64::instructions::port::Port;
    let status: u8 = unsafe { Port::<u8>::new(0x64).read() };
    crate::slog_nano!(
        "MOUSE",
        "info",
        "{} status={:#04x} obf={} ibf={} aux={} pos={}x{}",
        tag,
        status,
        status & 1,
        (status >> 1) & 1,
        (status >> 5) & 1,
        MOUSE_ABS_X.load(Ordering::Relaxed),
        MOUSE_ABS_Y.load(Ordering::Relaxed)
    );
}

extern "x86-interrupt" fn mouse_interrupt_handler(_stack_frame: InterruptStackFrame) {
    use x86_64::instructions::port::Port;
    let status: u8 = unsafe { Port::<u8>::new(0x64).read() };
    if status & 0x01 != 0 && status & 0x20 != 0 {
        let byte: u8 = unsafe { Port::<u8>::new(0x60).read() };
        mouse_feed_byte(byte);
    }
    send_eoi(44);
}

extern "x86-interrupt" fn unhandled_interrupt_handler(stack_frame: InterruptStackFrame) {
    crate::slog_nano!("IRQ", "info", "Interrupção não tratada ip={:#x}", stack_frame.instruction_pointer.as_u64());
    if crate::apic::USING_APIC.load(Ordering::Relaxed) {
        unsafe { crate::apic::apic_eoi(); }
    } else {
        unsafe {
            core::arch::asm!("out 0x20, al", in("al") 0x20u8, options(nostack, preserves_flags));
            core::arch::asm!("out 0xA0, al", in("al") 0x20u8, options(nostack, preserves_flags));
        }
    }
}

// IPI handlers para SMP
extern "x86-interrupt" fn ipi_reschedule_handler(_stack_frame: InterruptStackFrame) {
    IPI_RESCHEDULE.fetch_add(1, Ordering::Relaxed);
    crate::slog_nano!("IPI", "info", "Reschedule received on CPU {}", crate::smp::percpu::cpu_id());
    unsafe { crate::apic::apic_eoi(); }
}

extern "x86-interrupt" fn ipi_halt_handler(_stack_frame: InterruptStackFrame) {
    IPI_HALT.fetch_add(1, Ordering::Relaxed);
    crate::slog_nano!("IPI", "info", "Halt received on CPU {}", crate::smp::percpu::cpu_id());
    unsafe { crate::apic::apic_eoi(); }
    loop { x86_64::instructions::hlt(); }
}

extern "x86-interrupt" fn ipi_call_function_handler(_stack_frame: InterruptStackFrame) {
    IPI_CALL_FUNCTION.fetch_add(1, Ordering::Relaxed);
    crate::slog_nano!("IPI", "info", "Call function received on CPU {}", crate::smp::percpu::cpu_id());
    unsafe { crate::apic::apic_eoi(); }
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
        unsafe { idt.general_protection_fault.set_handler_fn(general_protection_fault_handler).set_stack_index(GENERAL_PROTECTION_IST_INDEX); }
        unsafe { idt.page_fault.set_handler_fn(page_fault_handler).set_stack_index(PAGE_FAULT_IST_INDEX); }
        idt.x87_floating_point.set_handler_fn(fpu_error_handler);
        idt.alignment_check.set_handler_fn(alignment_check_handler);
        idt.machine_check.set_handler_fn(machine_check_handler);
        idt.simd_floating_point.set_handler_fn(simd_fp_exception_handler);
        idt.virtualization.set_handler_fn(virtualization_handler);
        idt.security_exception.set_handler_fn(security_exception_handler);

        // Vetor 20, 22-31: reservados pela CPU (não devem disparar)
        // Vetor 21 = SecurityException (já configurado acima)

        // Hardware IRQs — use IST to avoid stack overflow at top boundary
        unsafe { idt[32].set_handler_fn(timer_handler).set_stack_index(TIMER_IST_INDEX); }
        unsafe { idt[33].set_handler_fn(keyboard_interrupt_handler).set_stack_index(TIMER_IST_INDEX); }
        unsafe { idt[44].set_handler_fn(mouse_interrupt_handler).set_stack_index(TIMER_IST_INDEX); }

        // IPI handlers para SMP (vetores 0x80-0x82)
        idt[0x80].set_handler_fn(ipi_reschedule_handler);
        idt[0x81].set_handler_fn(ipi_halt_handler);
        idt[0x82].set_handler_fn(ipi_call_function_handler);

        // Demais vetores (34-255, exceto IPI)
        for i in 34..=255usize {
            if i == 0x80 || i == 0x81 || i == 0x82 {
                continue; // IPI handlers já configurados
            }
            idt[i].set_handler_fn(unhandled_interrupt_handler);
        }

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
    crate::slog_nano!("IDT", "info", "IDT carregada: vetores 0-31 (exceções) + 32-33 (IRQ) + 34-255 (genérico) cobertos.");
}

// ponytail: init_pics removed — kernel só usa APIC

pub fn enable_interrupts() {
    x86_64::instructions::interrupts::enable();
    crate::slog_nano!("CPU", "info", "Interrupcoes de hardware habilitadas (IF=1).");
    println!("[CPU] Interrupcoes de hardware habilitadas (IF=1).");
}
