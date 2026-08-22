//! Residuals bin-only de interrupções (Ring3/TSS per-process, syscall 0x90,
//! hooks demand-page/allocator, PIC fallback com STI) — movidos do antigo
//! `crate::interrupts` (agora facade de `k_nano::interrupts`). Code MOVE puro:
//! mesma lógica, mesmos gates. Nenhum item aqui existe em k_nano (R0).

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};
use lazy_static::lazy_static;
use x86_64::instructions::segmentation::Segment;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::idt::{InterruptStackFrame, PageFaultErrorCode};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

use k_nano::interrupts::{PAGE_FAULT_COUNT, PIC_1_OFFSET, PIC_2_OFFSET};

const DOUBLE_FAULT_IST_INDEX: u16 = 0;
const PAGE_FAULT_IST_INDEX: u16 = 1;
const GENERAL_PROTECTION_IST_INDEX: u16 = 2;
const TIMER_IST_INDEX: u16 = 3;

// --------------------------------------------------------------------------
// P6: TSS per-process (Ring3) — GDT user + TSS.RSP0 para trap de CPL=3.
// --------------------------------------------------------------------------

// Wrapper com interior mutability para TSS — só mutado single-threaded
// durante transições Ring3 (CLI), portanto Sync é seguro.
struct TssCell(UnsafeCell<TaskStateSegment>);
unsafe impl Sync for TssCell {}

impl TssCell {
    fn new(tss: TaskStateSegment) -> Self {
        Self(UnsafeCell::new(tss))
    }

    /// Atualiza RSP0 (per-process). Single-threaded durante Ring3.
    fn set_rsp0(&self, stack_top: VirtAddr) {
        unsafe { (*self.0.get()).privilege_stack_table[0] = stack_top; }
    }
}

impl core::ops::Deref for TssCell {
    type Target = TaskStateSegment;
    fn deref(&self) -> &TaskStateSegment {
        unsafe { &*self.0.get() }
    }
}

/// Per-process TSS array for Ring3 isolation (F1.2).
/// Each process gets its own TSS with dedicated RSP0.
/// MAX_PROCS = 8 (configurable).
const MAX_PROCS: usize = 8;

lazy_static! {
    /// Array of TSS cells — one per process.
    /// Index 0 = kernel/initial process; 1..MAX_PROCS-1 = user processes.
    static ref TSS_ARRAY: [TssCell; MAX_PROCS] = {
        let mut arr: [Option<TssCell>; MAX_PROCS] = [None, None, None, None, None, None, None, None];
        for i in 0..MAX_PROCS {
            let mut tss = TaskStateSegment::new();
            // RSP0: stack kernel ao trapear de CPL=3 (int 0x90 / exceções)
            tss.privilege_stack_table[0] = {
                const STACK_SIZE: usize = 4096 * 4;
                static mut STACKS: [[u8; 4096 * 4]; MAX_PROCS] = [[0; 4096 * 4]; MAX_PROCS];
                let stack_start = unsafe { VirtAddr::from_ptr(core::ptr::addr_of!(STACKS[i])) };
                stack_start + STACK_SIZE
            };
            tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
                const STACK_SIZE: usize = 4096 * 5;
                static mut IST_DF: [[u8; 4096 * 5]; MAX_PROCS] = [[0; 4096 * 5]; MAX_PROCS];
                let stack_start = unsafe { VirtAddr::from_ptr(core::ptr::addr_of!(IST_DF[i])) };
                stack_start + STACK_SIZE
            };
            tss.interrupt_stack_table[PAGE_FAULT_IST_INDEX as usize] = {
                const STACK_SIZE: usize = 4096 * 4;
                static mut IST_PF: [[u8; 4096 * 4]; MAX_PROCS] = [[0; 4096 * 4]; MAX_PROCS];
                let stack_start = unsafe { VirtAddr::from_ptr(core::ptr::addr_of!(IST_PF[i])) };
                stack_start + STACK_SIZE
            };
            tss.interrupt_stack_table[GENERAL_PROTECTION_IST_INDEX as usize] = {
                const STACK_SIZE: usize = 4096 * 4;
                static mut IST_GP: [[u8; 4096 * 4]; MAX_PROCS] = [[0; 4096 * 4]; MAX_PROCS];
                let stack_start = unsafe { VirtAddr::from_ptr(core::ptr::addr_of!(IST_GP[i])) };
                stack_start + STACK_SIZE
            };
            tss.interrupt_stack_table[TIMER_IST_INDEX as usize] = {
                const STACK_SIZE: usize = 4096 * 4;
                static mut IST_TIMER: [[u8; 4096 * 4]; MAX_PROCS] = [[0; 4096 * 4]; MAX_PROCS];
                let stack_start = unsafe { VirtAddr::from_ptr(core::ptr::addr_of!(IST_TIMER[i])) };
                stack_start + STACK_SIZE
            };
            arr[i] = Some(TssCell::new(tss));
        }
        // Safe: all elements initialized
        core::array::from_fn(|i| arr[i].take().unwrap())
    };
}

