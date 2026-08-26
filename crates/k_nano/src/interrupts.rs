//! Interrupt and exception handling — IDT, GDT, TSS, PIC, handlers.

use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use lazy_static::lazy_static;
// ponytail: PICS/pic8259 removed — kernel só usa APIC
use x86_64::instructions::segmentation::{CS, DS, ES, SS, Segment};
use x86_64::structures::gdt::SegmentSelector;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = 40;

pub static TIMER_TICKS: AtomicUsize = AtomicUsize::new(0);
/// Timer frequency em Hz (calibrado na inicializaçao). Fallback 18 Hz (PIT).
pub static TIMER_HZ: AtomicU64 = AtomicU64::new(18);
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
pub static FB_RGB_ORDER: AtomicBool = AtomicBool::new(false);
/// Spinlock for cursor-area FB access between IRQ handler and compositor swap().
/// IRQ side uses swap(true) — never blocks (skips draw if locked).
/// Compositor side spins until lock acquired.
pub static CURSOR_LOCK: AtomicBool = AtomicBool::new(false);
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

// BSP TSS only in BSS (T-037). APs: heap via expand_gdt_aps.
static mut BSP_TSS_STORAGE: TaskStateSegment = TaskStateSegment::new();
static AP_TSS_PTR: AtomicUsize = AtomicUsize::new(0);
static AP_TSS_LEN: AtomicUsize = AtomicUsize::new(0);

lazy_static! {
    static ref TSS: &'static TaskStateSegment = {
        let tss = unsafe { &mut BSP_TSS_STORAGE };
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
        // RSP0: stack kernel em trap CPL=3 (int 0x90 / exceções sem IST).
        // SESSION_278: RSP0=0 → #PF no int 0x90 após iretq Ring3.
        tss.privilege_stack_table[0] = {
            const STACK_SIZE: usize = 4096 * 4;
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];
            let stack_start = VirtAddr::from_ptr(core::ptr::addr_of!(STACK));
            stack_start + STACK_SIZE
        };
        tss
    };
}

/// CS Ring3 (RPL=3 embutido).
pub fn user_code_selector() -> SegmentSelector {
    crate::gdt::sels().user_code_selector
}

/// DS/SS Ring3 (RPL=3 embutido).
pub fn user_data_selector() -> SegmentSelector {
    crate::gdt::sels().user_data_selector
}

/// DS Ring0.
pub fn kernel_data_selector() -> SegmentSelector {
    crate::gdt::sels().data_selector
}

pub fn shared_tss_selector() -> SegmentSelector {
    crate::gdt::sels().tss_selectors[0]
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

fn putdec(mut n: u64) {
    if n == 0 { putc(b'0'); return; }
    let mut buf = [0u8; 20];
    let mut i = 20;
    while n > 0 { i -= 1; buf[i] = (n % 10) as u8 + b'0'; n /= 10; }
    for &c in &buf[i..] { putc(c); }
}

fn dump_exception(name: &str, stack_frame: &InterruptStackFrame, error_code: Option<u64>) {
    puts(b"[EXC] ");
    puts(name.as_bytes());
    puts(b" ip="); puthex(stack_frame.instruction_pointer.as_u64());
    puts(b" fl="); puthex(stack_frame.cpu_flags as u64);
    puts(b" sp="); puthex(stack_frame.stack_pointer.as_u64());
    if let Some(code) = error_code { puts(b" err="); puthex(code); }
    // CR3 real para walk offline das page tables (diagnóstico #PF-storm).
    let cr3: u64;
    unsafe { core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack, preserves_flags)); }
    puts(b" cr3="); puthex(cr3);
    putc(b'\n');
    // Diagnóstico de crash: 24 qwords do stack interrompido (endereços crescentes).
    // Os valores 0xffffffff80xxxxxx são return addresses/locals resolvíveis
    // offline contra o kernel.elf (.symtab). Para no limite da página p/ não
    // tocar memória não-mapeada e cascata em #DF.
    let sp = stack_frame.stack_pointer.as_u64();
    puts(b"[EXC] stk:");
    let page_end = (sp | 0xFFF) + 1;
    for i in 0..24u64 {
        let a = sp + i * 8;
        if a + 8 > page_end { break; }
        let v = unsafe { core::ptr::read_volatile(a as *const u64) };
        putc(b' '); puthex(v);
    }
    putc(b'\n');
}

