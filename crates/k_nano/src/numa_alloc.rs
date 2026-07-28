//! NUMA-aware frame allocator (ADR-0061 Phase 2).
//! 
//! Provides per-NUMA-node frame allocation using the ACPI SRAT topology.
//! Each NUMA node gets its own BitmapFrameAllocator instance for local allocation.

use alloc::vec::Vec;
use ticket_lock::TicketLock;
use x86_64::structures::paging::{FrameAllocator, FrameDeallocator, PhysFrame, Size4KiB};
use x86_64::PhysAddr;

use crate::acpi::NumaMemoryRange;

/// Maximum number of NUMA nodes supported
pub const MAX_NUMA_NODES: usize = 16;

/// Per-NUMA-node frame allocator
pub struct NumaFrameAllocator {
    /// Bitmap allocator for this NUMA node
    allocator: BitmapFrameAllocator,
    /// NUMA node ID (proximity domain)
    node_id: u32,
    /// Memory ranges belonging to this node
    ranges: Vec<NumaMemoryRange>,
    /// APIC IDs associated with this node
    apic_ids: Vec<u32>,
}

impl NumaFrameAllocator {
    /// Create a new NUMA frame allocator for a specific node
    pub fn new(node_id: u32, ranges: Vec<NumaMemoryRange>, apic_ids: Vec<u32>) -> Self {
        Self {
            allocator: BitmapFrameAllocator::empty(),
            node_id,
            ranges,
            apic_ids,
        }
    }

    /// Initialize the allocator with memory ranges from SRAT
    pub fn init(&mut self, _physical_memory_offset: u64) {
        // Convert SRAT ranges to usable ranges for the bitmap allocator
        let mut usable_ranges = Vec::new();
        for range in &self.ranges {
            if range.length > 0 {
                usable_ranges.push((range.base, range.length));
            }
        }
        self.allocator.init_from_usable_ranges(&usable_ranges);
        
        crate::slog_nano!("NUMA", "init", "Node {} initialized with {} ranges, {} APICs", 
            self.node_id, self.ranges.len(), self.apic_ids.len());
    }

    /// Get the NUMA node ID
    pub fn node_id(&self) -> u32 {
        self.node_id
    }

    /// Get APIC IDs for this node
    pub fn apic_ids(&self) -> &[u32] {
        &self.apic_ids
    }

    /// Get memory ranges for this node
    pub fn ranges(&self) -> &[NumaMemoryRange] {
        &self.ranges
    }

    /// Allocate a frame from this NUMA node
    pub fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        self.allocator.allocate_frame()
    }

    /// Allocate contiguous frames from this NUMA node
    pub fn allocate_contiguous(&mut self, count: usize) -> Option<PhysFrame<Size4KiB>> {
        self.allocator.allocate_contiguous(count)
    }

    /// Allocate 2MB huge page from this NUMA node
    pub fn allocate_huge_2mb(&mut self, count: usize) -> Option<PhysFrame<Size4KiB>> {
        self.allocator.allocate_huge_2mb(count)
    }

    /// Allocate 1GB huge page from this NUMA node
    pub fn allocate_huge_1gb(&mut self) -> Option<PhysFrame<Size4KiB>> {
        self.allocator.allocate_huge_1gb()
    }

    /// Deallocate a frame back to this NUMA node
    pub unsafe fn deallocate_frame(&mut self, frame: PhysFrame<Size4KiB>) {
        self.allocator.deallocate_frame(frame);
    }

    /// Get usable memory bytes for this node
    pub fn usable_memory_bytes(&self) -> u64 {
        self.allocator.usable_memory_bytes()
    }

    /// Get allocated frame count for this node
    pub fn allocated_frame_count(&self) -> usize {
        self.allocator.allocated_frame_count()
    }

    /// Get hardware context tensor for this node
    pub fn hardware_context_tensor(&self) -> [f32; 2] {
        self.allocator.hardware_context_tensor()
    }
}

