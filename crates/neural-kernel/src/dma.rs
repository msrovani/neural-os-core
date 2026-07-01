//! DMA buffer allocation — páginas marcadas como UC para dispositivos PCI.
//! Previne corrupção por cache incoerente entre CPU e DMA.

use core::sync::atomic::Ordering;
use crate::memory::{GLOBAL_ALLOCATOR, PHYS_MEM_OFFSET};

/// Buffer DMA — memória compartilhada com dispositivo PCI
pub struct DmaBuf {
    pub phys: u64,
    pub virt: *mut u8,
    pub size: usize,
}

unsafe impl Send for DmaBuf {}

impl DmaBuf {
    pub fn as_ptr(&self) -> *const u8 { self.virt }
    pub fn as_mut_ptr(&mut self) -> *mut u8 { self.virt }
    pub unsafe fn as_slice(&self) -> &[u8] { core::slice::from_raw_parts(self.virt, self.size) }
    pub unsafe fn as_mut_slice(&mut self) -> &mut [u8] { core::slice::from_raw_parts_mut(self.virt, self.size) }
}

/// Aloca páginas de DMA uncacheable. Usa `set_page_uc` do apic para marcar cada página.
pub fn dma_alloc(size: usize) -> Option<DmaBuf> {
    let pages = (size + 4095) / 4096;
    if pages == 0 { return None; }
    let pa = unsafe {
        
        let mut guard = GLOBAL_ALLOCATOR.lock();
        let alloc = (*guard).as_mut()?;
        let frame = alloc.allocate_contiguous(pages)?;
        let pa = frame.start_address().as_u64();
        for i in 0..pages {
            crate::apic::set_page_uc(pa + i as u64 * 4096, PHYS_MEM_OFFSET.load(Ordering::Relaxed));
        }
        let va = (pa + PHYS_MEM_OFFSET.load(Ordering::Relaxed)) as *mut u8;
        core::ptr::write_bytes(va, 0, pages * 4096);
        pa
    };
    let virt = (pa + PHYS_MEM_OFFSET.load(Ordering::Relaxed)) as *mut u8;
    Some(DmaBuf { phys: pa, virt, size: pages * 4096 })
}

/// Libera páginas DMA
pub fn dma_free(buf: DmaBuf) {
    let phys = buf.phys;
    let pages = (buf.size + 4095) / 4096;
    unsafe {
        use x86_64::structures::paging::{FrameDeallocator, PhysFrame, Size4KiB};
        use x86_64::PhysAddr;
        let mut guard = GLOBAL_ALLOCATOR.lock();
        if let Some(alloc) = (*guard).as_mut() {
            for i in 0..pages {
                let f = PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(phys + i as u64 * 4096));
                alloc.deallocate_frame(f);
            }
        }
    }
}
