//! Trampoline SMP 16→32→64 (ADR-0055/0057).
//! Layout estilo Redox: byte 0 = jmp 16-bit; handshake na página lowmem.
//! Canário QEMU debugcon 0xE9: 0xAA / '3' / '6'.

use core::arch::global_asm;
use core::ptr;
use core::sync::atomic::Ordering;
use x86_64::structures::paging::{
    Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB,
};
use x86_64::{PhysAddr, VirtAddr};

/// Offsets na página SIPI (CS.base = phys, IP=0). Manter alinhado ao `global_asm!`.
pub const OFF_SIPI_HIT: usize = 0x08;
pub const OFF_READY: usize = 0x10;
pub const OFF_JMP32: usize = 0x18;
pub const OFF_JMP64: usize = 0x20;
pub const OFF_CR3: usize = 0x28;
pub const OFF_STACK: usize = 0x30;
pub const OFF_PERCPU: usize = 0x38;
pub const OFF_ENTRY: usize = 0x40;
/// Código 16-bit começa após o header de handshake/patch.
pub const OFF_REAL16: usize = 0x48;

global_asm!(
    ".section .text.trampoline, \"ax\"",
    ".balign 4096",

    ".globl trampoline_start",
    ".code16",
    "trampoline_start:",
    // SIPI: CS=página, IP=0 — o AP *não* pode executar o header como código.
    "  jmp trampoline_16",
    "  .balign 8, 0x90",

    ".globl trampoline_sipi_hit",
    "trampoline_sipi_hit: .quad 0",
    ".globl trampoline_ready",
    "trampoline_ready:    .quad 0",
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

    ".set off_gdt_pseudo, trampoline_gdt_pseudo - trampoline_start",
    ".set off_sipi_hit,   trampoline_sipi_hit - trampoline_start",
    ".set off_ready,      trampoline_ready - trampoline_start",
    ".set off_stack,      trampoline_patch_stack - trampoline_start",
    ".set off_cr3,        trampoline_patch_cr3 - trampoline_start",
    ".set off_jmp64,      trampoline_patch_jmp64 - trampoline_start",
    ".set off_percpu,     trampoline_patch_percpu - trampoline_start",
    ".set off_entry,      trampoline_patch_entry - trampoline_start",

    ".globl trampoline_16",
    "trampoline_16:",
    "  cli",
    "  cld",
    "  xor ax, ax",
    "  mov ds, ax",
    "  mov ss, ax",
    "  mov sp, 0x1000",

    // sipi_hit=1 via CS: (DS=0 ≠ phys do tramp se a página não é 0).
    "  .byte 0x2E, 0x66, 0xC7, 0x06",
    "  .word off_sipi_hit",
    "  .long 1",

    // QEMU debugcon (0xE9). Sem COM1: o AP corrompia o log do BSP.
    "  mov dx, 0xE9",
    "  mov al, 0xAA",
    "  out dx, al",

    "  .byte 0x2E, 0x0F, 0x01, 0x16",
    "  .word off_gdt_pseudo",

    "  .byte 0x2E, 0x66, 0xA1",
    "  .word trampoline_patch_jmp32 - trampoline_start",
    "  mov ebx, eax",

    "  mov eax, cr0",
    "  or al, 1",
    "  mov cr0, eax",

    "  .byte 0x66",
    "  push 0x08",
    "  .byte 0x66",
    "  push ebx",
    "  .byte 0x66",
    "  retf",

    ".globl trampoline_32",
    "trampoline_32:",
    ".code32",
    "  mov ax, 0x10",
    "  mov ds, ax",
    "  mov es, ax",
    "  mov ss, ax",
    "  mov fs, ax",
    "  mov gs, ax",

    "  mov dx, 0xE9",
    "  mov al, 0x33",
    "  out dx, al",

    "  .byte 0x81, 0xEB",
    "  .4byte trampoline_32 - trampoline_start",

    // Mesma página SIPI (4KiB). ebx+0x1000 era a página SEGUINTE, sem identity.
    "  lea esp, [ebx + 0xFF0]",

    "  mov eax, cr4",
    "  or eax, 0x20",
    "  mov cr4, eax",

    "  mov eax, [ebx + off_cr3]",
    "  mov cr3, eax",

    "  mov ecx, 0xC0000080",
    "  rdmsr",
    "  or eax, 0x100",
    "  wrmsr",

    "  mov eax, cr0",
    "  or eax, 0x80000000",
    "  mov cr0, eax",

    "  mov eax, [ebx + off_jmp64]",
    "  push 0x18",
    "  push eax",
    "  retf",

    ".globl trampoline_64",
    "trampoline_64:",
    ".code64",
    "  mov ecx, 0xC0000080",
    "  rdmsr",
    "  or eax, 0x800",
    "  wrmsr",

    "  mov dx, 0xE9",
    "  mov al, 0x36",
    "  out dx, al",

    "  mov rax, [rbx + off_stack]",
    "  mov rsp, rax",

    "  mov rax, [rbx + off_percpu]",
    "  test rax, rax",
    "  jz 1f",
    "  mov rcx, 0xC0000101",
    "  mov rdx, rax",
    "  shr rdx, 32",
    "  wrmsr",
    "1:",
    // ready *antes* do call para ap_entry (BSP distingue tramp vs Rust).
    "  mov rax, 1",
    "  mov [rbx + off_ready], rax",
    "  mov rax, [rbx + off_entry]",
    "  test rax, rax",
    "  jz 2f",
    "  call rax",
    "2: hlt",
    "  jmp 2b",

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
    ".code64",
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

fn hhdm_u8(phys: u64) -> *mut u8 {
    (phys + crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed)) as *mut u8
}

