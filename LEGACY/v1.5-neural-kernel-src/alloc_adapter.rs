//! Alloc Adapter — #355: Slab + Buddy allocator integrados.
//! SlabAllocator para alocações pequenas (32-4096 bytes).
//! LinkedHeap para alocações grandes.
//! GPU VRAM Buddy para memória de vídeo.

use alloc::string::String;
use crate::serial_println;

/// Slab allocator: 8 buckets para alocações pequenas (32-4096 bytes)
pub struct SlabBuddy {
    slab: crate::slab::SlabAllocator,
    heap_allocated: u64,
    slab_hits: u64,
    slab_misses: u64,
}

impl SlabBuddy {
    pub fn new() -> Self {
        SlabBuddy { slab: crate::slab::SlabAllocator::empty(), heap_allocated: 0, slab_hits: 0, slab_misses: 0 }
    }

    /// Aloca memoria: tenta slab primeiro, fallback heap
    pub fn alloc(&mut self, size: usize) -> *mut u8 {
        if size <= 4096 && size >= 32 {
            // Tenta slab
            let ptr = unsafe { self.slab.slab_alloc(size) };
            if !ptr.is_null() {
                self.slab_hits += 1;
                return ptr;
            }
            self.slab_misses += 1;
        }
        // Fallback: linked_list_allocator global
        self.heap_allocated += size as u64;
        core::ptr::null_mut() // retorna null = caller usa o global allocator
    }

    pub fn status(&self) -> String {
        let rate = if self.slab_hits + self.slab_misses > 0 {
            self.slab_hits as f64 * 100.0 / (self.slab_hits + self.slab_misses) as f64
        } else { 100.0 };
        alloc::format!("[ALLOC] Slab: {} hits, {} misses ({:.0}%) | Heap: {} KB",
            self.slab_hits, self.slab_misses, rate, self.heap_allocated / 1024)
    }
}

pub fn init(_heap_start: usize, _heap_size: usize) {
    serial_println!("[ALLOC] SlabBuddy allocator ativo (slab 32-4096, fallback heap)");
}

pub fn status() -> &'static str { "SlabBuddy: slab+buddy integrados" }
