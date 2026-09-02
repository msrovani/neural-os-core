//! k_nano R0 — paging/TSS/IST (ADR-0041 §11).
//! Consolidates AddressSpace (CR3/CoW), W^X exec arena, Ring3 iretq/TSS.RSP0.
//! Canonical Cap lives here (R0) and is re-exported by k_hal::cap_gate (R1).
//! Bare-metal only (`target_os="none"`); host builds get stubs to avoid
//! STATUS_ILLEGAL_INSTRUCTION / MSR #GP in soft-float / cargo test.
//! Gate `#[cfg(all(x86_64, not(target_os="none")))]` for host SSE stubs (lesson SSE).

#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(static_mut_refs)]
#![allow(unused_unsafe)]

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use x86_64::registers::control::{Cr3, Cr3Flags};
use x86_64::structures::paging::{PageTable, PageTableFlags, PhysFrame, Size4KiB};
use x86_64::{PhysAddr, VirtAddr};

// ─── Cap — canonical (k_nano R0, re-exported by k_hal R1) ────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cap(pub u64);

impl Cap {
    pub const EMPTY: Cap = Cap(0);
    pub const PING: Cap = Cap(1 << 0);
    pub const RING_OP: Cap = Cap(1 << 1);
    pub const MAP_FB: Cap = Cap(1 << 2);
    pub const WRITE_FB: Cap = Cap(1 << 3);
    pub const PIN_DMA: Cap = Cap(1 << 4);
    pub const MAP_DMA: Cap = Cap(1 << 5);
    pub const MAP_WEIGHTS: Cap = Cap(1 << 6);
    pub const ENTER_USER: Cap = Cap(1 << 7);
    pub const DEMAND_PAGE: Cap = Cap(1 << 8);
    pub const MAP_FILE: Cap = Cap(1 << 9);

    #[inline]
    pub fn bits(self) -> u64 { self.0 }
    #[inline]
    pub fn from_bits(bits: u64) -> Cap { Cap(bits) }
    #[inline]
    pub fn contains(self, other: Cap) -> bool { (self.0 & other.0) == other.0 }
    #[inline]
    pub fn union(self, other: Cap) -> Cap { Cap(self.0 | other.0) }
}

// Syscall numbers (ADR-0076 §4.3) — kept here for R0 dispatch helpers and R1 gate.
pub const SYSCALL_VECTOR: u8 = 0x90;
pub const SYS_PING: u64 = 1;
pub const SYS_RING_OP: u64 = 2;
pub const SYS_MAP_FB: u64 = 3;
pub const SYS_PRESENT_FB: u64 = 4;
pub const SYS_PIN_DMA: u64 = 5;
pub const SYS_MAP_DMA: u64 = 6;
pub const SYS_MAP_WEIGHTS: u64 = 7;
pub const SYS_EXIT_USER: u64 = 8;
pub const SYS_DEMAND_PAGE: u64 = 9;
pub const SYS_MAP_FILE: u64 = 10;
pub const RING_OP_WRITE: u64 = 0;
pub const RING_OP_READ: u64 = 1;

// ─── AddressSpace ─────────────────────────────────────────────────────────

pub const MVP_REGION_BASE: u64 = 0x0000_7000_0000_0000;
pub const SHARED_RING_VA: u64 = MVP_REGION_BASE;
pub const PRIVATE_PAGE_VA: u64 = MVP_REGION_BASE + 0x1000;

pub struct AddressSpace {
    pub l4_frame: PhysFrame<Size4KiB>,
}

fn phys_offset() -> u64 {
    crate::memory::PHYS_MEM_OFFSET.load(Ordering::Acquire)
}

unsafe fn frame_as_table(frame: PhysFrame<Size4KiB>) -> *mut PageTable {
    let virt = VirtAddr::new(phys_offset() + frame.start_address().as_u64());
    virt.as_mut_ptr()
}

unsafe fn zero_frame(frame: PhysFrame<Size4KiB>) {
    let virt = phys_offset() + frame.start_address().as_u64();
    core::ptr::write_bytes(virt as *mut u8, 0, 4096);
}

fn alloc_zeroed_frame() -> Option<PhysFrame<Size4KiB>> {
    let frame = crate::memory::alloc_physical_frame()?;
    unsafe { zero_frame(frame) };
    Some(frame)
}

unsafe fn clone_table(src: PhysFrame<Size4KiB>) -> Result<PhysFrame<Size4KiB>, &'static str> {
    let dst = alloc_zeroed_frame().ok_or("mvp-c: sem frame CoW")?;
    let s = &*frame_as_table(src);
    let d = &mut *frame_as_table(dst);
    for i in 0..512 {
        d[i] = s[i].clone();
    }
    Ok(dst)
}

