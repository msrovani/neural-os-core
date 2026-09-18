//! CPU↔GPU KV cache transfer via aperture BAR (memcpy UC), não CE/DMA real.
//! Referência aspiracional: dmaplane (arXiv 2603.10030) — residual Layer S.
//!
//! Honesty: VRAM phys só é tocável após `init_vram_tier` mapear UC
//! (`phys + pmoff`). Sem VRAM_READY → recusa. `wait()` nunca gira eterno.

use crate::gpu::detect::GpuInfo;
use crate::gpu::vram::{vram_alloc, VRAM_READY};
use core::sync::atomic::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmaDir {
    CpuToGpu,
    GpuToCpu,
}

/// Transferência de KV cache entre CPU RAM e GPU VRAM (BAR memcpy).
pub struct KvDmaTransfer {
    pub cpu_paddr: u64,
    pub gpu_paddr: u64,
    pub size: u64,
    pub dir: DmaDir,
    pub done: bool,
}

impl KvDmaTransfer {
    pub fn new(cpu_vaddr: u64, size: u64, dir: DmaDir, _gpu: &GpuInfo) -> Option<Self> {
        if !VRAM_READY.load(Ordering::Acquire) || size == 0 {
            k_nano::slog_hal!("GPU", "kvdma", "refuse — VRAM não ready ou size=0");
            return None;
        }
        let gpu_paddr = vram_alloc(size as usize)?;
        let pmoff = k_nano::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        // Aperture já mapeada UC em init_vram_tier: VA = phys + pmoff.
        let bar_va = (gpu_paddr.wrapping_add(pmoff)) as *mut u8;

        match dir {
            DmaDir::CpuToGpu => {
                let src = cpu_vaddr as *const u8;
                unsafe {
                    core::ptr::copy_nonoverlapping(src, bar_va, size as usize);
                    core::arch::asm!("sfence", options(nostack, preserves_flags));
                }
            }
            DmaDir::GpuToCpu => {
                let dst = cpu_vaddr as *mut u8;
                unsafe {
                    core::ptr::copy_nonoverlapping(bar_va as *const u8, dst, size as usize);
                    core::arch::asm!("lfence", options(nostack, preserves_flags));
                }
            }
        }

        k_nano::slog_hal!(
            "GPU",
            "kvdma",
            "cpu_vaddr={:#x} gpu_paddr={:#x} ({} bytes) dir={:?} path=bar_memcpy",
            cpu_vaddr,
            gpu_paddr,
            size,
            dir
        );

        Some(KvDmaTransfer {
            cpu_paddr: cpu_vaddr,
            gpu_paddr,
            size,
            dir,
            done: true,
        })
    }

    /// Síncrono no `new` (memcpy). Nunca spin infinito.
    pub fn wait(&mut self) -> bool {
        self.done
    }
}

/// Transfere KV cache layer entre RAM e VRAM (BAR memcpy; exige VRAM_READY).
pub fn kv_transfer_layer(
    layer_k_cpu: &[f32],
    layer_v_cpu: &[f32],
    seq_len: usize,
    hidden: usize,
    _gpu: &GpuInfo,
) -> Option<(u64, u64)> {
    if !VRAM_READY.load(Ordering::Acquire) {
        return None;
    }
    let layer_bytes = seq_len.checked_mul(hidden)?.checked_mul(4)?;
    let pmoff = k_nano::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);

    let k_gpu = vram_alloc(layer_bytes)?;
    let v_gpu = vram_alloc(layer_bytes)?;

    unsafe {
        core::ptr::copy_nonoverlapping(
            layer_k_cpu.as_ptr(),
            (k_gpu + pmoff) as *mut f32,
            seq_len * hidden,
        );
        core::ptr::copy_nonoverlapping(
            layer_v_cpu.as_ptr(),
            (v_gpu + pmoff) as *mut f32,
            seq_len * hidden,
        );
        core::arch::asm!("sfence", options(nostack, preserves_flags));
    }

    k_nano::slog_hal!(
        "GPU",
        "kvdma",
        "Layer K@{:#x} V@{:#x} ({} seq, {} hidden, {} MB) path=bar_memcpy",
        k_gpu,
        v_gpu,
        seq_len,
        hidden,
        (layer_bytes * 2) / (1024 * 1024)
    );

    Some((k_gpu, v_gpu))
}