/// Global NUMA allocator registry
static NUMA_ALLOCATORS: TicketLock<Option<Vec<NumaFrameAllocator>>> = TicketLock::new(None);
static NUMA_TOPOLOGY: TicketLock<Option<crate::acpi::NumaTopologyMap>> = TicketLock::new(None);

/// Initialize NUMA allocators from SRAT topology
/// Called after ACPI SRAT parsing during boot
pub fn init_numa_allocators(topology: crate::acpi::NumaTopologyMap, physical_memory_offset: u64) {
    let mut allocators = Vec::new();
    
    // Group memory ranges by proximity domain
    let mut ranges_by_domain: alloc::collections::BTreeMap<u32, Vec<crate::acpi::NumaMemoryRange>> = 
        alloc::collections::BTreeMap::new();
    for range in &topology.memory_ranges {
        ranges_by_domain.entry(range.proximity_domain).or_default().push(*range);
    }
    
    // Group APIC affinities by proximity domain
    let mut apics_by_domain: alloc::collections::BTreeMap<u32, Vec<u32>> = 
        alloc::collections::BTreeMap::new();
    for apic in &topology.apic_affinities {
        apics_by_domain.entry(apic.proximity_domain).or_default().push(apic.apic_id);
    }
    
    // Create allocator for each domain
    for (domain, ranges) in ranges_by_domain {
        let apic_ids = apics_by_domain.remove(&domain).unwrap_or_default();
        let mut allocator = NumaFrameAllocator::new(domain, ranges, apic_ids);
        allocator.init(physical_memory_offset);
        allocators.push(allocator);
    }
    
    let node_count = allocators.len();
    *NUMA_ALLOCATORS.lock() = Some(allocators);
    *NUMA_TOPOLOGY.lock() = Some(topology);
    
    crate::slog_nano!("NUMA", "init", "Initialized {} NUMA node allocators", node_count);
}

/// Get the NUMA allocator for a specific node and execute a closure with it
pub fn with_numa_allocator<F, R>(node_id: u32, f: F) -> Option<R>
where
    F: FnOnce(&mut NumaFrameAllocator) -> Option<R>,
{
    let mut allocators = NUMA_ALLOCATORS.lock();
    let allocator = allocators.as_mut()?.iter_mut().find(|a| a.node_id() == node_id)?;
    f(allocator)
}

/// Allocate a frame from a specific NUMA node
pub fn numa_allocate_frame(node_id: u32) -> Option<PhysFrame<Size4KiB>> {
    with_numa_allocator(node_id, |a| a.allocate_frame())
}

/// Allocate a frame from the local NUMA node (based on current APIC ID)
pub fn numa_allocate_local() -> Option<PhysFrame<Size4KiB>> {
    // Get current APIC ID
    let apic_id = crate::apic::lapic_id() as u32;
    
    // Find which NUMA node this APIC belongs to
    let topology = NUMA_TOPOLOGY.lock();
    let topology = topology.as_ref()?;
    
    let domain = topology.domain_for_apic(apic_id)?;
    drop(topology);
    
    numa_allocate_frame(domain)
}

/// Allocate contiguous frames from a specific NUMA node
pub fn numa_allocate_contiguous(node_id: u32, count: usize) -> Option<PhysFrame<Size4KiB>> {
    with_numa_allocator(node_id, |a| a.allocate_contiguous(count))
}

/// Allocate 2MB huge page from a specific NUMA node
pub fn numa_allocate_huge_2mb(node_id: u32, count: usize) -> Option<PhysFrame<Size4KiB>> {
    with_numa_allocator(node_id, |a| a.allocate_huge_2mb(count))
}

/// Allocate 1GB huge page from a specific NUMA node
pub fn numa_allocate_huge_1gb(node_id: u32) -> Option<PhysFrame<Size4KiB>> {
    with_numa_allocator(node_id, |a| a.allocate_huge_1gb())
}

/// Deallocate a frame to its NUMA node
pub unsafe fn numa_deallocate_frame(node_id: u32, frame: PhysFrame<Size4KiB>) {
    with_numa_allocator(node_id, |a| {
        a.deallocate_frame(frame);
        Some(())
    });
}

