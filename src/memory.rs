use bootloader::bootinfo::{MemoryMap, MemoryRegionType};
use x86_64::structures::paging::{FrameAllocator, OffsetPageTable, PageTable, PhysFrame, Size4KiB};
use x86_64::PhysAddr;
use x86_64::VirtAddr;

#[allow(dead_code)]
/// Trait para desalocação de frames físicos de 4KiB.
/// TODO: Sprint 9 — implementar bitmap/free-list frame deallocator.
pub trait FrameDeallocator {
    fn deallocate_frame(&mut self, frame: PhysFrame<Size4KiB>);
}

#[allow(dead_code)]
/// Deallocator vazio que apenas descarta frames.
/// TODO: Substituir por implementação real (bitmap/slab).
pub struct EmptyFrameDeallocator;

impl FrameDeallocator for EmptyFrameDeallocator {
    fn deallocate_frame(&mut self, _frame: PhysFrame<Size4KiB>) {
        // No-op: frames não são reutilizados até implementação do bitmap.
    }
}

pub struct BootInfoFrameAllocator {
    memory_map: &'static MemoryMap,
    next: usize,
}

impl BootInfoFrameAllocator {
    pub unsafe fn init(memory_map: &'static MemoryMap) -> Self {
        BootInfoFrameAllocator {
            memory_map,
            next: 0,
        }
    }

    fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> {
        let regions = self.memory_map.iter();
        let usable_regions = regions.filter(|r| r.region_type == MemoryRegionType::Usable);
        let addr_ranges = usable_regions.map(|r| r.range.start_addr()..r.range.end_addr());
        let frame_addresses = addr_ranges.flat_map(|r| r.step_by(4096));
        frame_addresses.map(|addr| PhysFrame::containing_address(PhysAddr::new(addr)))
    }
}

unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        let frame = self.usable_frames().nth(self.next);
        self.next += 1;
        frame
    }
}

pub unsafe fn init_memory(physical_memory_offset: u64) -> OffsetPageTable<'static> {
    let (level_4_frame, _) = x86_64::registers::control::Cr3::read();
    let phys = level_4_frame.start_address();
    let virt = VirtAddr::new(physical_memory_offset) + phys.as_u64();
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();
    let page_table = unsafe { &mut *page_table_ptr };
    unsafe { OffsetPageTable::new(page_table, VirtAddr::new(physical_memory_offset)) }
}
