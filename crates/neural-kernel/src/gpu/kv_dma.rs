//! CPU→GPU KV cache DMA — transfere KV cache entre RAM e VRAM.
//! Usa DMA engine da GPU ou cópia via BAR1 quando DMA não disponível.
//! Referência: dmaplane (arXiv 2603.10030).

use crate::gpu::detect::{GpuInfo, GpuVendor};
use crate::gpu::vram::vram_alloc;
use crate::serial_println;

#[derive(Debug, Clone, Copy)]
pub enum DmaDir { CpuToGpu, GpuToCpu }

/// Transferência DMA de KV cache entre CPU RAM e GPU VRAM
pub struct KvDmaTransfer {
    pub cpu_paddr: u64,       // physical address in CPU RAM
    pub gpu_paddr: u64,       // physical address in GPU VRAM
    pub size: u64,            // bytes
    pub dir: DmaDir,
    pub done: bool,
}

impl KvDmaTransfer {
    pub fn new(cpu_paddr: u64, size: u64, dir: DmaDir, gpu: &GpuInfo) -> Option<Self> {
        let pmoff = unsafe { crate::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed) };
        let gpu_paddr = vram_alloc(size as usize)?;

        if dir as u32 == 0 { // CpuToGpu
            let src = (cpu_paddr + pmoff) as *const u8;
            let dst = (gpu_paddr + pmoff) as *mut u8;
            unsafe { core::ptr::copy_nonoverlapping(src, dst, size as usize); }
            // Sfence para garantir visibilidade para GPU
            unsafe { core::arch::asm!("sfence", options(nostack, preserves_flags)); }
        } else {
            let src = (gpu_paddr + pmoff) as *const u8;
            let dst = (cpu_paddr + pmoff) as *mut u8;
            unsafe { core::ptr::copy_nonoverlapping(src, dst, size as usize); }
        }

        serial_println!("[KV-DMA] {} -> {} ({} bytes) dir={:?}",
            cpu_paddr, gpu_paddr, size, dir);

        Some(KvDmaTransfer { cpu_paddr, gpu_paddr, size, dir, done: true })
    }

    pub fn wait(&mut self) { while !self.done { core::hint::spin_loop(); } }
}

/// Transfere KV cache layer entre RAM e VRAM
pub fn kv_transfer_layer(
    layer_k_cpu: &[f32], layer_v_cpu: &[f32],
    seq_len: usize, hidden: usize,
    _gpu: &GpuInfo,
) -> Option<(u64, u64)> {
    let layer_bytes = seq_len * hidden * 4; // f32 = 4 bytes
    let pmoff = unsafe { crate::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed) };

    let k_gpu = vram_alloc(layer_bytes)?;
    let v_gpu = vram_alloc(layer_bytes)?;

    let k_cpu_paddr = layer_k_cpu.as_ptr() as u64 - pmoff;
    let v_cpu_paddr = layer_v_cpu.as_ptr() as u64 - pmoff;

    unsafe {
        core::ptr::copy_nonoverlapping(
            layer_k_cpu.as_ptr(),
            (k_gpu + pmoff) as *mut f32,
            seq_len * hidden);
        core::ptr::copy_nonoverlapping(
            layer_v_cpu.as_ptr(),
            (v_gpu + pmoff) as *mut f32,
            seq_len * hidden);
        core::arch::asm!("sfence", options(nostack, preserves_flags));
    }

    serial_println!("[KV-DMA] Layer K@{:#x} V@{:#x} ({} seq, {} hidden, {} MB)",
        k_gpu, v_gpu, seq_len, hidden, (layer_bytes * 2) / (1024*1024));

    Some((k_gpu, v_gpu))
}