/// Get the NUMA topology map
pub fn numa_topology() -> Option<crate::acpi::NumaTopologyMap> {
    NUMA_TOPOLOGY.lock().as_ref().cloned()
}

/// Get count of NUMA nodes
pub fn numa_node_count() -> usize {
    NUMA_ALLOCATORS.lock().as_ref().map_or(0, |a| a.len())
}

/// Alias for compatibility
pub fn initialized_node_count() -> usize {
    numa_node_count()
}

/// Get all NUMA node IDs
pub fn numa_node_ids() -> Vec<u32> {
    NUMA_ALLOCATORS.lock().as_ref().map_or(Vec::new(), |a| a.iter().map(|n| n.node_id()).collect())
}

/// Get NUMA node for a physical address
pub fn numa_node_for_phys(phys: u64) -> Option<u32> {
    let topology = NUMA_TOPOLOGY.lock();
    topology.as_ref()?.domain_for_phys(phys)
}

/// Get NUMA node for an APIC ID
pub fn numa_node_for_apic(apic_id: u32) -> Option<u32> {
    let topology = NUMA_TOPOLOGY.lock();
    topology.as_ref()?.domain_for_apic(apic_id)
}

/// Print NUMA allocator statistics
pub fn numa_stats() {
    if let Some(allocators) = NUMA_ALLOCATORS.lock().as_ref() {
        for alloc in allocators {
            crate::slog_nano!("NUMA", "stats", 
                "Node {}: usable={}MB allocated={} frames APICs={:?}",
                alloc.node_id(),
                alloc.usable_memory_bytes() / (1024 * 1024),
                alloc.allocated_frame_count(),
                alloc.apic_ids()
            );
        }
    }
}

/// BitmapFrameAllocator for NumaFrameAllocator
mod bitmap_allocator {
    use super::*;
    
    pub const BITMAP_SIZE: usize = 262144; // 256KB covers 8GB physical
    const BITS_PER_BYTE: usize = 8;
    const FRAME_SIZE: u64 = 4096;

    pub struct BitmapFrameAllocator {
        pub bitmap: [u8; BITMAP_SIZE],
        pub next_free_bit: usize,
        pub total_frames: usize,
        pub usable_frames: usize,
        pub allocated_count: usize,
    }

    impl BitmapFrameAllocator {
        pub const fn empty() -> Self {
            BitmapFrameAllocator {
                bitmap: [0xFFu8; BITMAP_SIZE],
                next_free_bit: 0,
                total_frames: 0,
                usable_frames: 0,
                allocated_count: 0,
            }
        }

        pub fn init_from_usable_ranges(&mut self, ranges: &[(u64, u64)]) {
            self.bitmap = [0xFFu8; BITMAP_SIZE];
            let mut last_end: u64 = 0;
            let mut usable_count: usize = 0;

            for &(base, length) in ranges.iter() {
                if length == 0 { continue; }
                let end = base.saturating_add(length);
                let start_frame = base / 4096;
                let end_frame = (end.saturating_sub(1)) / 4096;
                
                for i in start_frame..=end_frame {
                    if (i as usize) < BITMAP_SIZE * 8 {
                        self.clear_bit(i as usize);
                        usable_count += 1;
                    }
                }
                if end > last_end { last_end = end; }
            }

            for i in 2..160 {
                if (i as usize) < BITMAP_SIZE * 8 {
                    self.clear_bit(i as usize);
                    usable_count += 1;
                }
            }

            self.total_frames = core::cmp::min(
                (last_end / 4096) as usize,
                BITMAP_SIZE * 8,
            );
            if self.total_frames == 0 {
                self.total_frames = BITMAP_SIZE * 8;
            }
            self.usable_frames = usable_count;
            self.allocated_count = 0;
            self.next_free_bit = 256;
        }

        #[inline]
        fn clear_bit(&mut self, index: usize) {
            let byte_idx = index / 8;
            let bit_idx = index % 8;
            self.bitmap[byte_idx] &= !(1u8 << bit_idx);
        }