extern "x86-interrupt" fn divide_error_handler(f: InterruptStackFrame) { dump_exception("#DE", &f, None); loop { x86_64::instructions::hlt(); } }
extern "x86-interrupt" fn debug_handler(f: InterruptStackFrame) { dump_exception("#DB", &f, None); loop { x86_64::instructions::hlt(); } }
extern "x86-interrupt" fn nmi_handler(f: InterruptStackFrame) { dump_exception("#NMI", &f, None); loop { x86_64::instructions::hlt(); } }
extern "x86-interrupt" fn breakpoint_handler(_f: InterruptStackFrame) { puts(b"[EXC] #BP Breakpoint\n"); }
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
    if count <= 3 {
        return;
    }
    puts(b"[SELF-HEAL] #PF threshold exceeded -- halting.\n");
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
        puts(b"[TIMER] Interrupt fired! tick="); putdec(ticks as u64); putc(b'\n');
    }
    // Só marca; o poll dos futures roda no idle do scheduler (fora do IRQ).
    crate::async_rt::request_wake_processing();
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
    // ponytail: lock-free byte-level logging for IRQ context
    if n < 64 {
        puts(b"[MOUSE] byte["); putdec(n as u64); puts(b"]=");
        puthex(byte as u64); puts(b" phase=");
        putdec(MOUSE_PHASE.load(Ordering::Relaxed) as u64);
        puts(b" aux\n");
    }

    let phase = MOUSE_PHASE.load(Ordering::Relaxed);
    if phase == 0 && (byte & 0x08) == 0 {
        if n < 64 {
            puts(b"[MOUSE] byte["); putdec(n as u64); puts(b"] discard (no sync bit3)\n");
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
                puts(b"[MOUSE] CLICK press="); puthex(pressed as u64);
                puts(b" @"); putdec(nx as u64); putc(b','); putdec(ny as u64);
                puts(b" (irq)\n");
            } else if (prev & !btn) != 0 {
                puts(b"[MOUSE] CLICK release="); puthex((prev & !btn) as u64);
                puts(b" @"); putdec(nx as u64); putc(b','); putdec(ny as u64);
                puts(b" (irq)\n");
            }

            let pkt_n = n / 3;
            if pkt_n < 32 || pressed != 0 {
                puts(b"[MOUSE] pkt #"); putdec(pkt_n as u64);
                puts(b" raw="); puthex(b0 as u64); puthex(b1 as u64); puthex(b2 as u64);
                puts(b" d="); putdec(dx as u64); putc(b','); putdec(dy as u64);
                puts(b" pos="); putdec(nx as u64); putc(b'x'); putdec(ny as u64);
                puts(b" btn="); puthex(btn as u64); putc(b'\n');
            }
        }
        _ => {}
    }
}