/// Lê handshake/patch via HHDM (BSP, CR3 do kernel).
pub unsafe fn read_hhdm_u64(phys: u64, off: usize) -> u64 {
    ptr::read_volatile(hhdm_u8(phys).add(off) as *const u64)
}

pub unsafe fn write_hhdm_u64(phys: u64, off: usize, val: u64) {
    ptr::write_volatile(hhdm_u8(phys).add(off) as *mut u64, val);
}

pub unsafe fn clear_handshake(phys: u64) {
    write_hhdm_u64(phys, OFF_SIPI_HIT, 0);
    write_hhdm_u64(phys, OFF_READY, 0);
}

/// Identity-map da página SIPI **sem NX** (executável). Se já existe, só limpa NX.
pub unsafe fn map_identity_executable(tramp_phys: u64) {
    let phys_offset = crate::memory::PHYS_MEM_OFFSET.load(Ordering::Acquire);
    let (l4_frame, _) = x86_64::registers::control::Cr3::read();
    let virt = VirtAddr::new(phys_offset) + l4_frame.start_address().as_u64();
    let page_table = &mut *(virt.as_mut_ptr() as *mut PageTable);
    let mut mapper = OffsetPageTable::new(page_table, VirtAddr::new(phys_offset));

    let page = Page::<Size4KiB>::containing_address(VirtAddr::new(tramp_phys));
    let frame = PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(tramp_phys));
    // Sem NO_EXECUTE: AP executa após EFER.NXE. W necessário para re-patch sequencial.
    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

    let mut guard = crate::memory::GLOBAL_ALLOCATOR.lock();
    let Some(allocator) = guard.as_mut() else {
        crate::slog_nano!("SMP", "warn", "map tramp: sem frame alloc");
        return;
    };
    match mapper.map_to(page, frame, flags, &mut *allocator) {
        Ok(flush) => flush.flush(),
        Err(_) => match mapper.update_flags(page, flags) {
            Ok(flush) => flush.flush(),
            Err(_) => {
                crate::slog_nano!(
                    "SMP",
                    "warn",
                    "update_flags tramp 0x{:x} falhou — tenta SIPI mesmo assim",
                    tramp_phys
                );
            }
        },
    }
}

