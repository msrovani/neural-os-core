use alloc::string::String;
use alloc::vec::Vec;
use x86_64::PhysAddr;
use x86_64::structures::paging::{FrameAllocator, FrameDeallocator};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AllocTier {
    Dram,
    Vram,
    Nvme,
    Hdd,
}

#[derive(Debug, Clone)]
pub struct MemoryTier {
    pub kind: AllocTier,
    pub capacity_bytes: u64,
    pub bandwidth_mbs: u32,
    pub latency_ns: u32,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct MemoryHierarchy {
    pub tiers: Vec<MemoryTier>,
}

impl MemoryHierarchy {
    pub fn new() -> Self {
        let total_ram = estimate_total_ram();
        MemoryHierarchy {
            tiers: alloc::vec![
                MemoryTier {
                    kind: AllocTier::Dram,
                    capacity_bytes: total_ram,
                    bandwidth_mbs: 20000,
                    latency_ns: 100,
                    name: String::from("DRAM"),
                },
            ],
        }
    }

    pub fn best_tier(&self) -> AllocTier {
        AllocTier::Dram
    }
}

fn estimate_total_ram() -> u64 {
    let guard = crate::memory::GLOBAL_ALLOCATOR.lock();
    guard.as_ref().map_or(0, |a| a.usable_memory_bytes())
}

pub fn alloc_by_tier(tier: AllocTier, size: usize) -> Option<PhysAddr> {
    match tier {
        AllocTier::Dram => {
            let num_frames = (size + 4095) / 4096;
            if num_frames == 0 { return None; }
            let mut guard = crate::memory::GLOBAL_ALLOCATOR.lock();
            let alloc = guard.as_mut()?;
            if num_frames == 1 {
                let frame = alloc.allocate_frame()?;
                return Some(frame.start_address());
            }
            let contiguous = alloc.allocate_contiguous(num_frames);
            if let Some(frame) = contiguous {
                return Some(frame.start_address());
            }
            // Fallback: allocate non-contiguous frames, free on failure
            let mut frames = alloc::vec::Vec::new();
            for _ in 0..num_frames {
                match alloc.allocate_frame() {
                    Some(f) => frames.push(f),
                    None => {
                        // Free already-allocated frames
                        for f in frames {
                            unsafe { alloc.deallocate_frame(f); }
                        }
                        return None;
                    }
                }
            }
            Some(frames[0].start_address())
        }
        AllocTier::Vram => {
            crate::serial_println!("[MHI] Vram alloc not implemented (no GPU driver)");
            None
        }
        AllocTier::Nvme => {
            crate::serial_println!("[MHI] Nvme alloc not implemented (no NVMe driver)");
            None
        }
        AllocTier::Hdd => {
            crate::serial_println!("[MHI] Hdd alloc not implemented (no storage driver)");
            None
        }
    }
}
