//! Tier 1 — Global allocator (Hermes / JARBAS / UI).
//! talc substitui linked_list_allocator: menos fragmentação em alocações variadas.

use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Mutex;
use talc::{ErrOnOom, Span, Talc, Talck};
use x86_64::structures::paging::{FrameAllocator, Mapper, Page, PageTable, PageTableFlags, Size4KiB};
use x86_64::{PhysAddr, VirtAddr};

#[global_allocator]
static TALC_ALLOC: Talck<spin::Mutex<()>, ErrOnOom> = Talck::new(Talc::new(ErrOnOom));

static CLAIMED_HEAP: Mutex<Option<Span>> = Mutex::new(None);

pub const HEAP_START: usize = 0x_4000_0000_0000;
/// 4GB: BitNet 1.3B/2B (~320-604MB packed) + copia + load_model sem resize_heap lento (WHPX).
pub const HEAP_SIZE: usize = 4096 * 1024 * 1024;

pub static CURRENT_HEAP_MB: AtomicUsize = AtomicUsize::new(4096);

pub const SLAB_START: usize = HEAP_START;
pub const SLAB_SIZE: usize = 8 * 65536;
pub const LARGE_HEAP_START: usize = HEAP_START + SLAB_SIZE;
pub const LARGE_HEAP_SIZE: usize = HEAP_SIZE - SLAB_SIZE;

pub fn try_alloc_check() -> bool {
    CLAIMED_HEAP.lock().is_some()
}

pub fn resize_heap_to_mb(target_mb: usize) {
    let current = CURRENT_HEAP_MB.load(Ordering::SeqCst);
    if target_mb <= current {
        return;
    }
    let diff_pages = (target_mb - current) * 256;
    let pmoff = crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    let base = VirtAddr::new(pmoff);
    let start_virt = HEAP_START as u64 + (current as u64 * 1024 * 1024);

    let mut allocated = 0usize;
    for i in 0..diff_pages {
        let phys = {
            let mut g = crate::memory::GLOBAL_ALLOCATOR.lock();
            match g.as_mut().and_then(|a| a.allocate_frame()) {
                Some(f) => f.start_address().as_u64(),
                None => break,
            }
        };
        let virt = VirtAddr::new(start_virt + (i as u64 * 4096));
        unsafe {
            map_page_direct(base, virt, phys);
        }
        allocated += 1;
    }

    if allocated > 0 {
        let new_mb = current + allocated / 256;
        let new_size = new_mb * 1024 * 1024;
        unsafe {
            let mut guard = TALC_ALLOC.lock();
            if let Some(old) = *CLAIMED_HEAP.lock() {
                let req = Span::from_base_size(LARGE_HEAP_START as *mut u8, new_size - SLAB_SIZE);
                let extended = guard.extend(old, req);
                *CLAIMED_HEAP.lock() = Some(extended);
            }
        }
        CURRENT_HEAP_MB.store(new_mb, Ordering::SeqCst);
        k_nano::slog_bin!("HEAP", "TALC", "{} MB → {} MB ({} pages added)",
            current,
            new_mb,
            allocated);
    }
}