/// Current process index for TSS selection (0 = kernel).
static CURRENT_PROC_IDX: AtomicUsize = AtomicUsize::new(0);

/// Atualiza RSP0 do TSS para o processo atual (Phase 2).
pub fn set_rsp0(stack_top: VirtAddr) {
    let idx = CURRENT_PROC_IDX.load(Ordering::Relaxed);
    TSS_ARRAY[idx].set_rsp0(stack_top);
}

/// Switch to process TSS (loads new TSS selector via LTR).
/// Called during context switch to Ring3 process.
pub fn switch_to_proc_tss(proc_idx: usize) {
    if proc_idx >= MAX_PROCS {
        return;
    }
    CURRENT_PROC_IDX.store(proc_idx, Ordering::SeqCst);
    // LTR with the TSS selector for this process
    // Note: GDT has only one TSS entry; for true per-process TSS we'd need
    // multiple TSS descriptors in GDT. For now, we update the single TSS's RSP0.
    // True per-process TSS requires GDT expansion (future).
    let _ = proc_idx;
}

/// Retorna referência ao TSS do processo atual para init da GDT.
fn tss_ref() -> &'static TaskStateSegment {
    &TSS_ARRAY[0]
}

lazy_static! {
    static ref GDT: (GlobalDescriptorTable, Selectors) = {
        let mut gdt = GlobalDescriptorTable::new();
        // GDT max 8 slots: null + 4 usersegs + TSS(2) = 7
        let code_selector = gdt.add_entry(Descriptor::kernel_code_segment());
        let data_selector = gdt.add_entry(Descriptor::kernel_data_segment());
        let user_code_selector = gdt.add_entry(Descriptor::user_code_segment());
        let user_data_selector = gdt.add_entry(Descriptor::user_data_segment());
        let tss_selector = gdt.add_entry(Descriptor::tss_segment(tss_ref()));
        (
            gdt,
            Selectors {
                code_selector,
                data_selector,
                user_code_selector,
                user_data_selector,
                tss_selector,
            },
        )
    };
}

struct Selectors {
    code_selector: SegmentSelector,
    data_selector: SegmentSelector,
    user_code_selector: SegmentSelector,
    user_data_selector: SegmentSelector,
    tss_selector: SegmentSelector,
}

/// Seletor CS Ring0 (P6 / enter_user_mode).
pub fn kernel_code_selector() -> SegmentSelector {
    GDT.1.code_selector
}

/// Seletor DS Ring0.
pub fn kernel_data_selector() -> SegmentSelector {
    GDT.1.data_selector
}

/// Seletor CS Ring3 — **GDT carregada** em `k_nano::interrupts` (SESSION_278).
pub fn user_code_selector() -> SegmentSelector {
    k_nano::interrupts::user_code_selector()
}

/// Seletor DS/SS Ring3 — GDT k_nano (não a GDT fantasma local).
pub fn user_data_selector() -> SegmentSelector {
    k_nano::interrupts::user_data_selector()
}

