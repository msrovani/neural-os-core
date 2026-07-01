//! Auto-tiling layout engine — estilo COSMIC.
//! Organiza janelas em grid/tile automaticamente.

use alloc::vec::Vec;
use crate::display::compositor::Compositor;
use crate::display::workspace::LayoutMode;

/// Aplica layout automatico nas janelas do workspace atual
pub fn apply_layout(comp: &mut Compositor, mode: LayoutMode, screen_w: u32, screen_h: u32) {
    let dock_h = 36;
    let _title_h = 20;
    let avail_w = screen_w as i32;
    let avail_h = (screen_h - dock_h) as i32;

    let windows: Vec<u32> = comp.windows.iter().map(|w| w.id).collect();
    let count = windows.len();
    if count == 0 { return; }

    match mode {
        LayoutMode::Tiled => {
            let fcount = count as f32;
            let cols = unsafe { libm::ceilf(libm::sqrtf(fcount)) } as i32;
            let rows = unsafe { libm::ceilf(fcount / cols as f32) } as i32;
            let cell_w = avail_w / cols;
            let cell_h = avail_h / rows;
            let mut idx = 0;
            for row in 0..rows {
                for col in 0..cols {
                    if idx >= count { break; }
                    if let Some(win) = comp.windows.iter_mut().find(|w| w.id == windows[idx]) {
                        win.x = col * cell_w;
                        win.y = row * cell_h;
                        win.w = cell_w as u32;
                        win.h = cell_h as u32;
                    }
                    idx += 1;
                }
            }
        }
        LayoutMode::Grid => {
            let cols = 2i32.max(count as i32);
            let rows = unsafe { libm::ceilf((count as f32) / cols as f32) } as i32;
            let cell_w = avail_w / cols;
            let cell_h = avail_h / rows;
            for (i, &wid) in windows.iter().enumerate() {
                if let Some(win) = comp.windows.iter_mut().find(|w| w.id == wid) {
                    let col = i as i32 % cols;
                    let row = i as i32 / cols;
                    win.x = col * cell_w;
                    win.y = row * cell_h;
                    win.w = cell_w as u32;
                    win.h = cell_h as u32;
                }
            }
        }
        LayoutMode::Maximized => {
            if let Some(win) = comp.windows.iter_mut().find(|w| w.id == windows[0]) {
                win.x = 0;
                win.y = 0;
                win.w = avail_w as u32;
                win.h = avail_h as u32;
            }
        }
        LayoutMode::Floating => {
            // Mantem posicoes atuais (sem alteracao)
        }
    }
}