impl AddressSpace {
    pub fn clone_current() -> Result<Self, &'static str> {
        let (src_l4, _) = Cr3::read();
        let dst_l4 = unsafe { clone_table(src_l4)? };
        Ok(Self { l4_frame: dst_l4 })
    }

    unsafe fn ensure_owned_child(
        entry: &mut x86_64::structures::paging::page_table::PageTableEntry,
        user: bool,
    ) -> Result<PhysFrame<Size4KiB>, &'static str> {
        if entry.flags().contains(PageTableFlags::HUGE_PAGE) {
            return Err("mvp-c: huge page no caminho");
        }
        let mut parent = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        if user { parent.insert(PageTableFlags::USER_ACCESSIBLE); }
        if !entry.flags().contains(PageTableFlags::PRESENT) {
            let f = alloc_zeroed_frame().ok_or("mvp-c: sem frame PT")?;
            entry.set_addr(f.start_address(), parent);
            return Ok(f);
        }
        let old = PhysFrame::<Size4KiB>::containing_address(entry.addr());
        let owned = clone_table(old)?;
        let mut f = entry.flags();
        f.insert(parent);
        entry.set_addr(owned.start_address(), f);
        Ok(owned)
    }

    unsafe fn map_page_inner(
        &mut self,
        virt: VirtAddr,
        frame: PhysFrame<Size4KiB>,
        flags: PageTableFlags,
        user: bool,
    ) -> Result<(), &'static str> {
        let l4 = &mut *frame_as_table(self.l4_frame);
        let e3 = &mut l4[virt.p4_index()];
        let p3_frame = Self::ensure_owned_child(e3, user)?;
        let l3 = &mut *frame_as_table(p3_frame);
        let e2 = &mut l3[virt.p3_index()];
        let p2_frame = Self::ensure_owned_child(e2, user)?;
        let l2 = &mut *frame_as_table(p2_frame);
        let e1 = &mut l2[virt.p2_index()];
        let p1_frame = Self::ensure_owned_child(e1, user)?;
        let l1 = &mut *frame_as_table(p1_frame);
        let leaf = &mut l1[virt.p1_index()];
        if leaf.flags().contains(PageTableFlags::PRESENT) {
            return Err("mvp-c: VA ja mapeada");
        }
        leaf.set_addr(frame.start_address(), flags);
        x86_64::instructions::tlb::flush(virt);
        Ok(())
    }

    pub unsafe fn map_page(
        &mut self,
        virt: VirtAddr,
        frame: PhysFrame<Size4KiB>,
        flags: PageTableFlags,
    ) -> Result<(), &'static str> {
        self.map_page_inner(virt, frame, flags, false)
    }

    pub unsafe fn map_user_page(
        &mut self,
        virt: VirtAddr,
        frame: PhysFrame<Size4KiB>,
        flags: PageTableFlags,
    ) -> Result<(), &'static str> {
        let mut f = flags;
        f.insert(PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE);
        self.map_page_inner(virt, frame, f, true)
    }

    pub unsafe fn reserve_page(&mut self, virt: VirtAddr, user: bool) -> Result<(), &'static str> {
        let l4 = &mut *frame_as_table(self.l4_frame);
        let e3 = &mut l4[virt.p4_index()];
        let p3_frame = Self::ensure_owned_child(e3, user)?;
        let l3 = &mut *frame_as_table(p3_frame);
        let e2 = &mut l3[virt.p3_index()];
        let p2_frame = Self::ensure_owned_child(e2, user)?;
        let l2 = &mut *frame_as_table(p2_frame);
        let e1 = &mut l2[virt.p2_index()];
        let p1_frame = Self::ensure_owned_child(e1, user)?;
        let l1 = &mut *frame_as_table(p1_frame);
        let leaf = &mut l1[virt.p1_index()];
        if leaf.flags().contains(PageTableFlags::PRESENT) {
            return Err("mvp-c: VA ja mapeada");
        }
        leaf.set_unused();
        Ok(())
    }

    pub unsafe fn activate(&self) {
        Cr3::write(self.l4_frame, Cr3Flags::empty());
    }

    pub fn frame_for_virt(&self, virt: VirtAddr) -> Option<PhysFrame<Size4KiB>> {
        let l4 = unsafe { &*frame_as_table(self.l4_frame) };
        let e3 = &l4[virt.p4_index()];
        if !e3.flags().contains(PageTableFlags::PRESENT) { return None; }
        let l3 = unsafe { &*frame_as_table(PhysFrame::containing_address(e3.addr())) };
        let e2 = &l3[virt.p3_index()];
        if !e2.flags().contains(PageTableFlags::PRESENT) { return None; }
        let l2 = unsafe { &*frame_as_table(PhysFrame::containing_address(e2.addr())) };
        let e1 = &l2[virt.p2_index()];
        if !e1.flags().contains(PageTableFlags::PRESENT) { return None; }
        let l1 = unsafe { &*frame_as_table(PhysFrame::containing_address(e1.addr())) };
        let leaf = &l1[virt.p1_index()];
        if !leaf.flags().contains(PageTableFlags::PRESENT) { return None; }
        Some(PhysFrame::<Size4KiB>::containing_address(leaf.addr()))
    }

    pub fn set_user_leaf_flags(&self, virt: VirtAddr, flags: PageTableFlags) -> Result<(), &'static str> {
        let l4 = unsafe { &mut *frame_as_table(self.l4_frame) };
        let e3 = &mut l4[virt.p4_index()];
        if !e3.flags().contains(PageTableFlags::PRESENT) { return Err("wxe: P3 ausente"); }
        let l3 = unsafe { &mut *frame_as_table(PhysFrame::containing_address(e3.addr())) };
        let e2 = &l3[virt.p3_index()];
        if !e2.flags().contains(PageTableFlags::PRESENT) { return Err("wxe: P2 ausente"); }
        let l2 = unsafe { &mut *frame_as_table(PhysFrame::containing_address(e2.addr())) };
        let e1 = &mut l2[virt.p2_index()];
        if !e1.flags().contains(PageTableFlags::PRESENT) { return Err("wxe: P1 ausente"); }
        let l1 = unsafe { &mut *frame_as_table(PhysFrame::containing_address(e1.addr())) };
        let leaf = &mut l1[virt.p1_index()];
        if !leaf.flags().contains(PageTableFlags::PRESENT) { return Err("wxe: folha ausente"); }
        let mut f = flags;
        if leaf.flags().contains(PageTableFlags::USER_ACCESSIBLE) {
            f.insert(PageTableFlags::USER_ACCESSIBLE);
        }
        let frame = PhysFrame::<Size4KiB>::containing_address(leaf.addr());
        leaf.set_addr(frame.start_address(), f);
        x86_64::instructions::tlb::flush(virt);
        Ok(())
    }
}

pub unsafe fn install_present_leaf_current(
    virt: VirtAddr,
    frame: PhysFrame<Size4KiB>,
    flags: PageTableFlags,
) -> Result<(), &'static str> {
    let (l4_frame, _) = Cr3::read();
    let l4 = &*frame_as_table(l4_frame);
    let e3 = &l4[virt.p4_index()];
    if !e3.flags().contains(PageTableFlags::PRESENT) { return Err("p7: P3 ausente"); }
    let l3 = &*frame_as_table(PhysFrame::containing_address(e3.addr()));
    let e2 = &l3[virt.p3_index()];
    if !e2.flags().contains(PageTableFlags::PRESENT) { return Err("p7: P2 ausente"); }
    let l2 = &*frame_as_table(PhysFrame::containing_address(e2.addr()));
    let e1 = &l2[virt.p2_index()];
    if !e1.flags().contains(PageTableFlags::PRESENT) { return Err("p7: P1 ausente"); }
    let l1 = &mut *frame_as_table(PhysFrame::containing_address(e1.addr()));
    let leaf = &mut l1[virt.p1_index()];
    if leaf.flags().contains(PageTableFlags::PRESENT) { return Ok(()); }
    leaf.set_addr(frame.start_address(), flags);
    x86_64::instructions::tlb::flush(virt);
    Ok(())
}

pub fn hhdm_mut<T>(frame: PhysFrame<Size4KiB>) -> *mut T {
    (phys_offset() + frame.start_address().as_u64()) as *mut T
}

pub fn create_sandbox_as() -> Result<AddressSpace, &'static str> {
    fn supervisor_only(mut entry: x86_64::structures::paging::page_table::PageTableEntry) -> x86_64::structures::paging::page_table::PageTableEntry {
        if entry.flags().contains(PageTableFlags::PRESENT) {
            let addr = entry.addr();
            let mut flags = entry.flags();
            flags.remove(PageTableFlags::USER_ACCESSIBLE);
            entry.set_addr(addr, flags);
        }
        entry
    }
    let (kernel_l4, _) = Cr3::read();
    let kernel_l4_ptr = unsafe { &*frame_as_table(kernel_l4) };
    let l4_frame = alloc_zeroed_frame().ok_or("sandbox: sem frame L4")?;
    let l4_ptr = unsafe { &mut *frame_as_table(l4_frame) };
    const KERNEL_P4: usize = 511;
    let kernel_entry = &kernel_l4_ptr[KERNEL_P4];
    if !kernel_entry.flags().contains(PageTableFlags::PRESENT) {
        return Err("sandbox: P4[511] kernel ausente");
    }
    l4_ptr[KERNEL_P4] = supervisor_only(kernel_entry.clone());
    let pm = crate::memory::PHYS_MEM_OFFSET.load(Ordering::Acquire);
    if pm != 0 {
        let hhdm_p4 = ((pm >> 39) & 0x1ff) as usize;
        if hhdm_p4 != KERNEL_P4 {
            let e = &kernel_l4_ptr[hhdm_p4];
            if e.flags().contains(PageTableFlags::PRESENT) {
                l4_ptr[hhdm_p4] = supervisor_only(e.clone());
            }
        }
    }
    Ok(AddressSpace { l4_frame })
}

/// Mapeia página USER da mailbox syscall (N4).
pub fn map_user_mailbox(aspace: &mut AddressSpace) -> Result<PhysFrame<Size4KiB>, &'static str> {
    let frame = alloc_frame()?;
    unsafe {
        aspace.map_user_page(VirtAddr::new(crate::ring3::USER_MAILBOX_VA), frame, user_data_flags())?;
        core::ptr::write_bytes(hhdm_mut::<u8>(frame), 0, 4096);
    }
    Ok(frame)
}

