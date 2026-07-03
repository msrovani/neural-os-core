use linked_list_allocator::LockedHeap;
use x86_64::structures::paging::{FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB};
use x86_64::VirtAddr;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

pub const HEAP_START: usize = 0x_4444_4444_0000;
pub const HEAP_SIZE: usize = 16 * 1024 * 1024;

pub static CURRENT_HEAP_MB: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(16);

pub const SLAB_START: usize = HEAP_START;
pub const SLAB_SIZE: usize = 8 * 65536;
pub const LARGE_HEAP_START: usize = HEAP_START + SLAB_SIZE;
pub const LARGE_HEAP_SIZE: usize = HEAP_SIZE - SLAB_SIZE;

pub fn try_alloc_check() -> bool {
    let heap = ALLOCATOR.lock();
    heap.size() > 0
}

/// Expande o heap dinamicamente baseado no tamanho do modelo AI.
/// Usa frame allocator global + map_page_uc para mapear novas paginas.
/// Thread-safe: so cresce, nunca encolhe.
pub fn resize_heap_to_mb(target_mb: usize) {
    let current = CURRENT_HEAP_MB.load(core::sync::atomic::Ordering::SeqCst);
    if target_mb <= current { return; }
    let diff_pages = (target_mb - current) * 256;
    let pmoff = crate::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
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
        let virt = start_virt + (i as u64 * 4096);
        unsafe {
            crate::apic::map_page_uc(phys, pmoff);
            ALLOCATOR.lock().extend(4096);
        }
        allocated += 1;
    }

    let new_mb = current + allocated / 256;
    CURRENT_HEAP_MB.store(new_mb, core::sync::atomic::Ordering::SeqCst);
    crate::serial_println!("[HEAP] {} MB → {} MB ({} pages added)", current, new_mb, allocated);
}

pub fn heap_stats() -> (usize, usize) {
    let guard = ALLOCATOR.lock();
    (guard.used(), guard.free())
}

#[alloc_error_handler]
fn oom(_: core::alloc::Layout) -> ! {
    use core::fmt::Write;
    {
        let mut w = crate::vga_buffer::WRITER.lock();
        if let Some(ref mut w) = *w { let _ = write!(w, "[OOM] sem memoria"); }
    }
    {
        let mut s = crate::serial::SERIAL.lock();
        if let Some(ref mut s) = *s { let _ = write!(s, "[OOM] sem memoria. Aumente HEAP_SIZE.\n"); }
    }
    loop { x86_64::instructions::hlt(); }
}

pub fn init_heap(
    mapper: &mut impl Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) -> Result<(), &'static str> {
    let page_range = {
        let heap_start = VirtAddr::new(HEAP_START as u64);
        let heap_end = heap_start + HEAP_SIZE as u64 - 1u64;
        let heap_start_page = Page::containing_address(heap_start);
        let heap_end_page = Page::containing_address(heap_end);
        Page::range_inclusive(heap_start_page, heap_end_page)
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
        crate::slab::SLAB_ALLOCATOR.lock().init(SLAB_START);
        ALLOCATOR.lock().init(LARGE_HEAP_START, LARGE_HEAP_SIZE);
    }

    Ok(())
}