unsafe fn map_page_direct(base: VirtAddr, virt: VirtAddr, phys: u64) {
    let (l4_frame, _) = x86_64::registers::control::Cr3::read();
    let l4_virt = base + l4_frame.start_address().as_u64();
    let l4_tbl = &mut *(l4_virt.as_mut_ptr::<PageTable>());
    let e3 = &mut l4_tbl[virt.p4_index()];
    if !e3.flags().contains(PageTableFlags::PRESENT) {
        let f = alloc_pt_frame(base);
        if f == 0 {
            return;
        }
        e3.set_addr(PhysAddr::new(f), PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
    }
    let l3_virt = base + e3.addr().as_u64();
    let l3_tbl = &mut *(l3_virt.as_mut_ptr::<PageTable>());
    let e2 = &mut l3_tbl[virt.p3_index()];
    if !e2.flags().contains(PageTableFlags::PRESENT) {
        let f = alloc_pt_frame(base);
        if f == 0 {
            return;
        }
        e2.set_addr(PhysAddr::new(f), PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
    }
    let l2_virt = base + e2.addr().as_u64();
    let l2_tbl = &mut *(l2_virt.as_mut_ptr::<PageTable>());
    let e1 = &mut l2_tbl[virt.p2_index()];
    if !e1.flags().contains(PageTableFlags::PRESENT) {
        let f = alloc_pt_frame(base);
        if f == 0 {
            return;
        }
        e1.set_addr(PhysAddr::new(f), PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
    }
    if e1.flags().contains(PageTableFlags::HUGE_PAGE) {
        // 2MB PDE — nao e buraco 4KiB; nao sobrescrever
        return;
    }
    let l1_virt = base + e1.addr().as_u64();
    let l1_tbl = &mut *(l1_virt.as_mut_ptr::<PageTable>());
    let pte = &mut l1_tbl[virt.p1_index()];
    if pte.flags().contains(PageTableFlags::PRESENT) {
        return;
    }
    pte.set_addr(PhysAddr::new(phys), PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
    x86_64::instructions::tlb::flush(virt);
}

/// #PF cure: buraco no heap Tier-1 (BitNet `to_vec` mid-load). Retorna true se page presente apos.
pub fn try_fault_in_heap(cr2: u64) -> bool {
    let start = HEAP_START as u64;
    let end = start + (CURRENT_HEAP_MB.load(Ordering::Relaxed) as u64) * 1024 * 1024;
    if cr2 < start || cr2 >= end {
        return false;
    }
    let pmoff = crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    if pmoff == 0 {
        return false;
    }
    let virt = VirtAddr::new(cr2 & !0xFFF);
    let base = VirtAddr::new(pmoff);
    // Ja presente? So TLB stale apos CR3 switch — flush e retry.
    if unsafe { heap_pte_present(base, virt) } {
        x86_64::instructions::tlb::flush(virt);
        return true;
    }
    let phys = match crate::memory::alloc_physical_frame() {
        Some(f) => f.start_address().as_u64(),
        None => return false,
    };
    unsafe {
        map_page_direct(base, virt, phys);
        heap_pte_present(base, virt)
    }
}

unsafe fn heap_pte_present(base: VirtAddr, virt: VirtAddr) -> bool {
    let (l4_frame, _) = x86_64::registers::control::Cr3::read();
    let l4 = &*( (base + l4_frame.start_address().as_u64()).as_ptr::<PageTable>() );
    let e3 = &l4[virt.p4_index()];
    if !e3.flags().contains(PageTableFlags::PRESENT) {
        return false;
    }
    let l3 = &*( (base + e3.addr().as_u64()).as_ptr::<PageTable>() );
    let e2 = &l3[virt.p3_index()];
    if !e2.flags().contains(PageTableFlags::PRESENT) {
        return false;
    }
    if e2.flags().contains(PageTableFlags::HUGE_PAGE) {
        return true;
    }
    let l2 = &*( (base + e2.addr().as_u64()).as_ptr::<PageTable>() );
    let e1 = &l2[virt.p2_index()];
    if !e1.flags().contains(PageTableFlags::PRESENT) {
        return false;
    }
    if e1.flags().contains(PageTableFlags::HUGE_PAGE) {
        return true;
    }
    let l1 = &*( (base + e1.addr().as_u64()).as_ptr::<PageTable>() );
    l1[virt.p1_index()].flags().contains(PageTableFlags::PRESENT)
}

unsafe fn alloc_pt_frame(base: VirtAddr) -> u64 {
    use x86_64::structures::paging::FrameAllocator;
    let mut g = crate::memory::GLOBAL_ALLOCATOR.lock();
    if let Some(alloc) = g.as_mut() {
        if let Some(frame) = alloc.allocate_frame() {
            let pa = frame.start_address().as_u64();
            core::ptr::write_bytes((base + pa).as_mut_ptr::<u8>(), 0, 4096);
            return pa;
        }
    }
    0
}

pub fn heap_stats() -> (usize, usize) {
    let claimed = CLAIMED_HEAP.lock();
    if let Some(span) = *claimed {
        (0, span.size())
    } else {
        (0, 0)
    }
}

#[alloc_error_handler]
fn oom(_: core::alloc::Layout) -> ! {
    use core::fmt::Write;
    {
        let mut w = crate::vga_buffer::WRITER.lock();
        if let Some(ref mut w) = *w {
            let _ = write!(w, "[OOM/TALC] sem memoria");
        }
    }
    {
        let mut s = crate::serial::SERIAL.lock();
        if let Some(ref mut s) = *s {
            let _ = write!(s, "[OOM/TALC] sem memoria Tier 1. Verifique HEAP_SIZE.\n");
        }
    }
    loop {
        x86_64::instructions::hlt();
    }
}

pub fn init_heap(
    mapper: &mut impl Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) -> Result<(), &'static str> {
    let page_range = {
        let heap_start = VirtAddr::new(HEAP_START as u64);
        let heap_end = heap_start + HEAP_SIZE as u64 - 1u64;
        Page::range_inclusive(
            Page::containing_address(heap_start),
            Page::containing_address(heap_end),
        )
    };

    for page in page_range {
        let frame = frame_allocator
            .allocate_frame()
            .ok_or("failed to allocate frame")?;
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        unsafe {
            mapper
                .map_to(page, frame, flags, frame_allocator)
                .map_err(|_| "failed to map page")?
                .flush();
        }
    }

    unsafe {
        k_nano::slab::SLAB_ALLOCATOR.lock().init(SLAB_START);
        let span = Span::from_base_size(LARGE_HEAP_START as *mut u8, LARGE_HEAP_SIZE);
        let claimed = TALC_ALLOC.lock().claim(span).map_err(|_| "talc claim failed")?;
        *CLAIMED_HEAP.lock() = Some(claimed);
    }

    k_nano::slog_bin!("HEAP", "TALC", "Tier 1 ready: virt={:#x} size={} MB (Hermes/JARBAS/UI)",
        LARGE_HEAP_START,
        LARGE_HEAP_SIZE / (1024 * 1024));
    Ok(())
}
