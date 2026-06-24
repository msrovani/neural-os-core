use core::arch::global_asm;
use core::ptr;
use core::sync::atomic::Ordering;

global_asm!(
    ".intel_syntax noprefix",
    ".section .text.trampoline, \"ax\"",
    ".balign 4096",

    ".globl trampoline_start",
    "trampoline_start:",

    // Header: 48 bytes of patchable u64 fields
    ".globl trampoline_patch_jmp32",
    "trampoline_patch_jmp32: .quad 0",
    ".globl trampoline_patch_jmp64",
    "trampoline_patch_jmp64: .quad 0",
    ".globl trampoline_patch_cr3",
    "trampoline_patch_cr3:   .quad 0",
    ".globl trampoline_patch_stack",
    "trampoline_patch_stack: .quad 0",
    ".globl trampoline_patch_percpu",
    "trampoline_patch_percpu: .quad 0",
    ".globl trampoline_patch_entry",
    "trampoline_patch_entry: .quad 0",

    // Offset constants (single-symbol operands for asm)
    ".set off_gdt_pseudo, trampoline_gdt_pseudo - trampoline_start",
    ".set off_stack,      trampoline_patch_stack - trampoline_start",
    ".set off_cr3,        trampoline_patch_cr3 - trampoline_start",
    ".set off_jmp64,      trampoline_patch_jmp64 - trampoline_start",
    ".set off_stack, trampoline_patch_stack - trampoline_start",
    ".set off_percpu, trampoline_patch_percpu - trampoline_start",
    ".set off_entry, trampoline_patch_entry - trampoline_start",
    ".set off_32,         trampoline_32 - trampoline_start",

    // 16-bit entry: SIPI lands here with CS.base = trampoline phys addr
    ".code16",
    "  cli",
    "  cld",
    "  xor ax, ax",
    "  mov ds, ax",
    "  mov ss, ax",
    "  mov sp, 0x1000",

    // LGDT antes de sair do modo real (GDT via cs:offset)
    "  .byte 0x2E, 0x0F, 0x01, 0x16",
    "  .word off_gdt_pseudo",

    // Load jmp32_val into EBX via cs:[patch_jmp32 - start]
    "  .byte 0x2E, 0x66, 0xA1",
    "  .word trampoline_patch_jmp32 - trampoline_start",
    "  mov ebx, eax",

    // Set PE bit
    "  mov eax, cr0",
    "  or al, 1",
    "  mov cr0, eax",

    // Far jump to 32-bit via retf (push CS=0x08, push EIP=ebx)
    "  .byte 0x66",
    "  push 0x08",
    "  .byte 0x66",
    "  push ebx",
    "  .byte 0x66",
    "  retf",

    // 32-bit protected mode
    ".globl trampoline_32",
    "trampoline_32:",
    ".code32",
    "  mov ax, 0x10",
    "  mov ds, ax",
    "  mov es, ax",
    "  mov ss, ax",
    "  mov fs, ax",
    "  mov gs, ax",

    // EBX = jmp32_val, phys_base = EBX - offset(trampoline_32 - trampoline_start)
    // Emit SUB EBX, imm32 via raw bytes (GAS Intel treats .set symbols as memory, not immediate)
    "  .byte 0x81, 0xEB",
    "  .4byte trampoline_32 - trampoline_start",

    // Stack: use top of trampoline page (identity-mapped, below 2MB)
    // 64-bit mode will reload RSP from the heap via the patch field
    "  lea esp, [ebx + 0x1000]",

    // PAE
    "  mov eax, cr4",
    "  or eax, 0x20",
    "  mov cr4, eax",

    // CR3
    "  mov eax, [ebx + off_cr3]",
    "  mov cr3, eax",

    // EFER.LME
    "  mov ecx, 0xC0000080",
    "  rdmsr",
    "  or eax, 0x100",
    "  wrmsr",

    // Paging
    "  mov eax, cr0",
    "  or eax, 0x80000000",
    "  mov cr0, eax",

    // Far jump to 64-bit
    "  mov eax, [ebx + off_jmp64]",
    "  push 0x18",
    "  push eax",
    "  retf",

    // 64-bit long mode
    ".globl trampoline_64",
    "trampoline_64:",
    ".code64",
    // Segments already valid from 32-bit mode; RBX holds trampoline phys base
    // Use [rbx + offset] instead of RIP-relative (which breaks when code is relocated)

    // Enable NXE (No-Execute) in EFER — required for page tables with NX bit set
    "  mov ecx, 0xC0000080",
    "  rdmsr",
    "  or eax, 0x800",
    "  wrmsr",

    // Stack
    "  mov rax, [rbx + off_stack]",
    "  mov rsp, rax",

    // GS.base = PerCpu
    "  mov rax, [rbx + off_percpu]",
    "  test rax, rax",
    "  jz 1f",
    "  mov rcx, 0xC0000101",
    "  mov rdx, rax",
    "  shr rdx, 32",
    "  wrmsr",
    "1:",
    "  mov rax, [rbx + off_entry]",
    "  test rax, rax",
    "  jz 2f",
    "  call rax",
    "2: hlt",
    "  jmp 2b",

    // GDT
    ".balign 8",
    ".globl trampoline_gdt",
    "trampoline_gdt:",
    "  .quad 0x0000000000000000",
    "  .quad 0x00CF9A000000FFFF",
    "  .quad 0x00CF92000000FFFF",
    "  .quad 0x00209A0000000000",
    "trampoline_gdt_end:",

    ".balign 8",
    ".globl trampoline_gdt_pseudo",
    "trampoline_gdt_pseudo:",
    "  .word trampoline_gdt_end - trampoline_gdt - 1",
    "  .long 0x00000000",

    ".globl trampoline_end",
    "trampoline_end:",
);

