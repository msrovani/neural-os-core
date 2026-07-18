//! AddressSpace — page tables próprias + troca de CR3 (MVP C / ADR-0041).
//! Shallow-copy do L4 kernel; mapeamentos privados com CoW no caminho.

use core::sync::atomic::Ordering;
use x86_64::registers::control::{Cr3, Cr3Flags};
use x86_64::structures::paging::{PageTable, PageTableFlags, PhysFrame, Size4KiB};
use x86_64::VirtAddr;

use crate::memory::{alloc_physical_frame, PHYS_MEM_OFFSET};

/// VA base para páginas privadas/shared do MVP C (L4 index 224 — fora do heap).
pub const MVP_REGION_BASE: u64 = 0x0000_7000_0000_0000;
pub const SHARED_RING_VA: u64 = MVP_REGION_BASE;
pub const PRIVATE_PAGE_VA: u64 = MVP_REGION_BASE + 0x1000;

pub struct AddressSpace {
    pub l4_frame: PhysFrame<Size4KiB>,
}

fn phys_offset() -> u64 {
    PHYS_MEM_OFFSET.load(Ordering::Acquire)
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
    let frame = alloc_physical_frame()?;
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
    /// Clona o L4 atual (shallow): herda mapas do kernel.
    pub fn clone_current() -> Result<Self, &'static str> {
        let (src_l4, _) = Cr3::read();
        let dst_l4 = unsafe { clone_table(src_l4)? };
        Ok(Self { l4_frame: dst_l4 })
    }

    /// Garante entrada de nível intermediário privada (aloca ou CoW se já PRESENT).
    /// `user`: propaga USER_ACCESSIBLE em todos os níveis (obrigatório p/ CPL=3).
    unsafe fn ensure_owned_child(
        entry: &mut x86_64::structures::paging::page_table::PageTableEntry,
        user: bool,
    ) -> Result<PhysFrame<Size4KiB>, &'static str> {
        if entry.flags().contains(PageTableFlags::HUGE_PAGE) {
            return Err("mvp-c: huge page no caminho");
        }
        let mut parent = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        if user {
            parent.insert(PageTableFlags::USER_ACCESSIBLE);
        }
        if !entry.flags().contains(PageTableFlags::PRESENT) {
            let f = alloc_zeroed_frame().ok_or("mvp-c: sem frame PT")?;
            entry.set_addr(f.start_address(), parent);
            return Ok(f);
        }
        // CoW: clona PT compartilhada (ou já presente) antes de mutar folhas.
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

    /// Mapeia `virt` → `frame` sem mutar page tables compartilhadas do kernel.
    pub unsafe fn map_page(
        &mut self,
        virt: VirtAddr,
        frame: PhysFrame<Size4KiB>,
        flags: PageTableFlags,
    ) -> Result<(), &'static str> {
        self.map_page_inner(virt, frame, flags, false)
    }

    /// Mapeia página acessível em CPL=3 (USER em toda a cadeia PT).
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

    /// Reserva VA: CoW do caminho PT, leaf fica NOT PRESENT (demand-paging P7).
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
}

/// Instala leaf PRESENT no CR3 atual sem alocar frames intermediários (#PF-safe).
pub unsafe fn install_present_leaf_current(
    virt: VirtAddr,
    frame: PhysFrame<Size4KiB>,
    flags: PageTableFlags,
) -> Result<(), &'static str> {
    let (l4_frame, _) = Cr3::read();
    let l4 = &*frame_as_table(l4_frame);
    let e3 = &l4[virt.p4_index()];
    if !e3.flags().contains(PageTableFlags::PRESENT) {
        return Err("p7: P3 ausente");
    }
    let l3 = &*frame_as_table(PhysFrame::containing_address(e3.addr()));
    let e2 = &l3[virt.p3_index()];
    if !e2.flags().contains(PageTableFlags::PRESENT) {
        return Err("p7: P2 ausente");
    }
    let l2 = &*frame_as_table(PhysFrame::containing_address(e2.addr()));
    let e1 = &l2[virt.p2_index()];
    if !e1.flags().contains(PageTableFlags::PRESENT) {
        return Err("p7: P1 ausente");
    }
    let l1 = &mut *frame_as_table(PhysFrame::containing_address(e1.addr()));
    let leaf = &mut l1[virt.p1_index()];
    if leaf.flags().contains(PageTableFlags::PRESENT) {
        return Ok(());
    }
    leaf.set_addr(frame.start_address(), flags);
    x86_64::instructions::tlb::flush(virt);
    Ok(())
}

/// Ponteiro HHDM para um frame físico (válido no CR3 kernel e em clones shallow).
pub fn hhdm_mut<T>(frame: PhysFrame<Size4KiB>) -> *mut T {
    (phys_offset() + frame.start_address().as_u64()) as *mut T
}

