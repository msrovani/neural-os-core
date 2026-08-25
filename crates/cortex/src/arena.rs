//! Tier 2 — Bump/Arena allocator para tensores Trinity e cache R3 (MoE).
//! Ciclo de vida: nasce no rollout → consumido no replay RL → reset_epoch() O(1).
//! NÃO usa #[global_allocator] — passado explicitamente como &mut TensorArena.

use core::sync::atomic::{AtomicUsize, Ordering};
use x86_64::structures::paging::{FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB};
use x86_64::VirtAddr;

/// Região virtual isolada do heap Hermes/JARBAS (Tier 1).
pub const CORTEX_ARENA_VIRT: usize = 0x4800_0000_0000;
/// Maximum arena size: 4 GB (suporta Falcon3-10B 2.5 GB).
pub const CORTEX_ARENA_MAX_SIZE: usize = 4 * 1024 * 1024 * 1024;
/// Fallback default: 2 GB (se RAM não detectada).
pub const CORTEX_ARENA_DEFAULT_SIZE: usize = 2 * 1024 * 1024 * 1024;

static ARENA_SIZE_BYTES: AtomicUsize = AtomicUsize::new(CORTEX_ARENA_DEFAULT_SIZE);

/// Auto-size arena: 12.5% da RAM, clamp [64MB, 256MB] no T+0.
/// 50%/4GB mapeava ~1M páginas no boot (freeze i5/240H) e comia frames do modelo.
pub fn auto_arena_size() -> usize {
    let ram_mb = k_nano::memory::TOTAL_RAM_MB.load(core::sync::atomic::Ordering::Relaxed) as usize;
    if ram_mb == 0 {
        return 256 * 1024 * 1024;
    }
    let size = (ram_mb / 8) * 1024 * 1024;
    let cap = if ram_mb as u64 >= k_nano::memory::RAM_FULL_PACK_MB {
        1024 * 1024 * 1024
    } else {
        256 * 1024 * 1024
    };
    size.clamp(64 * 1024 * 1024, cap)
}

/// Bump allocator O(1) alloc / O(1) reset — zero fragmentação.
pub struct TensorArena {
    start: usize,
    end: usize,
    current: usize,
}

impl TensorArena {
    pub fn from_region(start: usize, size: usize) -> Self {
        TensorArena {
            start,
            end: start.saturating_add(size),
            current: start,
        }
    }

    pub fn is_initialized(&self) -> bool {
        self.end > self.start
    }

    pub fn virt_base(&self) -> usize {
        self.start
    }

    /// Alocação bruta alinhada — retorna endereço virtual.
    pub fn alloc_bytes(&mut self, size: usize, align: usize) -> Option<usize> {
        if size == 0 {
            return Some(self.current);
        }
        let align = align.max(1);
        let mask = align - 1;
        let alloc_start = (self.current + mask) & !mask;
        if alloc_start.saturating_add(size) > self.end {
            return None;
        }
        self.current = alloc_start + size;
        Some(alloc_start)
    }

    /// Aloca slice tipado na arena — sem Box/Vec global.
    pub fn alloc_slice<T>(&mut self, count: usize) -> Option<&mut [T]> {
        if count == 0 {
            return Some(&mut []);
        }
        let size = count.checked_mul(core::mem::size_of::<T>())?;
        let addr = self.alloc_bytes(size, core::mem::align_of::<T>())?;
        Some(unsafe { core::slice::from_raw_parts_mut(addr as *mut T, count) })
    }

    /// Reset O(1) após ciclo PPO/GRPO — alias R3.
    pub fn reset_epoch(&mut self) {
        self.current = self.start;
    }

    pub fn reset_moe_cache(&mut self) {
        self.reset_epoch();
    }

    pub fn used_bytes(&self) -> usize {
        self.current.saturating_sub(self.start)
    }

    pub fn capacity_bytes(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub fn free_bytes(&self) -> usize {
        self.end.saturating_sub(self.current)
    }
}

/// Mapeia páginas físicas contíguas para a região Tier 2 via frame allocator do boot.
pub fn init_arena_region(
    mapper: &mut impl Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
    virt_start: usize,
    size: usize,
) -> Result<TensorArena, &'static str> {
    if size == 0 {
        return Err("arena size zero");
    }
    let heap_start = VirtAddr::new(virt_start as u64);
    let heap_end = heap_start + size as u64 - 1u64;
    let page_range = Page::range_inclusive(
        Page::containing_address(heap_start),
        Page::containing_address(heap_end),
    );
    for page in page_range {
        let frame = frame_allocator
            .allocate_frame()
            .ok_or("cortex arena: out of frames")?;
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        unsafe {
            mapper
                .map_to(page, frame, flags, frame_allocator)
                .map_err(|_| "cortex arena: map failed")?
                .flush();
        }
    }
    ARENA_SIZE_BYTES.store(size, Ordering::SeqCst);
    k_nano::slog_cortex!("CORTEX", "ARENA", "Tier 2 mapped: virt={:#x} size={} MB",
        virt_start,
        size / (1024 * 1024));
    // MHI índice: VA ≠ PhysAddr HHDM. kind=VirtMapped — tick não memcpy.
    k_nano::mhi::register_virt_region(virt_start as u64, size, "tensor_arena");
    Ok(TensorArena::from_region(virt_start, size))
}

pub fn configured_arena_size() -> usize {
    ARENA_SIZE_BYTES.load(Ordering::SeqCst)
}