// --------------------------------------------------------------------------
// Serial lock-free (exception / P6 abort path — sem Mutex; evita #DF cascade)
// --------------------------------------------------------------------------

// ponytail: lock-free serial write for exception context (avoids #DF cascade)
fn putc(c: u8) {
    unsafe { core::arch::asm!("out dx, al", in("dx") 0x3F8u16, in("al") c, options(nostack, preserves_flags)); }
}
/// Lock-free serial write (exception / P6 abort path — sem Mutex).
pub(crate) fn puts(s: &[u8]) { for &c in s { putc(c); } }
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
    putc(b'\n');
}

// --------------------------------------------------------------------------
// Hooks bin-only instalados sobre o IDT de k_nano (patch_idt):
// #GP/#PF com abort P6 + demand-page/allocator.
// --------------------------------------------------------------------------

extern "x86-interrupt" fn invalid_opcode_handler(f: InterruptStackFrame) {
    if crate::user_mode::demo_active() {
        dump_exception("#UD", &f, None);
        crate::user_mode::fault_abort("P6 #UD in Ring3 demo");
    }
    dump_exception("#UD", &f, None);
    loop { x86_64::instructions::hlt(); }
}

extern "x86-interrupt" fn general_protection_fault_handler(f: InterruptStackFrame, code: u64) {
    if crate::user_mode::demo_active() {
        dump_exception("#GP", &f, Some(code));
        crate::user_mode::fault_abort("P6 #GP in Ring3 demo");
    }
    dump_exception("#GP", &f, Some(code));
    loop { x86_64::instructions::hlt(); }
}

extern "x86-interrupt" fn page_fault_handler(f: InterruptStackFrame, code: PageFaultErrorCode) {
    let cr2 = x86_64::registers::control::Cr2::read();
    // P6 abort em curso: NUNCA return/retry (evita storm CR2=ip err=0x10).
    if crate::user_mode::demo_active() {
        dump_exception("#PF", &f, Some(code.bits() as u64));
        puts(b" CR2=");
        puthex(cr2.as_u64());
        putc(b'\n');
        crate::user_mode::fault_abort("P6 #PF in Ring3 demo");
    }
    // P7: demand-paging — cura lazy map e retorna (retry insn); sem hlt.
    if crate::demand_page::try_handle_fault(cr2.as_u64()) {
        return;
    }
    // Heap Tier-1: buracos apos demos CR3/AS ou map incompleto — cura 4KiB e retry.
    if crate::allocator::try_fault_in_heap(cr2.as_u64()) {
        return;
    }
    dump_exception("#PF", &f, Some(code.bits() as u64));
    puts(b" CR2="); puthex(cr2.as_u64()); putc(b'\n');
    let count = PAGE_FAULT_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    if count <= 10 {
        return;
    }
    loop { x86_64::instructions::hlt(); }
}

// --------------------------------------------------------------------------
// patch_idt — overlays bin-only no IDT carregado por k_nano::interrupts::init_idt().
// k_nano não expõe o IDT (lazy_static privado): patcheamos o gate de 16 bytes
// no slot do vetor via IDTR (sidt). Mesmos gates do antigo IDT do bin:
//   - 0x90 syscall (DPL=3, sem IST)
//   - 0x0D #GP (IST 2 → hardware 3)
//   - 0x0E #PF (IST 1 → hardware 2)
// --------------------------------------------------------------------------