extern "C" {
    static trampoline_start: u8;
    static trampoline_patch_jmp32: u8;
    static trampoline_patch_jmp64: u8;
    static trampoline_patch_cr3: u8;
    static trampoline_patch_stack: u8;
    static trampoline_patch_percpu: u8;
    static trampoline_patch_entry: u8;
    static trampoline_32: u8;
    static trampoline_64: u8;
    static trampoline_gdt: u8;
    static trampoline_gdt_pseudo: u8;
    static trampoline_end: u8;
}

fn offset_of(from: *const u8, to: *const u8) -> usize {
    (to as usize).wrapping_sub(from as usize)
}

pub unsafe fn init_trampoline(
    phys_addr: u64,
    cr3_value: u64,
    ap_stack: u64,
    percpu_addr: u64,
    entry_fn: extern "C" fn(u64) -> !,
) {
    let tramp_virt = (phys_addr + crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed)) as *mut u8;
    let size = offset_of(&trampoline_start as *const u8, &trampoline_end as *const u8);
    ptr::copy_nonoverlapping(&trampoline_start as *const u8, tramp_virt, size);

    let patch64 = |sym: *const u8, val: u64| {
        let off = offset_of(&trampoline_start as *const u8, sym);
        ptr::write_volatile(tramp_virt.add(off) as *mut u64, val);
    };

    let jmp32_val = phys_addr + offset_of(&trampoline_start as *const u8, &trampoline_32 as *const u8) as u64;
    let jmp64_val = phys_addr + offset_of(&trampoline_start as *const u8, &trampoline_64 as *const u8) as u64;

    patch64(&trampoline_patch_jmp32 as *const u8, jmp32_val);
    patch64(&trampoline_patch_jmp64 as *const u8, jmp64_val);
    patch64(&trampoline_patch_cr3 as *const u8, cr3_value);
    patch64(&trampoline_patch_stack as *const u8, ap_stack);
    patch64(&trampoline_patch_percpu as *const u8, percpu_addr);
    patch64(&trampoline_patch_entry as *const u8, entry_fn as u64);

    // Patch GDT base in pseudo-descriptor (offset + 2)
    let gdt_phys = phys_addr + offset_of(&trampoline_start as *const u8, &trampoline_gdt as *const u8) as u64;
    let gp_off = offset_of(&trampoline_start as *const u8, &trampoline_gdt_pseudo as *const u8);
    ptr::write_volatile(tramp_virt.add(gp_off + 2) as *mut u32, gdt_phys as u32);
}

pub unsafe fn trampoline_size() -> usize {
    offset_of(&trampoline_start as *const u8, &trampoline_end as *const u8)
}
