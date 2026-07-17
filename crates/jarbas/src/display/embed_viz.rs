//! H2 Embedding-space desktop PoC — project latent 256D → 2D points on FB.
//! H5 Thought canvas — short-lived Gaussian-ish splats while thinking.

use event_bus::latent::{F16Bits, LATENT_DIM};
use crate::display::fb::DoubleBuffer;

/// Project halves of latent to screen coords.
pub fn latent_to_xy(vec: &[F16Bits; LATENT_DIM], w: usize, h: usize) -> (usize, usize) {
    let mut sx = 0u32;
    let mut sy = 0u32;
    for i in 0..128 {
        sx = sx.wrapping_add(vec[i] as u32);
        sy = sy.wrapping_add(vec[128 + i] as u32);
    }
    let x = (sx as usize % w.max(1)).min(w.saturating_sub(1));
    let y = (sy as usize % h.max(1)).min(h.saturating_sub(1));
    (x, y)
}

fn unpack_rgb(color: u32) -> (u8, u8, u8) {
    (
        ((color >> 16) & 0xFF) as u8,
        ((color >> 8) & 0xFF) as u8,
        (color & 0xFF) as u8,
    )
}

/// Draw a small cross / cluster at (x,y) — embedding point.
pub fn draw_embed_point(fb: &mut DoubleBuffer, x: usize, y: usize, color: u32) {
    let (r, g, b) = unpack_rgb(color);
    for dy in 0..3u32 {
        for dx in 0..3u32 {
            let px = x.saturating_add(dx as usize).saturating_sub(1);
            let py = y.saturating_add(dy as usize).saturating_sub(1);
            if px < fb.info.width && py < fb.info.height {
                fb.set_pixel(px, py, r, g, b);
            }
        }
    }
}

/// H5: draw soft splat (diamond falloff) centered at (cx,cy).
pub fn draw_thought_splat(fb: &mut DoubleBuffer, cx: usize, cy: usize, radius: usize, color: u32) {
    let (r, g, b) = unpack_rgb(color);
    let rad = radius.max(2);
    let r2 = (rad * rad) as i32;
    for oy in 0..(2 * rad + 1) {
        for ox in 0..(2 * rad + 1) {
            let dx = ox as i32 - rad as i32;
            let dy = oy as i32 - rad as i32;
            if dx * dx + dy * dy > r2 {
                continue;
            }
            let px = cx as isize + dx as isize;
            let py = cy as isize + dy as isize;
            if px >= 0 && py >= 0 {
                let px = px as usize;
                let py = py as usize;
                if px < fb.info.width && py < fb.info.height {
                    fb.set_pixel(px, py, r, g, b);
                }
            }
        }
    }
}

static H2_OK: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
static H5_OK: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

pub fn mark_h2() {
    H2_OK.store(true, core::sync::atomic::Ordering::Relaxed);
}
pub fn mark_h5() {
    H5_OK.store(true, core::sync::atomic::Ordering::Relaxed);
}

pub fn gate_status() -> (&'static str, &'static str) {
    (
        if H2_OK.load(core::sync::atomic::Ordering::Relaxed) {
            "OK"
        } else {
            "ABSENT"
        },
        if H5_OK.load(core::sync::atomic::Ordering::Relaxed) {
            "OK"
        } else {
            "ABSENT"
        },
    )
}