pub fn kernel_cr3() -> (PhysFrame<Size4KiB>, Cr3Flags) { Cr3::read() }
pub unsafe fn restore_cr3(frame: PhysFrame<Size4KiB>, flags: Cr3Flags) { Cr3::write(frame, flags); }
pub fn alloc_frame() -> Result<PhysFrame<Size4KiB>, &'static str> { alloc_zeroed_frame().ok_or("mvp-c: sem frame fisico") }
pub fn rw_flags() -> PageTableFlags { PageTableFlags::PRESENT | PageTableFlags::WRITABLE }
pub fn user_code_flags() -> PageTableFlags { PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE }
pub fn user_data_flags() -> PageTableFlags { PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE }

// ─── W^X exec arena ───────────────────────────────────────────────────────

const ARENA_VA: u64 = 0x0000_5000_0000_0000;

unsafe fn table_mut(frame: PhysFrame<Size4KiB>) -> *mut PageTable {
    VirtAddr::new(phys_offset() + frame.start_address().as_u64()).as_mut_ptr()
}
unsafe fn alloc_zeroed_arena() -> Option<PhysFrame<Size4KiB>> {
    let f = crate::memory::alloc_physical_frame()?;
    core::ptr::write_bytes((phys_offset() + f.start_address().as_u64()) as *mut u8, 0, 4096);
    Some(f)
}
unsafe fn map_leaf(
    virt: VirtAddr,
    frame: PhysFrame<Size4KiB>,
    flags: PageTableFlags,
) -> Result<(), &'static str> {
    let (l4f, _) = Cr3::read();
    let parent = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
    let idxs = [virt.p4_index(), virt.p3_index(), virt.p2_index()];
    let mut cur = l4f;
    for &idx in idxs.iter() {
        let t = &mut *table_mut(cur);
        let e = &mut t[idx];
        if e.flags().contains(PageTableFlags::HUGE_PAGE) { return Err("exec_arena: huge page no caminho"); }
        if !e.flags().contains(PageTableFlags::PRESENT) {
            let nf = alloc_zeroed_arena().ok_or("exec_arena: sem frame PT")?;
            e.set_addr(nf.start_address(), parent);
            cur = nf;
        } else { cur = PhysFrame::containing_address(e.addr()); }
    }
    let l1 = &mut *table_mut(cur);
    let leaf = &mut l1[virt.p1_index()];
    if leaf.flags().contains(PageTableFlags::PRESENT) { return Err("exec_arena: VA ja mapeada"); }
    leaf.set_addr(frame.start_address(), flags);
    x86_64::instructions::tlb::flush(virt);
    Ok(())
}
unsafe fn set_leaf_flags(virt: VirtAddr, flags: PageTableFlags) -> Result<(), &'static str> {
    let (l4f, _) = Cr3::read();
    let idxs = [virt.p4_index(), virt.p3_index(), virt.p2_index()];
    let mut cur = l4f;
    for &idx in idxs.iter() {
        let t = &*table_mut(cur);
        let e = &t[idx];
        if !e.flags().contains(PageTableFlags::PRESENT) || e.flags().contains(PageTableFlags::HUGE_PAGE) {
            return Err("exec_arena: caminho inválido no flip");
        }
        cur = PhysFrame::containing_address(e.addr());
    }
    let l1 = &mut *table_mut(cur);
    let leaf = &mut l1[virt.p1_index()];
    let frame = PhysFrame::<Size4KiB>::containing_address(leaf.addr());
    leaf.set_addr(frame.start_address(), flags);
    x86_64::instructions::tlb::flush(virt);
    Ok(())
}

/// Aloca 1 página, escreve `code` (RW), vira RX e retorna VA executável (Ring0).
pub unsafe fn jit_write_exec(code: &[u8]) -> Result<u64, &'static str> {
    if code.is_empty() || code.len() > 4096 { return Err("exec_arena: código vazio/grande demais"); }
    let virt = VirtAddr::new(ARENA_VA);
    let (l4f, _) = Cr3::read();
    {
        let l4 = &*table_mut(l4f);
        if l4[virt.p4_index()].flags().contains(PageTableFlags::PRESENT) {
            return Err("exec_arena: índice L4 ocupado");
        }
    }
    let frame = crate::memory::alloc_physical_frame().ok_or("exec_arena: sem frame código")?;
    map_leaf(virt, frame, PageTableFlags::PRESENT | PageTableFlags::WRITABLE)?;
    let dst = ARENA_VA as *mut u8;
    core::ptr::copy_nonoverlapping(code.as_ptr(), dst, code.len());
    core::sync::atomic::fence(Ordering::SeqCst);
    set_leaf_flags(virt, PageTableFlags::PRESENT)?;
    core::arch::asm!(
        "push rbx", "cpuid", "pop rbx",
        out("eax") _, out("ecx") _, out("edx") _,
        options(nomem, nostack, preserves_flags)
    );
    Ok(ARENA_VA)
}
pub fn arena_self_test() -> bool {
    let code: [u8; 6] = [0xB8, 0x2A, 0x00, 0x00, 0x00, 0xC3];
    let va = match unsafe { jit_write_exec(&code) } { Ok(v) => v, Err(e) => { crate::slog_nano!("EXEC-ARENA", "warn", "self-test FAIL setup: {}", e); return false; } };
    let f: unsafe extern "C" fn() -> u32 = unsafe { core::mem::transmute::<*const (), unsafe extern "C" fn() -> u32>(va as *const ()) };
    let r = unsafe { f() };
    if r == 42 { crate::slog_nano!("EXEC-ARENA", "info", "W^X JIT self-test PASS (native mov eax,42;ret -> {}) — ADR-0059 F7", r); true } else { crate::slog_nano!("EXEC-ARENA", "warn", "self-test resultado inesperado: {}", r); false }
}
/// USER arena (sandbox AS) — W^X com USER_ACCESSIBLE.
pub unsafe fn jit_write_exec_user(
    aspace: &mut AddressSpace,
    code: &[u8],
) -> Result<u64, &'static str> {
    if code.is_empty() || code.len() > 4096 { return Err("exec_arena: código vazio/grande demais"); }
    let virt = VirtAddr::new(ARENA_VA);
    let frame = crate::memory::alloc_physical_frame().ok_or("exec_arena: sem frame código")?;
    let write_flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;
    aspace.map_user_page(virt, frame, write_flags)?;
    let dst = hhdm_mut::<u8>(frame) as *mut u8;
    core::ptr::copy_nonoverlapping(code.as_ptr(), dst, code.len());
    core::sync::atomic::fence(Ordering::SeqCst);
    aspace.set_user_leaf_flags(virt, PageTableFlags::PRESENT)?;
    Ok(ARENA_VA)
}
pub fn user_arena_self_test() -> bool {
    let mut aspace = match create_sandbox_as() { Ok(a) => a, Err(e) => { crate::slog_nano!("EXEC-ARENA", "warn", "user selftest: sandbox fail {}", e); return false; } };
    let code: [u8; 6] = [0xB8, 0x2A, 0x00, 0x00, 0x00, 0xC3];
    let va = match unsafe { jit_write_exec_user(&mut aspace, &code) } { Ok(v) => v, Err(e) => { crate::slog_nano!("EXEC-ARENA", "warn", "user selftest: jit fail {}", e); return false; } };
    let frame = match aspace.frame_for_virt(VirtAddr::new(va)) { Some(f) => f, None => { crate::slog_nano!("EXEC-ARENA", "warn", "user selftest: folha nao mapeada"); return false; } };
    let ptr = hhdm_mut::<u8>(frame) as *const u8;
    for (i, &b) in code.iter().enumerate() { if unsafe { core::ptr::read_volatile(ptr.add(i)) } != b { crate::slog_nano!("EXEC-ARENA", "warn", "user selftest: bytes corrompidos"); return false; } }
    crate::slog_nano!("EXEC-ARENA", "info", "W^X USER arena self-test PASS ({} bytes USER RX @{:#x}) — ADR-0082 F3.1", code.len(), va);
    true
}