/// Pinta seta mínima no FB físico (visível mesmo com Hermes bloqueado).
fn mouse_paint_irq_cursor(x: u32, y: u32) {
    // Try-lock from IRQ: skip if compositor is rendering (never spin in IRQ)
    if CURSOR_LOCK.swap(true, Ordering::Acquire) {
        return;
    }
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
    let rgb_order = FB_RGB_ORDER.load(Ordering::Relaxed);
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
                if rgb_order {
                    core::ptr::write_volatile(ptr.add(off), r);
                    if bpp > 1 {
                        core::ptr::write_volatile(ptr.add(off + 1), g);
                    }
                    if bpp > 2 {
                        core::ptr::write_volatile(ptr.add(off + 2), b);
                    }
                } else {
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
    CURSOR_LOCK.store(false, Ordering::Release);
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
/// ponytail: lock-free putc/puts — this may be called from IRQ context
pub fn mouse_log_status(tag: &str) {
    use x86_64::instructions::port::Port;
    let status: u8 = unsafe { Port::<u8>::new(0x64).read() };
    puts(b"[MOUSE] "); puts(tag.as_bytes());
    puts(b" status="); puthex(status as u64);
    puts(b" obf="); putdec((status & 1) as u64);
    puts(b" ibf="); putdec(((status >> 1) & 1) as u64);
    puts(b" aux="); putdec(((status >> 5) & 1) as u64);
    puts(b" pos="); putdec(MOUSE_ABS_X.load(Ordering::Relaxed) as u64);
    putc(b'x'); putdec(MOUSE_ABS_Y.load(Ordering::Relaxed) as u64); putc(b'\n');
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

/// HDA audio capture interrupt handler (SD0 - vector 0x30)
extern "x86-interrupt" fn hda_interrupt_handler(_stack_frame: InterruptStackFrame) {
    // Delegate to HDA driver
    unsafe { crate::audio::hda::hda_irq_handler(); }
    // EOI via APIC
    if crate::apic::USING_APIC.load(Ordering::Relaxed) {
        unsafe { crate::apic::apic_eoi(); }
    }
}

extern "x86-interrupt" fn unhandled_interrupt_handler(stack_frame: InterruptStackFrame) {
    puts(b"[IRQ] Unhandled ip="); puthex(stack_frame.instruction_pointer.as_u64()); putc(b'\n');
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
    puts(b"[IPI] Reschedule on CPU "); putdec(crate::smp::percpu::cpu_id()); putc(b'\n');
    unsafe { crate::apic::apic_eoi(); }
}

extern "x86-interrupt" fn ipi_halt_handler(_stack_frame: InterruptStackFrame) {
    IPI_HALT.fetch_add(1, Ordering::Relaxed);
    puts(b"[IPI] Halt on CPU "); putdec(crate::smp::percpu::cpu_id()); putc(b'\n');
    unsafe { crate::apic::apic_eoi(); }
    loop { x86_64::instructions::hlt(); }
}

extern "x86-interrupt" fn ipi_call_function_handler(_stack_frame: InterruptStackFrame) {
    IPI_CALL_FUNCTION.fetch_add(1, Ordering::Relaxed);
    puts(b"[IPI] Call function on CPU "); putdec(crate::smp::percpu::cpu_id()); putc(b'\n');
    unsafe { crate::apic::apic_eoi(); }
}

// --------------------------------------------------------------------------
// IDT init — cobertura total de 0 a 31 + hardware + syscall
// --------------------------------------------------------------------------
// cfg(not(windows)): InterruptDescriptorTable é repr(C, align(16)) — static em
// lazy_static dispara `offset is not a multiple of 16` no codegen MSVC/COFF do
// host (cargo test). IDT é código de boot, morto em builds de host (windows).
#[cfg(not(windows))]
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
        // HDA audio capture (SD0) - vector 0x30 (48), routed via IOAPIC
        unsafe { idt[0x30].set_handler_fn(hda_interrupt_handler).set_stack_index(TIMER_IST_INDEX); }

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

// --------------------------------------------------------------------------
// Per-AP TSS/IDT support (ADR-0057 WS-F / ADR-0065 FASE 3.1 P13)
// --------------------------------------------------------------------------

/// Per-CPU TSS with IST stacks. Created at runtime for each AP.
pub struct ApTss {
    pub tss: &'static mut TaskStateSegment,
    pub ist_stacks: [VirtAddr; 3], // DF, PF, GP
    pub selector: SegmentSelector,
}

/// Initialize TSS for an AP with given IST stack tops (heap slot).
pub fn init_ap_tss(ap_index: usize, ist_tops: [VirtAddr; 3]) -> ApTss {
    let n = AP_TSS_LEN.load(Ordering::Acquire);
    let base = AP_TSS_PTR.load(Ordering::Acquire) as *mut TaskStateSegment;
    assert!(!base.is_null() && ap_index < n, "T-037: expand_gdt_aps antes do wake");
    let tss = unsafe { &mut *base.add(ap_index) };

    tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = ist_tops[0];
    tss.interrupt_stack_table[PAGE_FAULT_IST_INDEX as usize] = ist_tops[1];
    tss.interrupt_stack_table[GENERAL_PROTECTION_IST_INDEX as usize] = ist_tops[2];
    tss.interrupt_stack_table[TIMER_IST_INDEX as usize] = ist_tops[0];

    let selector = crate::gdt::tss_selector(ap_index + 1).unwrap_or(crate::gdt::sels().tss_selectors[0]);
    ApTss { tss, ist_stacks: ist_tops, selector }
}

/// MADT conhecido: TSS APs no heap + GDT expandida. 1c = no-op.
pub unsafe fn expand_gdt_aps(n_aps: usize) -> bool {
    if n_aps == 0 {
        return true;
    }
    let mut v = alloc::vec::Vec::with_capacity(n_aps);
    for _ in 0..n_aps {
        v.push(TaskStateSegment::new());
    }
    let leaked = alloc::boxed::Box::leak(v.into_boxed_slice());
    AP_TSS_PTR.store(leaked.as_mut_ptr() as usize, Ordering::Release);
    AP_TSS_LEN.store(n_aps, Ordering::Release);
    let bsp = *TSS;
    if !crate::gdt::expand_for_aps(n_aps, leaked, bsp) {
        return false;
    }
    crate::gdt::load();
    let s = crate::gdt::sels();
    x86_64::instructions::segmentation::CS::set_reg(s.code_selector);
    x86_64::instructions::segmentation::SS::set_reg(s.data_selector);
    x86_64::instructions::tables::load_tss(s.tss_selectors[0]);
    crate::slog_nano!("GDT", "info", "BSP lgdt+ltr após expand n_aps={}", n_aps);
    true
}

/// GDT+IDT deste CPU. `ltr`+`sti` só com TSS próprio (silício, não crate-8).
pub unsafe fn ap_load_idt_and_tss(tss_selector: Option<SegmentSelector>) {
    crate::gdt::load();
    let s = crate::gdt::sels();
    x86_64::instructions::segmentation::CS::set_reg(s.code_selector);
    x86_64::instructions::segmentation::SS::set_reg(s.data_selector);
    #[cfg(not(windows))]
    IDT.load();
    if let Some(sel) = tss_selector {
        x86_64::instructions::tables::load_tss(sel);
        x86_64::instructions::interrupts::enable();
    }
}

// --------------------------------------------------------------------------

/// Carrega GDT + TSS + IDT
pub fn init_idt() {
    // T-037: `load()` lia GDT_BASE=0 (lazy_static ainda nao rodou) → lgdt no-op;
    // depois CS::set_reg usava seletores novos contra a GDT do Limine → #GP.
    // Nao usar lazy_static Once em Selectors: abs32 truncava rdi (CR2 low 32-bit).
    let _bsp = &*TSS;
    let sels = unsafe { crate::gdt::build_early(_bsp) };
    crate::gdt::load();
    puts(b"[GDT] lgdt ok\n");
    unsafe {
        CS::set_reg(sels.code_selector);
        puts(b"[GDT] CS ok\n");
        SS::set_reg(sels.data_selector);
        DS::set_reg(sels.data_selector);
        ES::set_reg(sels.data_selector);
        puts(b"[GDT] SS/DS/ES ok\n");
        x86_64::instructions::tables::load_tss(sels.tss_selectors[0]);
        puts(b"[GDT] ltr ok\n");
    }
    #[cfg(not(windows))]
    IDT.load();
    crate::slog_nano!("IDT", "info", "IDT carregada: vetores 0-31 (exceções) + 32-33 (IRQ) + 34-255 (genérico) cobertos.");
}

// ponytail: init_pics removed — kernel só usa APIC

/// HW-3: Remap PIC + program PIT channel 0 — fallback quando APIC/LAPIC timer
/// não está disponível (ex.: `init_acpi()` retornou `None` em HW real sem ACPI).
/// Se o APIC já estiver ativo (`USING_APIC == true`), não faz nada.
pub fn remap_pic_pit_fallback() {
    if crate::apic::USING_APIC.load(Ordering::Relaxed) {
        return;
    }
    unsafe {
        // ICW1: begin init, expect ICW4
        core::arch::asm!("out dx, al", in("dx") 0x20u16, in("al") 0x11u8, options(nostack, preserves_flags));
        core::arch::asm!("out dx, al", in("dx") 0xA0u16, in("al") 0x11u8, options(nostack, preserves_flags));
        // ICW2: remap IRQs → 32–39 / 40–47
        core::arch::asm!("out dx, al", in("dx") 0x21u16, in("al") PIC_1_OFFSET, options(nostack, preserves_flags));
        core::arch::asm!("out dx, al", in("dx") 0xA1u16, in("al") PIC_2_OFFSET, options(nostack, preserves_flags));
        // ICW3: slave on IRQ2
        core::arch::asm!("out dx, al", in("dx") 0x21u16, in("al") 0x04u8, options(nostack, preserves_flags));
        core::arch::asm!("out dx, al", in("dx") 0xA1u16, in("al") 0x02u8, options(nostack, preserves_flags));
        // ICW4: 8086 mode
        core::arch::asm!("out dx, al", in("dx") 0x21u16, in("al") 0x01u8, options(nostack, preserves_flags));
        core::arch::asm!("out dx, al", in("dx") 0xA1u16, in("al") 0x01u8, options(nostack, preserves_flags));
        // Mask: IRQ0 (PIT) + IRQ1 (teclado) + IRQ2 (cascade) abertos; resto mascarado.
        // 0xF8 = 1111_1000 (bits 0,1,2 = 0). ANTES era 0xFA (1111_1010) — bit 1
        // setado = IRQ1 do teclado MASCARADO → sendkey/scancode nunca chegava ao
        // InputAgent (o mouse funciona por polling do status 0x64, não IRQ12).
        core::arch::asm!("out dx, al", in("dx") 0x21u16, in("al") 0xF8u8, options(nostack, preserves_flags));
        core::arch::asm!("out dx, al", in("dx") 0xA1u16, in("al") 0xFFu8, options(nostack, preserves_flags));
    }
    unsafe { crate::apic::pit_init(); }
    crate::slog_nano!("PIC", "info", "Fallback 8259 remapado (IRQ0→vec32). PIT ativo.");
}

pub fn enable_interrupts() {
    x86_64::instructions::interrupts::enable();
    puts(b"[CPU] Interrupcoes de hardware habilitadas (IF=1).\n");
}

/// Estima a frequência da TSC via CPUID leaf 0x15 (Intel) ou fallback.
fn estimate_tsc_hz() -> u64 {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let max = core::arch::x86_64::__cpuid(0).eax;
        if max >= 0x15 {
            let leaf = core::arch::x86_64::__cpuid(0x15);
            let num = leaf.ebx as u64;
            let den = leaf.eax as u64;
            let crystal = leaf.ecx as u64;
            if den > 0 {
                if crystal > 0 {
                    return crystal * num / den;
                }
                // crystal desconhecido — tenta leaf 0x16 como fallback
                if max >= 0x16 {
                    let l16 = core::arch::x86_64::__cpuid(0x16);
                    let base_mhz = (l16.eax & 0xFFFF) as u64;
                    if base_mhz > 0 {
                        return base_mhz * 1_000_000 * num / den;
                    }
                }
                // sem crystal e sem leaf 0x16: assume num/den = 1
            }
        }
        // Fallback: leaf 0x16 base MHz
        if max >= 0x16 {
            let l16 = core::arch::x86_64::__cpuid(0x16);
            let base_mhz = (l16.eax & 0xFFFF) as u64;
            if base_mhz > 0 {
                return base_mhz * 1_000_000;
            }
        }
    }
    2_000_000_000 // fallback conservador 2 GHz
}

/// Calibra TIMER_HZ — método primário: LAPIC_CURRENT_COUNT (HW direto).
/// Fallback: busy-wait 0.5s contando TIMER_TICKS.
/// Fallback final: mantém 18 Hz se tudo falhar.
pub fn calibrate_timer_hz() {
    #[cfg(target_arch = "x86_64")]
    {
        let tsc_hz = estimate_tsc_hz();
        if crate::apic::USING_APIC.load(core::sync::atomic::Ordering::Relaxed) {
            let hz = crate::apic::estimate_timer_hz(tsc_hz);
            if hz > 0 {
                crate::slog_nano!("TIMER", "info", "LAPIC direct: {} Hz (tsc_hz={})", hz, tsc_hz);
                TIMER_HZ.store(hz, core::sync::atomic::Ordering::Relaxed);
                return;
            }
        }

        // Fallback: busy-wait 0.5s
        let sample_tsc = tsc_hz / 2;
        let tsc_start = unsafe { core::arch::x86_64::_rdtsc() };
        let tick_start = TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
        loop {
            let tsc_now = unsafe { core::arch::x86_64::_rdtsc() };
            if tsc_now.wrapping_sub(tsc_start) >= sample_tsc { break; }
            core::hint::spin_loop();
        }
        let tsc_end = unsafe { core::arch::x86_64::_rdtsc() };
        let tick_end = TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
        let elapsed_ticks = tick_end.wrapping_sub(tick_start);
        let elapsed_tsc = tsc_end.wrapping_sub(tsc_start);
        if elapsed_ticks > 0 && elapsed_tsc > 0 {
            let hz_fb = (tsc_hz as u128).saturating_mul(elapsed_ticks as u128) / elapsed_tsc as u128;
            let hz_fb = hz_fb.min(1_000_000).max(1) as u64;
            TIMER_HZ.store(hz_fb, core::sync::atomic::Ordering::Relaxed);
            crate::slog_nano!("TIMER", "info", "calibrado {} Hz via busy-wait (ticks={})", hz_fb, elapsed_ticks);
            return;
        }
        crate::slog_nano!("TIMER", "warn", "calibraçao falhou — mantendo fallback 18 Hz");
    }
    #[cfg(not(target_arch = "x86_64"))]
    { TIMER_HZ.store(18, core::sync::atomic::Ordering::Relaxed); }
}