pub fn kernel_cr3() -> (PhysFrame<Size4KiB>, Cr3Flags) {
    Cr3::read()
}

pub unsafe fn restore_cr3(frame: PhysFrame<Size4KiB>, flags: Cr3Flags) {
    Cr3::write(frame, flags);
}

pub fn alloc_frame() -> Result<PhysFrame<Size4KiB>, &'static str> {
    alloc_zeroed_frame().ok_or("mvp-c: sem frame fisico")
}

/// Flags RW kernel (dados). Sem USER — heap/kernel permanece inacessível a CPL=3.
pub fn rw_flags() -> PageTableFlags {
    PageTableFlags::PRESENT | PageTableFlags::WRITABLE
}

/// Flags código user (RX lógico; NX não forçado no PoC).
pub fn user_code_flags() -> PageTableFlags {
    PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE
}

/// Flags stack/dados user RW.
pub fn user_data_flags() -> PageTableFlags {
    PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE
}

/// ADR-0041 Fase 4 — AS HAL shallow (PoC ≠ produção).
/// R1: clone L4 + map 1 página BAR UC + switch CR3 + touch + restore.
/// R3: tentativa map BAR sem Cap → Deny (não mapeia).
pub fn demo_as_r1_r3_shallow() {
    k_nano::slog_bin!("AS", "r1", "demo shallow start (PoC monólito)");

    let bar0 = match k_hal::cap_gate::hal_as_bar0() {
        Some(b) if b != 0 && b != 0xffff_ffff_ffff_ffff => b,
        _ => {
            // bind se demo_h5 ainda nao setou
            let fallback = k_hal::discovery::device_tree()
                .into_iter()
                .find(|c| c.id.bar0 != 0)
                .map(|c| c.id.bar0);
            match fallback {
                Some(b) => {
                    k_hal::cap_gate::bind_hal_as(b);
                    b
                }
                None => {
                    k_nano::slog_bin!("AS", "r1", "skip — sem BAR no DeviceTree");
                    return;
                }
            }
        }
    };

    // R3 path: Cap MAP_BAR negado → não mapear BAR no AS “user”
    let r3 = k_hal::cap_gate::check_map_bar(3, false);
    k_nano::slog_bin!("AS", "r3", "MAP_BAR {:?} (expect Deny — sem BAR no AS R3)", r3);
    if r3 == k_hal::cap_gate::CapResult::Deny {
        k_nano::slog_bin!("AS", "r3", "BAR map skipped (Deny honesto)");
    }

    // R1 path: Cap Allow → shallow AS + touch
    if k_hal::cap_gate::check_map_bar(1, true) != k_hal::cap_gate::CapResult::Allow {
        k_nano::slog_bin!("AS", "r1", "unexpected MAP_BAR Deny em R1");
        return;
    }

    let (kernel_l4, kernel_flags) = kernel_cr3();
    let mut as_r1 = match AddressSpace::clone_current() {
        Ok(a) => a,
        Err(e) => {
            k_nano::slog_bin!("AS", "r1", "clone_current fail: {}", e);
            return;
        }
    };

    // VA dedicada no MVP region para shadow do BAR (não o HHDM do kernel)
    const AS_BAR_VA: u64 = MVP_REGION_BASE + 0x2000;
    let bar_frame = PhysFrame::<Size4KiB>::containing_address(x86_64::PhysAddr::new(bar0 & !0xFFF));
    let flags = rw_flags() | PageTableFlags::NO_CACHE;

    let map_ok = unsafe { as_r1.map_page(VirtAddr::new(AS_BAR_VA), bar_frame, flags) };
    match map_ok {
        Ok(()) => k_nano::slog_bin!("AS", "r1", "BAR {:#x} mapped @ {:#x} UC", bar0, AS_BAR_VA),
        Err(e) => {
            // Pode já estar no shallow clone via HHDM — ainda assim switch CR3
            k_nano::slog_bin!("AS", "r1", "map note: {} — continua switch CR3", e);
        }
    }

    unsafe {
        as_r1.activate();
    }
    // Touch via HHDM (sempre válido no monólito) — prova CR3 switch non-fatal
    let pmoff = phys_offset();
    let touch = (bar0.wrapping_add(pmoff)) as *const u32;
    let val = unsafe { core::ptr::read_volatile(touch) };
    k_nano::slog_bin!("AS", "r1", "touch BAR ok val={:#x} (CR3 switched)", val);

    unsafe {
        restore_cr3(kernel_l4, kernel_flags);
    }
    k_nano::slog_bin!(
        "AS",
        "r1",
        "restore CR3 OK — shallow PoC done (≠ isolamento produção)"
    );
}