// ─── Ring3 user-mode (iretq + TSS.RSP0) ───────────────────────────────────

pub const USER_CODE_VA: u64 = 0x0000_7000_0030_0000;
pub const USER_STACK_VA: u64 = 0x0000_7000_0030_1000;
/// Demo marker fica **após** a mailbox N4 (48 bytes) — evita clobber de `syscall_finish_ok`.
const USER_DEMO_MARKER_OFF: u64 = 48;
/// Alias histórico — mailbox canônica em `ring3::USER_MAILBOX_VA` (N4).
pub const USER_MARKER_VA: u64 = crate::ring3::USER_MAILBOX_VA;
pub const RING3_MAGIC: u64 = 0x0033_5249_4E47_0001;

static DEMO_ACTIVE: AtomicBool = AtomicBool::new(false);
static ABORTING: AtomicBool = AtomicBool::new(false);
static EXIT_OK: AtomicU64 = AtomicU64::new(0);
static SAVED_CR3: AtomicU64 = AtomicU64::new(0);
static SAVED_CR3_FLAGS: AtomicU64 = AtomicU64::new(0);
static SAVED_CR0: AtomicU64 = AtomicU64::new(0);
static STAGE_EXIT: AtomicBool = AtomicBool::new(true);
static USE_MAILBOX: AtomicBool = AtomicBool::new(false);
static SANDBOX_DMA_DENY: AtomicU64 = AtomicU64::new(0);
static SANDBOX_MMIO_DENY: AtomicU64 = AtomicU64::new(0);

static SANDBOX_BUSY: AtomicBool = AtomicBool::new(false);
static SAVED_GS_BASE: AtomicU64 = AtomicU64::new(0);
static RING3_CAN_IRETQ: AtomicBool = AtomicBool::new(false);

#[cfg(feature = "ring3")]
static mut SAVED_RIP: u64 = 0;
#[cfg(feature = "ring3")]
static mut SAVED_RSP: u64 = 0;
#[cfg(feature = "ring3")]
static mut SAVED_CALLEE: [u64; 6] = [0; 6];

pub const TRY_ENTER_RING3: bool = true;

#[inline]
pub fn demo_active() -> bool { DEMO_ACTIVE.load(Ordering::SeqCst) || ABORTING.load(Ordering::SeqCst) }
#[inline]
pub fn sandbox_syscalls() -> bool { demo_active() }
#[inline]
pub fn mailbox_syscalls() -> bool { USE_MAILBOX.load(Ordering::SeqCst) }
pub fn set_use_mailbox(on: bool) { USE_MAILBOX.store(on, Ordering::SeqCst); }
pub fn set_stage_exit(on: bool) { STAGE_EXIT.store(on, Ordering::SeqCst); }
pub fn reset_sandbox_denies() {
    SANDBOX_DMA_DENY.store(0, Ordering::SeqCst);
    SANDBOX_MMIO_DENY.store(0, Ordering::SeqCst);
}
pub fn saved_cr0_store(v: u64) { SAVED_CR0.store(v, Ordering::SeqCst); }
pub fn saved_cr0_take() -> u64 { SAVED_CR0.swap(0, Ordering::SeqCst) }
pub fn syscall_staged() -> (u64, u64, u64) {
    (
        SYS_NR.load(Ordering::SeqCst),
        SYS_ARG.load(Ordering::SeqCst),
        SYS_CAP.load(Ordering::SeqCst),
    )
}
pub fn syscall_stage(nr: u64, arg: u64, cap: Cap) { stage_syscall(nr, arg, cap); }
pub fn syscall_finish_ok(v: u64) {
    SYS_RESULT.store(v, Ordering::SeqCst);
    SYS_STATUS.store(0, Ordering::SeqCst);
    if mailbox_syscalls() {
        unsafe { crate::ring3::write_user_mailbox_result(v, 0); }
    }
}
pub fn syscall_finish_err() {
    SYS_STATUS.store(1, Ordering::SeqCst);
    if mailbox_syscalls() {
        unsafe { crate::ring3::write_user_mailbox_result(0, 1); }
    }
}
pub fn syscall_stage_from_mailbox(_mbox: u64) {
    let m = unsafe { crate::ring3::read_user_mailbox() };
    // Mailbox zerada: preserva stage de enter_user_mode (demo P6 pré-iretq).
    if m.nr == 0 && m.cap == 0 {
        return;
    }
    SYS_NR.store(m.nr, Ordering::SeqCst);
    SYS_ARG.store(m.arg0, Ordering::SeqCst);
    let cap_bits = if m.cap != 0 {
        m.cap
    } else {
        match m.nr {
            SYS_EXIT_USER => Cap::ENTER_USER.bits(),
            SYS_PIN_DMA => Cap::PIN_DMA.bits(),
            SYS_MAP_FB => Cap::MAP_FB.bits(),
            SYS_MAP_DMA => Cap::MAP_DMA.bits(),
            SYS_PRESENT_FB => Cap::WRITE_FB.bits(),
            _ => 0,
        }
    };
    SYS_CAP.store(cap_bits, Ordering::SeqCst);
}
pub fn syscall_try_regs_fallback() {
    let (nr, _, cap_bits) = syscall_staged();
    if nr != 0 || cap_bits != 0 {
        return;
    }
    let reg_nr: u64;
    let reg_arg: u64;
    let reg_cap: u64;
    unsafe {
        core::arch::asm!(
            "mov {nr}, rax",
            "mov {arg}, rdi",
            "mov {cap}, rdx",
            nr = out(reg) reg_nr,
            arg = out(reg) reg_arg,
            cap = out(reg) reg_cap,
            options(nostack, preserves_flags)
        );
    }
    if reg_nr != 0 {
        SYS_NR.store(reg_nr, Ordering::SeqCst);
        SYS_ARG.store(reg_arg, Ordering::SeqCst);
        SYS_CAP.store(reg_cap, Ordering::SeqCst);
    }
}
pub fn ring3_can_iretq() -> bool { RING3_CAN_IRETQ.load(Ordering::Relaxed) }
pub fn ring3_note_iretq_ok() { RING3_CAN_IRETQ.store(true, Ordering::Relaxed); }

fn free_frame(frame: PhysFrame<Size4KiB>) {
    unsafe { crate::memory::dealloc_physical_frame(frame); }
}

fn release_sandbox_slot() { SANDBOX_BUSY.store(false, Ordering::SeqCst); }

fn zero_gs_base() {
    unsafe {
        core::arch::asm!(
            "xor eax, eax",
            "xor edx, edx",
            "wrmsr",
            in("ecx") 0xC0000101u32,
            options(nostack, preserves_flags)
        );
    }
}

fn restore_gs_base() {
    let base = SAVED_GS_BASE.swap(0, Ordering::SeqCst);
    if base == 0 {
        return;
    }
    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") 0xC0000101u32,
            in("eax") (base & 0xFFFFFFFF) as u32,
            in("edx") (base >> 32) as u32,
            options(nostack, preserves_flags)
        );
    }
}

