//! ADR-0090 Tier 4 - GPU Compositing
//!
//! Uses k_hal GPU BE (blit_2d, fill_rect_2d) for DMA-accelerated compositing.

use crate::display::gpu_backend;
use crate::display::fb::DoubleBuffer;
use k_hal::gpu::blit::{blit_2d, fill_rect_2d};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositeMode {
    CpuOnly,
    GpuAccelerated,
}

pub struct GpuCompositor {
    mode: CompositeMode,
    pub blit_count: u64,
    pub fill_count: u64,
    pub cpu_fallback_count: u64,
}

impl GpuCompositor {
    pub fn new() -> Self {
        let mode = if gpu_backend::is_blit_ready() {
            CompositeMode::GpuAccelerated
        } else {
            CompositeMode::CpuOnly
        };
        Self { mode, blit_count: 0, fill_count: 0, cpu_fallback_count: 0 }
    }

    pub fn refresh_mode(&mut self) {
        self.mode = if gpu_backend::is_blit_ready() {
            CompositeMode::GpuAccelerated
        } else {
            CompositeMode::CpuOnly
        };
    }

    pub fn fill_rect(&mut self, fb: &mut DoubleBuffer, x: usize, y: usize, w: usize, h: usize, r: u8, g: u8, b: u8) {
        if self.mode == CompositeMode::GpuAccelerated {
            let color = if fb.info.rgb_order {
                u32::from_le_bytes([r, g, b, 0xFF])
            } else {
                u32::from_le_bytes([b, g, r, 0xFF])
            };
            let dst_pa = fb.phys_addr_for(x, y);
            if let Some(pa) = dst_pa {
                if fill_rect_2d(pa, w as u32, h as u32, fb.info.bpp as u32, color) {
                    self.fill_count += 1;
                    fb.mark_dirty();
                    return;
                }
            }
            self.cpu_fallback_count += 1;
        }
        fb.fill_rect_fast(x, y, w, h, r, g, b);
        self.cpu_fallback_count += 1;
    }

    pub fn blit_region(&mut self, fb: &mut DoubleBuffer, src: &[u8], src_w: usize, src_h: usize, src_bpp: usize, dst_x: usize, dst_y: usize) {
        if self.mode == CompositeMode::GpuAccelerated {
            let src_pa = src.as_ptr() as u64;
            let dst_pa = fb.phys_addr_for(dst_x, dst_y);
            if let (Some(spa), Some(dpa)) = (Some(src_pa), dst_pa) {
                if blit_2d(spa, dpa, src_w as u32, src_h as u32, src_bpp as u32) {
                    self.blit_count += 1;
                    fb.mark_dirty();
                    return;
                }
            }
            self.cpu_fallback_count += 1;
        }
        // CPU fallback
        for dy in 0..src_h {
            for dx in 0..src_w {
                let src_off = (dy * src_w + dx) * src_bpp;
                if src_off + 2 < src.len() {
                    fb.set_pixel(dst_x + dx, dst_y + dy, src[src_off+2], src[src_off+1], src[src_off]);
                }
            }
        }
        self.cpu_fallback_count += 1;
    }

    pub fn mode(&self) -> CompositeMode { self.mode }
    pub fn is_gpu(&self) -> bool { self.mode == CompositeMode::GpuAccelerated }
}

pub static GPU_COMPOSITOR: spin::Mutex<GpuCompositor> = spin::Mutex::new(GpuCompositor {
    mode: CompositeMode::CpuOnly, blit_count: 0, fill_count: 0, cpu_fallback_count: 0,
});