/// Escreve um gate de interrupção 64-bit (16 bytes) no slot `vec` do IDT em `base`.
/// `ist`: índice de software do IST (0 = sem IST); `dpl`: 0..3.
unsafe fn write_gate(idt_base: u64, vec: u8, handler: u64, ist: u8, dpl: u8) {
    let sel = x86_64::instructions::segmentation::CS::get_reg().0;
    let p = (idt_base + (vec as u64) * 16) as *mut u8;
    core::ptr::write_volatile(p, (handler & 0xFF) as u8);
    core::ptr::write_volatile(p.add(1), ((handler >> 8) & 0xFF) as u8);
    core::ptr::write_volatile(p.add(2), (sel & 0xFF) as u8);
    core::ptr::write_volatile(p.add(3), ((sel >> 8) & 0xFF) as u8);
    // byte 4: IST (hardware = software+1) — byte 5: P=1, DPL, tipo 0xE (int gate)
    core::ptr::write_volatile(p.add(4), if ist == 0 { 0 } else { ist + 1 });
    core::ptr::write_volatile(p.add(5), 0x8E | (dpl << 5));
    core::ptr::write_volatile(p.add(6), ((handler >> 16) & 0xFF) as u8);
    core::ptr::write_volatile(p.add(7), ((handler >> 24) & 0xFF) as u8);
    core::ptr::write_volatile(p.add(8), ((handler >> 32) & 0xFF) as u8);
    core::ptr::write_volatile(p.add(9), ((handler >> 40) & 0xFF) as u8);
    core::ptr::write_volatile(p.add(10), ((handler >> 48) & 0xFF) as u8);
    core::ptr::write_volatile(p.add(11), ((handler >> 56) & 0xFF) as u8);
    for i in 12..16 {
        core::ptr::write_volatile(p.add(i), 0u8);
    }
}

/// Aplica os overlays bin-only sobre o IDT de k_nano (chamar logo após
/// `k_nano::interrupts::init_idt()` no BSP).
pub fn patch_idt() {
    let idtr = x86_64::instructions::tables::sidt();
    let base = idtr.base.as_u64();
    let limit = idtr.limit as u64;
    // Bounds check: precisamos de pelo menos o vetor 0x90 (16 bytes cada).
    if base == 0 || limit < (0x90u64 * 16 + 15) {
        k_nano::slog_bin!("IDT", "warn", "patch_idt: IDTR fora do esperado (base={:#x} limit={}), pulando overlays", base, limit);
        return;
    }
    unsafe {
        write_gate(base, 0x06, invalid_opcode_handler as *const () as u64, 0, 0);
        write_gate(base, 0x0D, general_protection_fault_handler as *const () as u64, GENERAL_PROTECTION_IST_INDEX as u8, 0);
        write_gate(base, 0x0E, page_fault_handler as *const () as u64, PAGE_FAULT_IST_INDEX as u8, 0);
        // MVP C / P6: soft-syscall (0x90) — Cap gate; DPL=3 para int de Ring3
        write_gate(base, 0x90, crate::syscall::syscall_int_handler as *const () as u64, 0, 3);
    }
    k_nano::slog_bin!("IDT", "info", "patch_idt: overlays bin instalados (0x90 syscall DPL3, #UD/#GP/#PF hooks P6+demand).");
}

// --------------------------------------------------------------------------
// PIC fallback + STI (k_nano::remap_pic_pit_fallback NÃO faz STI; este sim).
// --------------------------------------------------------------------------

/// PIC8259 mínimo + PIT + STI — acordável em hlt() se APIC nunca subir.
/// Se PlatformAgent/`init_apic` rodar depois, `disable_pic()` mascara o PIC (transição OK).
pub unsafe fn init_pic_fallback_and_sti() {
    if crate::apic::USING_APIC.load(Ordering::Relaxed) {
        k_nano::interrupts::enable_interrupts();
        return;
    }

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
    // Mask: IRQ0 (PIT) + IRQ2 (cascade) abertos; resto mascarado
    core::arch::asm!("out dx, al", in("dx") 0x21u16, in("al") 0xFAu8, options(nostack, preserves_flags));
    core::arch::asm!("out dx, al", in("dx") 0xA1u16, in("al") 0xFFu8, options(nostack, preserves_flags));

    crate::apic::pit_init();
    k_nano::slog_bin!("PIC", "info", "Fallback 8259 remapido (IRQ0→vec32). STI antes do scheduler.");
    k_nano::interrupts::enable_interrupts();
}