fn read_gs_base() -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        core::arch::asm!(
            "rdmsr",
            in("ecx") 0xC0000101u32,
            out("eax") lo,
            out("edx") hi,
            options(nostack, preserves_flags)
        );
    }
    ((hi as u64) << 32) | lo as u64
}
pub fn note_sandbox_cap_deny(nr: u64) {
    match nr {
        SYS_PIN_DMA | SYS_MAP_DMA => { SANDBOX_DMA_DENY.fetch_add(1, Ordering::SeqCst); }
        SYS_MAP_FB | SYS_PRESENT_FB => { SANDBOX_MMIO_DENY.fetch_add(1, Ordering::SeqCst); }
        _ => {}
    }
}
pub fn sandbox_dma_denies() -> u64 { SANDBOX_DMA_DENY.load(Ordering::SeqCst) }
pub fn sandbox_mmio_denies() -> u64 { SANDBOX_MMIO_DENY.load(Ordering::SeqCst) }

// syscall staging statics (R0 mirror of int 0x90 ABI; R1 dispatch reads these)
static PING_COUNT: AtomicU64 = AtomicU64::new(0);
static SYS_NR: AtomicU64 = AtomicU64::new(0);
static SYS_ARG: AtomicU64 = AtomicU64::new(0);
static SYS_CAP: AtomicU64 = AtomicU64::new(0);
static SYS_RESULT: AtomicU64 = AtomicU64::new(0);
static SYS_STATUS: AtomicU64 = AtomicU64::new(0);

pub fn ping_count() -> u64 { PING_COUNT.load(Ordering::Relaxed) }
pub fn stage_syscall(nr: u64, arg: u64, cap: Cap) {
    SYS_NR.store(nr, Ordering::SeqCst);
    SYS_ARG.store(arg, Ordering::SeqCst);
    SYS_CAP.store(cap.bits(), Ordering::SeqCst);
    SYS_STATUS.store(0, Ordering::SeqCst);
}

fn puts(s: &[u8]) { for &c in s { unsafe { core::arch::asm!("out dx, al", in("dx") 0x3F8u16, in("al") c, options(nostack, preserves_flags)); } } }

pub fn fault_abort(msg: &'static str) -> ! {
    if ABORTING.swap(true, Ordering::SeqCst) { loop { x86_64::instructions::hlt(); } }
    DEMO_ACTIVE.store(false, Ordering::SeqCst);
    EXIT_OK.store(3, Ordering::SeqCst);
    crate::ring3::publish_sandbox_fault(msg);
    puts(b"[P6] WARN fault abort - restore CR3 + skip iretq\n");
    unsafe { jump_back_to_kernel() }
}
pub fn return_from_user(ok: bool) -> ! {
    EXIT_OK.store(if ok { 1 } else { 2 }, Ordering::SeqCst);
    unsafe { jump_back_to_kernel() }
}
unsafe fn jump_back_to_kernel() -> ! {
    DEMO_ACTIVE.store(false, Ordering::SeqCst);
    USE_MAILBOX.store(false, Ordering::SeqCst);
    release_sandbox_slot();
    restore_gs_base();
    let cr3_addr = SAVED_CR3.load(Ordering::SeqCst);
    let cr3_flags = SAVED_CR3_FLAGS.load(Ordering::SeqCst);
    if cr3_addr != 0 {
        let frame = PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(cr3_addr));
        let flags = Cr3Flags::from_bits_truncate(cr3_flags);
        restore_cr3(frame, flags);
    }
    let cr0 = SAVED_CR0.swap(0, Ordering::SeqCst);
    if cr0 != 0 { unsafe { core::arch::asm!("mov cr0, {}", in(reg) cr0, options(nostack)); } }
    #[cfg(feature = "ring3")]
    let rip = SAVED_RIP;
    #[cfg(not(feature = "ring3"))]
    let rip = 0u64;
    #[cfg(feature = "ring3")]
    let rsp = SAVED_RSP;
    #[cfg(not(feature = "ring3"))]
    let rsp = 0u64;
    if rip == 0 || rsp == 0 {
        ABORTING.store(false, Ordering::SeqCst);
        puts(b"[P6] WARN no SAVED_RIP - spin (no return)\n");
        loop { x86_64::instructions::hlt(); }
    }
    ABORTING.store(false, Ordering::SeqCst);
    #[cfg(feature = "ring3")]
    {
        let sc = core::ptr::addr_of_mut!(SAVED_CALLEE) as u64;
        core::arch::asm!(
            "mov rbx, qword ptr [{sc} + 0*8]",
            "mov rbp, qword ptr [{sc} + 1*8]",
            "mov r12, qword ptr [{sc} + 2*8]",
            "mov r13, qword ptr [{sc} + 3*8]",
            "mov r14, qword ptr [{sc} + 4*8]",
            "mov r15, qword ptr [{sc} + 5*8]",
            sc = in(reg) sc,
            options(nostack)
        );
    }
    core::arch::asm!("mov rsp, {rsp}", "jmp {rip}", rsp = in(reg) rsp, rip = in(reg) rip, options(noreturn));
}

/// Enter CPL=3 via iretq. Requires Cap::ENTER_USER.
pub unsafe fn enter_user_mode(
    entry: u64,
    user_stack: u64,
    user_l4: PhysFrame<Size4KiB>,
    held: Cap,
) -> Result<(), &'static str> {
    if !held.contains(Cap::ENTER_USER) {
        crate::slog_nano!("CapGate", "info", "DENY ENTER_USER held=0x{:x}", held.bits());
        return Err("EPERM: Cap::ENTER_USER");
    }
    if !TRY_ENTER_RING3 { return Err("P6: TRY_ENTER_RING3=false"); }
    if SANDBOX_BUSY
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("sandbox: busy (MAX_SANDBOXES=1)");
    }
    let (k_l4, k_flags) = kernel_cr3();
    SAVED_CR3.store(k_l4.start_address().as_u64(), Ordering::SeqCst);
    SAVED_CR3_FLAGS.store(k_flags.bits(), Ordering::SeqCst);
    EXIT_OK.store(0, Ordering::SeqCst);
    if STAGE_EXIT.swap(true, Ordering::SeqCst) { stage_syscall(SYS_EXIT_USER, 0, Cap::ENTER_USER); } else { stage_syscall(0, 0, Cap::EMPTY); }
    let ucs = crate::interrupts::user_code_selector().0 as u64;
    let uds = crate::interrupts::user_data_selector().0 as u64;
    let mut rflags: u64;
    core::arch::asm!("pushfq; pop {}", out(reg) rflags, options(nostack));
    // N6: IF=0 + without_interrupts = DoS por loop infinito documentado (ADR-0102 §4.5).
    rflags &= !0x200;
    crate::slog_nano!("P6", "info", "A: save rsp");
    let rsp_val: u64;
    core::arch::asm!("mov {}, rsp", out(reg) rsp_val, options(nostack));
    #[cfg(feature = "ring3")] { SAVED_RSP = rsp_val; }
    #[cfg(feature = "ring3")]
    {
        let p = core::ptr::addr_of_mut!(SAVED_CALLEE) as u64;
        core::arch::asm!(
            "mov qword ptr [{p} + 0*8], rbx",
            "mov qword ptr [{p} + 1*8], rbp",
            "mov qword ptr [{p} + 2*8], r12",
            "mov qword ptr [{p} + 3*8], r13",
            "mov qword ptr [{p} + 4*8], r14",
            "mov qword ptr [{p} + 5*8], r15",
            p = in(reg) p, options(nostack)
        );
    }
    DEMO_ACTIVE.store(true, Ordering::SeqCst);
    USE_MAILBOX.store(true, Ordering::SeqCst);
    SAVED_GS_BASE.store(read_gs_base(), Ordering::SeqCst);
    zero_gs_base();
    crate::slog_nano!("P6", "info", "B: cr3->user");
    restore_cr3(user_l4, Cr3Flags::empty());
    crate::slog_nano!("P6", "info", "C: cr3 switched (CPL0)");
    #[cfg(feature = "ring3")]
    let rip_ptr = core::ptr::addr_of_mut!(SAVED_RIP);
    #[cfg(not(feature = "ring3"))]
    let rip_ptr = core::ptr::null_mut::<u64>();
    crate::slog_nano!("P6", "info", "D: iretq->CPL3");
    core::arch::asm!(
        "lea {tmp}, [rip + 2f]",
        "mov qword ptr [{rip_ptr}], {tmp}",
        "mov ax, {uds:x}",
        "mov ds, ax",
        "mov es, ax",
        "push {uds}",
        "push {stack}",
        "push {rflags}",
        "push {ucs}",
        "push {entry}",
        "iretq",
        "2:",
        tmp = out(reg) _,
        rip_ptr = in(reg) rip_ptr,
        uds = in(reg) uds,
        ucs = in(reg) ucs,
        stack = in(reg) user_stack,
        rflags = in(reg) rflags,
        entry = in(reg) entry,
    );
    crate::slog_nano!("P6", "info", "E: returned from CPL3");
    DEMO_ACTIVE.store(false, Ordering::SeqCst);
    release_sandbox_slot();
    restore_gs_base();
    core::arch::asm!("mov ss, ax", in("ax") 0u16, options(nostack, preserves_flags));
    match EXIT_OK.load(Ordering::SeqCst) {
        1 => Ok(()),
        2 => Err("EPERM: Cap::ENTER_USER (exit)"),
        3 => Err("P6: fault during Ring3"),
        _ => Err("P6: enter_user sem EXIT"),
    }
}