        #[inline]
        fn set_bit(&mut self, index: usize) {
            let byte_idx = index / 8;
            let bit_idx = index % 8;
            self.bitmap[byte_idx] |= 1u8 << bit_idx;
        }

        #[inline]
        fn test_bit(&self, index: usize) -> bool {
            let byte_idx = index / 8;
            let bit_idx = index % 8;
            (self.bitmap[byte_idx] & (1u8 << bit_idx)) != 0
        }

        fn find_free_frame(&self, start_index: usize) -> Option<usize> {
            let mut i = start_index;
            while i < self.total_frames {
                if !self.test_bit(i) { return Some(i); }
                i += 1;
            }
            None
        }

        pub fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
            let idx = self.find_free_frame(self.next_free_bit)?;
            self.set_bit(idx);
            self.next_free_bit = idx + 1;
            self.allocated_count += 1;
            Some(PhysFrame::containing_address(PhysAddr::new(idx as u64 * 4096)))
        }

        pub fn allocate_contiguous(&mut self, count: usize) -> Option<PhysFrame<Size4KiB>> {
            if count == 0 { return None; }
            let mut i = self.next_free_bit;
            while i <= self.total_frames.saturating_sub(count) {
                let mut found = true;
                for j in 0..count {
                    if self.test_bit(i + j) { found = false; i += j + 1; break; }
                }
                if found {
                    for j in 0..count { self.set_bit(i + j); }
                    self.next_free_bit = i + count;
                    return Some(PhysFrame::containing_address(PhysAddr::new(i as u64 * 4096)));
                }
            }
            None
        }

        pub fn allocate_huge_2mb(&mut self, count: usize) -> Option<PhysFrame<Size4KiB>> {
            if count == 0 || count % 512 != 0 { return self.allocate_contiguous(count); }
            for h in 0.. {
                let start_bit = self.next_free_bit + h * 512;
                if start_bit % 512 != 0 { continue; }
                if start_bit + count > self.total_frames { break; }
                let mut ok = true;
                for j in 0..count { if self.test_bit(start_bit + j) { ok = false; break; } }
                if ok {
                    for j in 0..count { self.set_bit(start_bit + j); }
                    self.next_free_bit = start_bit + count;
                    self.allocated_count += count;
                    return Some(PhysFrame::containing_address(PhysAddr::new(start_bit as u64 * 4096)));
                }
            }
            None
        }

        pub fn allocate_huge_1gb(&mut self) -> Option<PhysFrame<Size4KiB>> {
            self.allocate_huge_2mb(262144)
        }

        pub fn usable_memory_bytes(&self) -> u64 {
            self.usable_frames as u64 * 4096
        }

        pub fn allocated_frame_count(&self) -> usize {
            self.allocated_count
        }

        pub fn hardware_context_tensor(&self) -> [f32; 2] {
            let total = core::cmp::max(self.usable_frames, 1);
            [self.allocated_count as f32 / total as f32, self.allocated_count as f32]
        }
    }

    unsafe impl FrameAllocator<Size4KiB> for BitmapFrameAllocator {
        fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
            let idx = self.find_free_frame(self.next_free_bit)?;
            self.set_bit(idx);
            self.next_free_bit = idx + 1;
            self.allocated_count += 1;
            Some(PhysFrame::containing_address(PhysAddr::new(idx as u64 * 4096)))
        }
    }

    impl FrameDeallocator<Size4KiB> for BitmapFrameAllocator {
        unsafe fn deallocate_frame(&mut self, frame: PhysFrame<Size4KiB>) {
            let idx = (frame.start_address().as_u64() / 4096) as usize;
            if idx < self.total_frames {
                self.clear_bit(idx);
                if idx < self.next_free_bit { self.next_free_bit = idx; }
                if self.allocated_count > 0 { self.allocated_count -= 1; }
            }
        }
    }
}

use bitmap_allocator::BitmapFrameAllocator;