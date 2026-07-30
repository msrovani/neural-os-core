//! SlabBuddy — allocator adaptativo: slab para alocações pequenas (32-4096)
//! com fallback para o linked_list_allocator global.
//!
//! Integração: MemoryAgent usa SlabBuddy como pré-alocador para objetos
//! pequenos e frequentes. O `status()` é exibido no SysInfo card.

use alloc::string::String;

/// Slab + Buddy allocator combinados.
pub struct SlabBuddy {
    slab: crate::slab::SlabAllocator,
    heap_allocated: u64,
    slab_hits: u64,
    slab_misses: u64,
}

impl SlabBuddy {
    pub fn new() -> Self {
        SlabBuddy {
            slab: crate::slab::SlabAllocator::empty(),
            heap_allocated: 0,
            slab_hits: 0,
            slab_misses: 0,
        }
    }

    /// Tenta slab primeiro; se falhar, retorna null (caller usa global).
    pub fn alloc(&mut self, size: usize) -> *mut u8 {
        if size <= 4096 && size >= 32 {
            let ptr = unsafe { self.slab.slab_alloc(size) };
            if !ptr.is_null() {
                self.slab_hits += 1;
                return ptr;
            }
            self.slab_misses += 1;
        }
        self.heap_allocated += size as u64;
        core::ptr::null_mut()
    }

    pub fn status(&self) -> String {
        let rate = if self.slab_hits + self.slab_misses > 0 {
            self.slab_hits as f64 * 100.0 / (self.slab_hits + self.slab_misses) as f64
        } else {
            100.0
        };
        alloc::format!(
            "[ALLOC] Slab: {} hits, {} misses ({:.0}%) | Heap: {} KB",
            self.slab_hits,
            self.slab_misses,
            rate,
            self.heap_allocated / 1024
        )
    }
}

pub fn init() {
    crate::slog_nano!("ALLOC", "info", "SlabBuddy allocator ativo (slab 32-4096, fallback heap)");
}