// ─── helpers for syscall dispatch (called from k_hal) ─────────────────────

pub fn dispatch_check_sandbox(nr: u64, cap: Cap) -> Result<(), &'static str> {
    if sandbox_syscalls() && matches!(nr, SYS_PIN_DMA | SYS_MAP_DMA | SYS_MAP_FB | SYS_PRESENT_FB) {
        note_sandbox_cap_deny(nr);
        crate::slog_nano!("CapGate", "info", "DENY sandbox DMA/MMIO nr={} held=0x{:x}", nr, cap.bits());
        return Err("EPERM: sandbox DMA/MMIO");
    }
    Ok(())
}
pub fn alloc_and_map_fb(fb_phys: u64, fb_va: u64, pages: usize) -> Result<u64, &'static str> {
    if fb_phys == 0 { return Err("ENODEV: FB phys address is 0"); }
    // This helper is used by k_hal dispatch to avoid duplicating CR3 walk.
    // For now the full mapping is done in k_hal (needs jarbas_fb consts); provide stub.
    let _ = (fb_phys, fb_va, pages);
    Ok(fb_va)
}

// ─── syscall int 0x90 handler (R0 — IDT DPL3) ─────────────────────────────

pub fn init_syscall_fast_path() {
    let hv = crate::platform_probe::hypervisor();
    let syscall_ok = crate::platform_probe::probe_done()
        && matches!(hv, crate::platform_probe::HypervisorKind::None | crate::platform_probe::HypervisorKind::Kvm);
    if !syscall_ok {
        crate::slog_nano!("SYSCALL", "info", "SYSCALL/SYSRET gated off (probe={} hv={:?}) — fallback int 0x90", crate::platform_probe::probe_done(), hv);
        return;
    }
    let kernel_cs = 0x08u64;
    let kernel_ss = 0x10u64;
    let user_cs = 0x18u64 | 3;
    let user_ss = 0x20u64 | 3;
    let star = (user_cs << 48) | (kernel_cs << 32) | (user_ss << 16) | kernel_ss;
    let fmask = (1u64 << 9) | (1u64 << 8) | (1u64 << 16) | (1u64 << 14);
    unsafe {
        core::arch::asm!("wrmsr", in("ecx") 0xC0000081u32, in("eax") (star & 0xFFFFFFFF) as u32, in("edx") (star >> 32) as u32, options(nostack, preserves_flags));
        core::arch::asm!("wrmsr", in("ecx") 0xC0000082u32, in("eax") ((syscall_entry as *const () as u64) & 0xFFFFFFFF) as u32, in("edx") ((syscall_entry as *const () as u64) >> 32) as u32, options(nostack, preserves_flags));
        core::arch::asm!("wrmsr", in("ecx") 0xC0000084u32, in("eax") (fmask & 0xFFFFFFFF) as u32, in("edx") (fmask >> 32) as u32, options(nostack, preserves_flags));
    }
    crate::slog_nano!("SYSCALL", "info", "SYSCALL/SYSRET MSRs initialized (LSTAR={:#x}, STAR={:#x}, FMASK={:#x})", syscall_entry as *const () as u64, star, fmask);
}

#[unsafe(naked)]
unsafe extern "C" fn syscall_entry() {
    unsafe {
        core::arch::naked_asm!(
        "swapgs", "mov gs:[8], rsp", "mov rsp, gs:[0]", "push r11", "push rcx", "push rax", "push rdi", "push rsi", "push rdx", "push r10", "push r8", "push r9",
        "call {dispatch_syscall}",
        "pop r9", "pop r8", "pop r10", "pop rdx", "pop rsi", "pop rdi", "pop rax", "pop rcx", "pop r11", "mov gs:[0], rsp", "mov rsp, gs:[8]", "swapgs", "sysretq",
        dispatch_syscall = sym dispatch_syscall,
        );
    }
}
#[repr(C)] struct SyscallRet { result: u64, status: u64 }
#[no_mangle]
unsafe extern "C" fn dispatch_syscall(nr: u64, arg0: u64, _arg1: u64, cap_bits: u64, _arg2: u64, _arg3: u64, _arg4: u64) -> SyscallRet {
    let cap = Cap::from_bits(cap_bits);
    match krate_dispatch(nr, arg0, cap) { Ok(v) => SyscallRet { result: v, status: 0 }, Err(_) => SyscallRet { result: 0, status: 1 } }
}
fn krate_dispatch(nr: u64, arg: u64, cap: Cap) -> Result<u64, &'static str> {
    // Minimal R0 dispatch — full dispatch lives in k_hal (cap_gate) and calls back via crate::paging::krate_dispatch for R0 ops.
    // Here we only handle PING for host tests; real dispatch uses k_hal.
    dispatch_check_sandbox(nr, cap)?;
    match nr {
        SYS_PING => { if !cap.contains(Cap::PING) { return Err("EPERM: Cap::PING"); } Ok(PING_COUNT.fetch_add(1, Ordering::Relaxed) + 1) }
        _ => Err("ENOSYS"),
    }
}
pub fn soft_syscall(nr: u64, arg: u64, cap: Cap) -> Result<u64, &'static str> {
    stage_syscall(nr, arg, cap);
    unsafe { core::arch::asm!("int 0x90", options(nostack)); }
    if SYS_STATUS.load(Ordering::SeqCst) != 0 { Err("mvp-c: syscall negada") } else { Ok(SYS_RESULT.load(Ordering::SeqCst)) }
}
pub extern "x86-interrupt" fn syscall_int_handler(_stack: x86_64::structures::idt::InterruptStackFrame) {
    // R0 handler (DPL=3 na IDT). Bin patch_idt pode substituir por handler com dispatch k_hal.
    if mailbox_syscalls() {
        syscall_stage_from_mailbox(0);
    } else {
        syscall_try_regs_fallback();
    }
    let nr = SYS_NR.load(Ordering::SeqCst);
    let arg = SYS_ARG.load(Ordering::SeqCst);
    let cap = Cap::from_bits(SYS_CAP.load(Ordering::SeqCst));
    if nr == SYS_EXIT_USER && demo_active() {
        match krate_dispatch(nr, arg, cap) {
            Ok(v) => { SYS_RESULT.store(v, Ordering::SeqCst); SYS_STATUS.store(0, Ordering::SeqCst); return_from_user(true); }
            Err(_) => { SYS_STATUS.store(1, Ordering::SeqCst); return_from_user(false); }
        }
    }
    match krate_dispatch(nr, arg, cap) {
        Ok(v) => { SYS_RESULT.store(v, Ordering::SeqCst); SYS_STATUS.store(0, Ordering::SeqCst); }
        Err(_) => { SYS_STATUS.store(1, Ordering::SeqCst); }
    }
}

