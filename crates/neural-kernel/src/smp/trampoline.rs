//! SMP trampoline — N1.0: stub sem global_asm.
//! rustc 1.98 + target_feature=sse rejeita o asm multi-mode legado
//! com `error: offset is not a multiple of 16` (sem spans).
//! API preservada; SIPI/AP fica inativo até encoding raw.

use core::sync::atomic::Ordering;

#[repr(C, align(4096))]
struct TrampolinePage([u8; 4096]);

static TRAMPOLINE_PAGE: TrampolinePage = TrampolinePage([0u8; 4096]);

const OFF_JMP32: usize = 0;
const OFF_JMP64: usize = 16;
const OFF_CR3: usize = 32;
const OFF_STACK: usize = 48;
const OFF_PERCPU: usize = 64;
const OFF_ENTRY: usize = 80;
const OFF_GDT: usize = 96;
const OFF_GDT_PSEUDO: usize = 128;
const TRAMP_USED: usize = 160;

pub unsafe fn init_trampoline(
    phys_addr: u64,
    cr3_value: u64,
    ap_stack: u64,
    percpu_addr: u64,
    entry_fn: extern "C" fn(u64) -> !,
) {
    let tramp_virt =
        (phys_addr + crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed)) as *mut u8;
    core::ptr::copy_nonoverlapping(TRAMPOLINE_PAGE.0.as_ptr(), tramp_virt, TRAMP_USED);

    let write_u64 = |off: usize, val: u64| {
        core::ptr::write_volatile(tramp_virt.add(off) as *mut u64, val);
    };

    write_u64(OFF_JMP32, phys_addr + OFF_GDT as u64);
    write_u64(OFF_JMP64, phys_addr + OFF_GDT as u64);
    write_u64(OFF_CR3, cr3_value);
    write_u64(OFF_STACK, ap_stack);
    write_u64(OFF_PERCPU, percpu_addr);
    write_u64(OFF_ENTRY, entry_fn as u64);

    let gdt_phys = phys_addr + OFF_GDT as u64;
    core::ptr::write_volatile(tramp_virt.add(OFF_GDT_PSEUDO + 2) as *mut u32, gdt_phys as u32);

    crate::serial_println!("[SMP] trampoline STUB (global_asm disabled — LLVM align16/SSE)");
}

pub unsafe fn trampoline_size() -> usize {
    4096
}
