//! ADR-0090 Tier 4 - Client-Side Rendering
//!
//! Each app has its own buffer. Compositor blits by clipped region.

use alloc::vec::Vec;
use crate::display::fb::DoubleBuffer;
use crate::display::tiling::WindowId;

pub static CLIENT_RENDERING_ENABLED: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

pub struct AppBuffer {
    pub window_id: WindowId,
    pub width: usize,
    pub height: usize,
    pub bpp: usize,
    pub data: Vec<u8>,
    pub dirty: bool,
    pub screen_x: i32,
    pub screen_y: i32,
}

impl AppBuffer {
    pub fn new(window_id: WindowId, width: usize, height: usize) -> Self {
        let bpp = 4;
        let size = width * height * bpp;
        Self { window_id, width, height, bpp, data: alloc::vec![0u8; size], dirty: true, screen_x: 0, screen_y: 0 }
    }

    pub fn set_pixel(&mut self, x: usize, y: usize, r: u8, g: u8, b: u8) {
        if x >= self.width || y >= self.height { return; }
        let offset = (y * self.width + x) * self.bpp;
        if offset + 3 < self.data.len() {
            self.data[offset] = b;
            self.data[offset + 1] = g;
            self.data[offset + 2] = r;
            self.data[offset + 3] = 0xFF;
        }
        self.dirty = true;
    }

    pub fn fill_rect(&mut self, x: usize, y: usize, w: usize, h: usize, r: u8, g: u8, b: u8) {
        let x2 = (x + w).min(self.width);
        let y2 = (y + h).min(self.height);
        if x >= self.width || y >= self.height { return; }
        let aw = x2 - x;
        let ah = y2 - y;
        if aw == 0 || ah == 0 { return; }
        self.dirty = true;
        let bpp = self.bpp;
        let stride = self.width * bpp;
        let pix = u32::from_le_bytes([b, g, r, 0xFF]);
        for dy in 0..ah {
            let off = (y + dy) * stride + x * bpp;
            for dx in 0..(aw / 4) {
                let idx = off + dx * 4;
                if idx + 4 <= self.data.len() {
                    let slot = &mut self.data[idx..idx+4];
                    slot.copy_from_slice(&pix.to_le_bytes());
                }
            }
        }
    }

    pub fn clear(&mut self, r: u8, g: u8, b: u8) {
        let pix = u32::from_le_bytes([b, g, r, 0xFF]);
        for chunk in self.data.chunks_exact_mut(4) {
            chunk.copy_from_slice(&pix.to_le_bytes());
        }
        self.dirty = true;
    }
}

pub struct AppBufferRegistry {
    buffers: Vec<AppBuffer>,
}

impl AppBufferRegistry {
    pub fn new() -> Self { Self { buffers: Vec::new() } }

    pub fn register(&mut self, window_id: WindowId, width: usize, height: usize) -> usize {
        let idx = self.buffers.len();
        self.buffers.push(AppBuffer::new(window_id, width, height));
        idx
    }

    pub fn get_mut(&mut self, window_id: WindowId) -> Option<&mut AppBuffer> {
        self.buffers.iter_mut().find(|b| b.window_id == window_id)
    }

    pub fn composite(&mut self, fb: &mut DoubleBuffer) {
        for buf in &mut self.buffers {
            if !buf.dirty { continue; }
            let sx = buf.screen_x.max(0) as usize;
            let sy = buf.screen_y.max(0) as usize;
            let dw = buf.width.min(fb.info.width.saturating_sub(sx));
            let dh = buf.height.min(fb.info.height.saturating_sub(sy));
            if dw == 0 || dh == 0 { continue; }
            for dy in 0..dh {
                let src_row = dy * buf.width * buf.bpp;
                let dst_row = (sy + dy) * fb.info.stride + sx * fb.info.bpp;
                if dst_row + dw * fb.info.bpp > fb.back_len() { continue; }
                for dx in 0..dw {
                    let src_off = src_row + dx * buf.bpp;
                    let dst_off = dst_row + dx * fb.info.bpp;
                    if src_off + 3 < buf.data.len() && dst_off + 3 < fb.back_len() {
                        fb.write_back_raw(dst_off, buf.data[src_off], buf.data[src_off+1], buf.data[src_off+2]);
                    }
                }
            }
            buf.dirty = false;
        }
    }

    pub fn count(&self) -> usize { self.buffers.len() }
}

pub static APP_BUFFERS: spin::Mutex<AppBufferRegistry> = spin::Mutex::new(AppBufferRegistry { buffers: Vec::new() });