// ─── Isolation seam (R0 paging part) ─────────────────────────────────────

/// Phase 5: Hypervisor-aware gating for Ring3 (moved from bin isolation_ring).
pub fn ring3_is_safe() -> bool {
    match crate::platform_probe::hypervisor() {
        crate::platform_probe::HypervisorKind::None => false,
        crate::platform_probe::HypervisorKind::Kvm => true,
        crate::platform_probe::HypervisorKind::MicrosoftHv => false,
        _ => false,
    }
}

/// Blob-only native execution in Ring3 sandbox (W^X USER). ELF path stays in bin.
pub fn ring3_run_native_blob(code: &[u8]) -> Result<i64, &'static str> {
    if code.is_empty() { return Err("ring3: code vazio"); }
    crate::ring3::verify_blob_no_simd(code)?;
    let mut aspace = create_sandbox_as()?;
    let entry = unsafe { jit_write_exec_user(&mut aspace, code) }?;
    const USER_STACK_BASE: u64 = 0x0000_7000_0040_0000;
    const USER_STACK_PAGES: usize = 4;
    let mut stack_frames: [Option<PhysFrame<Size4KiB>>; 4] = [None, None, None, None];
    let mut n_stack = 0usize;
    let mailbox_frame = map_user_mailbox(&mut aspace)?;
    for j in 0..USER_STACK_PAGES {
        let va = USER_STACK_BASE + (j as u64) * 4096;
        let frame = alloc_frame()?;
        stack_frames[j] = Some(frame);
        n_stack = j + 1;
        unsafe { aspace.map_user_page(VirtAddr::new(va), frame, user_data_flags())?; }
    }
    let stack_top = USER_STACK_BASE + (USER_STACK_PAGES as u64) * 4096;
    let code_frame = aspace.frame_for_virt(VirtAddr::new(entry)).ok_or("ring3: code leaf")?;
    crate::slog_nano!("ISO-RING", "info", "ring3_run_native_blob: blob @{:#x} stack @{:#x}", entry, stack_top);
    let result = unsafe { x86_64::instructions::interrupts::without_interrupts(|| enter_user_mode(entry, stack_top, aspace.l4_frame, Cap::ENTER_USER)) };
    free_frame(code_frame);
    free_frame(mailbox_frame);
    for f in stack_frames[..n_stack].iter().flatten() {
        free_frame(*f);
    }
    free_frame(aspace.l4_frame);
    match result { Ok(()) => Ok(0), Err(e) => Err(e) }
}

/// Re-export H3 gate separado de hypervisor vendor table.
pub fn ring3_can_register_native() -> bool {
    crate::ring3::ring3_can_register_native()
}

// Host SSE stub gated to avoid STATUS_ILLEGAL_INSTRUCTION soft-float (lesson SSE)
#[cfg(all(x86_64, not(target_os="none")))]
pub fn host_sse_stub() {}

// ─── P6 demos (ADR-0102 H2) ───────────────────────────────────────────────

fn demo_write_stub(code: PhysFrame<Size4KiB>) {
    let mut buf = [0u8; 48];
    let mut o = 0usize;
    let result_va = USER_MARKER_VA + USER_DEMO_MARKER_OFF;
    buf[o] = 0x48; buf[o + 1] = 0xB8; o += 2;
    buf[o..o + 8].copy_from_slice(&result_va.to_le_bytes()); o += 8;
    buf[o] = 0x48; buf[o + 1] = 0xB9; o += 2;
    buf[o..o + 8].copy_from_slice(&RING3_MAGIC.to_le_bytes()); o += 8;
    buf[o] = 0x48; buf[o + 1] = 0x89; buf[o + 2] = 0x08; o += 3;
    buf[o] = 0xCD; buf[o + 1] = 0x90; o += 2;
    buf[o] = 0xF4; o += 1;
    let dst = hhdm_mut::<u8>(code);
    unsafe { core::ptr::copy_nonoverlapping(buf.as_ptr(), dst, o); }
}

fn demo_write_fault_stub(code: PhysFrame<Size4KiB>, bad_va: u64) {
    let mut buf = [0u8; 24];
    let mut o = 0usize;
    buf[o] = 0x48; buf[o + 1] = 0xB8; o += 2;
    buf[o..o + 8].copy_from_slice(&bad_va.to_le_bytes()); o += 8;
    buf[o] = 0x48; buf[o + 1] = 0x8B; buf[o + 2] = 0x00; o += 3;
    buf[o] = 0xF4; o += 1;
    let dst = hhdm_mut::<u8>(code);
    unsafe { core::ptr::copy_nonoverlapping(buf.as_ptr(), dst, o); }
}

fn demo_write_capgate_stub(code: PhysFrame<Size4KiB>) {
    let mut buf = [0u8; 80];
    let mut o = 0usize;
    buf[o] = 0x48; buf[o + 1] = 0xB8; o += 2;
    buf[o..o + 8].copy_from_slice(&crate::ring3::USER_MAILBOX_VA.to_le_bytes()); o += 8;
    for nr in [SYS_PIN_DMA, SYS_MAP_FB, SYS_EXIT_USER] {
        buf[o] = 0x48; buf[o + 1] = 0xC7; buf[o + 2] = 0x00; o += 3;
        buf[o..o + 4].copy_from_slice(&(nr as u32).to_le_bytes()); o += 4;
        buf[o] = 0xCD; buf[o + 1] = 0x90; o += 2;
    }
    buf[o] = 0xF4; o += 1;
    let dst = hhdm_mut::<u8>(code);
    unsafe { core::ptr::copy_nonoverlapping(buf.as_ptr(), dst, o); }
}

fn demo_write_sse_stub(code: PhysFrame<Size4KiB>) {
    let mut buf = [0u8; 8];
    buf[0] = 0x0F; buf[1] = 0x57; buf[2] = 0xC0; buf[3] = 0xF4;
    let dst = hhdm_mut::<u8>(code);
    unsafe { core::ptr::copy_nonoverlapping(buf.as_ptr(), dst, 4); }
}

fn demo_free_triplet(aspace: &AddressSpace, code: PhysFrame<Size4KiB>, stack: PhysFrame<Size4KiB>, marker: Option<PhysFrame<Size4KiB>>) {
    free_frame(code);
    free_frame(stack);
    if let Some(m) = marker { free_frame(m); }
    free_frame(aspace.l4_frame);
}