// TODO(ADR-0057): após todos os APs, update_flags(PRESENT) sem WRITABLE → RX-only.
// Re-patch sequencial precisa W durante o wake; não deixar W+X como política permanente.

pub unsafe fn init_trampoline(
    phys_addr: u64,
    cr3_value: u64,
    ap_stack: u64,
    percpu_addr: u64,
    entry_fn: extern "C" fn(u64) -> !,
) {
    let tramp_virt = hhdm_u8(phys_addr);
    let size = offset_of(&trampoline_start as *const u8, &trampoline_end as *const u8);
    ptr::copy_nonoverlapping(&trampoline_start as *const u8, tramp_virt, size);

    let link = &trampoline_start as *const u8 as u64;
    let first_link = ptr::read_volatile(&trampoline_start as *const u8);
    let first_phys = ptr::read_volatile(tramp_virt);
    let vec = (phys_addr >> 12) as u8;
    crate::slog_nano!(
        "SMP",
        "trace",
        "TRAMP_CHAIN link={:#x} load_hhdm={:#x} phys={:#x} size={} vec={:#04x} vec_ok={} first_link={:#04x} first_phys={:#04x} jmp_rel16={}",
        link,
        tramp_virt as u64,
        phys_addr,
        size,
        vec,
        (phys_addr < 0x100000 && phys_addr % 0x1000 == 0 && vec != 0 && (vec as u64) << 12 == phys_addr) as u8,
        first_link,
        first_phys,
        (first_phys == 0xEB) as u8
    );
    let id_flags = crate::memory::page_leaf_flags(phys_addr);
    let hhdm_flags = crate::memory::page_leaf_flags(tramp_virt as u64);
    crate::slog_nano!(
        "SMP",
        "trace",
        "TRAMP_PTE ident={:#x} hhdm={:#x} nx_ident={} nx_hhdm={}",
        id_flags.unwrap_or(0),
        hhdm_flags.unwrap_or(0),
        id_flags.map(|f| (f >> 63) & 1).unwrap_or(2),
        hhdm_flags.map(|f| (f >> 63) & 1).unwrap_or(2)
    );

    let patch64 = |sym: *const u8, val: u64| {
        let off = offset_of(&trampoline_start as *const u8, sym);
        ptr::write_volatile(tramp_virt.add(off) as *mut u64, val);
    };

    let jmp32_val =
        phys_addr + offset_of(&trampoline_start as *const u8, &trampoline_32 as *const u8) as u64;
    let jmp64_val =
        phys_addr + offset_of(&trampoline_start as *const u8, &trampoline_64 as *const u8) as u64;

    patch64(&trampoline_patch_jmp32 as *const u8, jmp32_val);
    patch64(&trampoline_patch_jmp64 as *const u8, jmp64_val);
    patch64(&trampoline_patch_cr3 as *const u8, cr3_value);
    patch64(&trampoline_patch_stack as *const u8, ap_stack);
    patch64(&trampoline_patch_percpu as *const u8, percpu_addr);
    patch64(&trampoline_patch_entry as *const u8, entry_fn as u64);

    let gdt_phys =
        phys_addr + offset_of(&trampoline_start as *const u8, &trampoline_gdt as *const u8) as u64;
    let gp_off = offset_of(
        &trampoline_start as *const u8,
        &trampoline_gdt_pseudo as *const u8,
    );
    ptr::write_volatile(tramp_virt.add(gp_off + 2) as *mut u32, gdt_phys as u32);

    clear_handshake(phys_addr);
}

pub unsafe fn trampoline_size() -> usize {
    offset_of(&trampoline_start as *const u8, &trampoline_end as *const u8)
}

#[inline]
pub fn sipi_ready() -> bool {
    crate::platform_probe::allow_smp()
}