/// Demo non-fatal: Cap deny → iretq round-trip → marker → SYS_EXIT_USER.
pub fn demo_ring3() -> Result<(), &'static str> {
    if !TRY_ENTER_RING3 { return Ok(()); }
    if phys_offset() == 0 {
        crate::slog_nano!("P6", "info", "PHYS_MEM_OFFSET=0 — Ring3 demo SKIP");
        return Ok(());
    }
    let deny = unsafe {
        enter_user_mode(USER_CODE_VA, USER_STACK_VA + 0x1000, kernel_cr3().0, Cap::EMPTY)
    };
    if deny.is_ok() { return Err("P6: Cap vazia nao deveria entrar"); }

    let mut as_user = create_sandbox_as()?;
    let code_frame = alloc_frame()?;
    let stack_frame = alloc_frame()?;
    let marker_frame = alloc_frame()?;
    demo_write_stub(code_frame);
    unsafe {
        as_user.map_user_page(VirtAddr::new(USER_CODE_VA), code_frame, user_code_flags())?;
        as_user.map_user_page(VirtAddr::new(USER_STACK_VA), stack_frame, user_data_flags())?;
        as_user.map_user_page(VirtAddr::new(USER_MARKER_VA), marker_frame, user_data_flags())?;
        let mb = hhdm_mut::<crate::ring3::SyscallMailbox>(marker_frame);
        (*mb).nr = SYS_EXIT_USER;
        (*mb).cap = Cap::ENTER_USER.bits();
    }
    let stack_top = USER_STACK_VA + 0x1000;
    x86_64::instructions::interrupts::without_interrupts(|| unsafe {
        enter_user_mode(USER_CODE_VA, stack_top, as_user.l4_frame, Cap::ENTER_USER)
    })?;
    let marker = unsafe {
        let base = hhdm_mut::<u8>(marker_frame);
        core::ptr::read_volatile(base.add(USER_DEMO_MARKER_OFF as usize) as *const u64)
    };
    demo_free_triplet(&as_user, code_frame, stack_frame, Some(marker_frame));
    if marker != RING3_MAGIC { return Err("P6: marker Ring3 nao escrito"); }
    ring3_note_iretq_ok();
    crate::slog_nano!("P6", "info", "SUCCESS iretq+CPL3 marker={:x}", marker);
    Ok(())
}

pub fn demo_ring3_fault_containment() -> Result<(), &'static str> {
    if !TRY_ENTER_RING3 { return Ok(()); }
    const UNMAPPED_VA: u64 = 0x0000_7000_0040_0000;
    let mut as_user = create_sandbox_as()?;
    let code_frame = alloc_frame()?;
    let stack_frame = alloc_frame()?;
    demo_write_fault_stub(code_frame, UNMAPPED_VA);
    unsafe {
        as_user.map_user_page(VirtAddr::new(USER_CODE_VA), code_frame, user_code_flags())?;
        as_user.map_user_page(VirtAddr::new(USER_STACK_VA), stack_frame, user_data_flags())?;
    }
    let r = x86_64::instructions::interrupts::without_interrupts(|| unsafe {
        enter_user_mode(USER_CODE_VA, USER_STACK_VA + 0x1000, as_user.l4_frame, Cap::ENTER_USER)
    });
    demo_free_triplet(&as_user, code_frame, stack_frame, None);
    match r {
        Err(e) if e.contains("fault") || e.contains("P6") => {
            crate::slog_nano!("P6", "info", "SUCCESS fault-containment ({})", e);
            Ok(())
        }
        Ok(()) => Err("P6: fault stub nao gerou falta"),
        Err(e) => Err(e),
    }
}

pub fn demo_ring3_capgate_dma_mmio() -> Result<(), &'static str> {
    if !TRY_ENTER_RING3 { return Ok(()); }
    reset_sandbox_denies();
    let mut as_user = create_sandbox_as()?;
    let code_frame = alloc_frame()?;
    let stack_frame = alloc_frame()?;
    let marker_frame = alloc_frame()?;
    demo_write_capgate_stub(code_frame);
    unsafe {
        as_user.map_user_page(VirtAddr::new(USER_CODE_VA), code_frame, user_code_flags())?;
        as_user.map_user_page(VirtAddr::new(USER_STACK_VA), stack_frame, user_data_flags())?;
        as_user.map_user_page(VirtAddr::new(USER_MARKER_VA), marker_frame, user_data_flags())?;
        hhdm_mut::<u64>(marker_frame).write_volatile(0);
    }
    set_use_mailbox(true);
    set_stage_exit(false);
    let r = x86_64::instructions::interrupts::without_interrupts(|| unsafe {
        enter_user_mode(USER_CODE_VA, USER_STACK_VA + 0x1000, as_user.l4_frame, Cap::ENTER_USER)
    });
    set_use_mailbox(false);
    set_stage_exit(true);
    let dma = sandbox_dma_denies();
    let mmio = sandbox_mmio_denies();
    demo_free_triplet(&as_user, code_frame, stack_frame, Some(marker_frame));
    r?;
    if dma == 0 || mmio == 0 { return Err("P6: sandbox nao negou PIN_DMA/MAP_FB"); }
    crate::slog_nano!("P6", "info", "SUCCESS CapGate deny PIN_DMA={} MAP_FB={}", dma, mmio);
    Ok(())
}

/// T-056 opção A: verificador rejeita blob SSE **antes** do iretq (sem HW).
pub fn demo_ring3_t056_opcode_gate() -> Result<(), &'static str> {
    let sse_blob = [0x0F, 0x57, 0xC0, 0xC3]; // xorps xmm0,xmm0; ret
    match crate::ring3::verify_blob_no_simd(&sse_blob) {
        Err(_) => {
            crate::slog_nano!("P6", "info", "SUCCESS T-056 opcode gate rejeitou xorps");
            Ok(())
        }
        Ok(()) => Err("P6: T-056 deveria rejeitar xorps"),
    }
}

pub fn demo_ring3_softfloat_sse() -> Result<(), &'static str> {
    if !TRY_ENTER_RING3 { return Ok(()); }
    let mut as_user = create_sandbox_as()?;
    let code_frame = alloc_frame()?;
    let stack_frame = alloc_frame()?;
    demo_write_sse_stub(code_frame);
    unsafe {
        as_user.map_user_page(VirtAddr::new(USER_CODE_VA), code_frame, user_code_flags())?;
        as_user.map_user_page(VirtAddr::new(USER_STACK_VA), stack_frame, user_data_flags())?;
    }
    let mut cr0: u64;
    unsafe { core::arch::asm!("mov {}, cr0", out(reg) cr0, options(nostack)); }
    saved_cr0_store(cr0);
    let em = cr0 | (1 << 2);
    unsafe { core::arch::asm!("mov cr0, {}", in(reg) em, options(nostack)); }
    let r = x86_64::instructions::interrupts::without_interrupts(|| unsafe {
        enter_user_mode(USER_CODE_VA, USER_STACK_VA + 0x1000, as_user.l4_frame, Cap::ENTER_USER)
    });
    let cr0_now = saved_cr0_take();
    if cr0_now != 0 {
        unsafe { core::arch::asm!("mov cr0, {}", in(reg) cr0_now, options(nostack)); }
    }
    demo_free_triplet(&as_user, code_frame, stack_frame, None);
    match r {
        Err(e) if e.contains("fault") || e.contains("P6") => {
            crate::slog_nano!("P6", "info", "SUCCESS soft-float SSE #UD contained ({})", e);
            Ok(())
        }
        Ok(()) => Err("P6: xorps nao gerou #UD"),
        Err(e) => Err(e),
    }
}

/// H3: self-test de boot — roda demo_ring3 e cacheia resultado.
pub fn ring3_self_test_iretq() -> bool {
    match demo_ring3() {
        Ok(()) => true,
        Err(e) => {
            crate::slog_nano!("P6", "warn", "ring3_self_test_iretq FAIL: {}", e);
            false
        }
    }
}
