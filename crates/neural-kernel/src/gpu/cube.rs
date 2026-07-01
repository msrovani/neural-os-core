//! Desktop Cube — transicao crossfade entre workspaces.
//! Fallback CPU quando GPU blitter nao disponivel.
//! Usa inteiros em vez de float para evitar FPU sem init.

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use crate::display::fb::DoubleBuffer;
use crate::display::theme;

static CUBE_ACTIVE: AtomicBool = AtomicBool::new(false);
static CUBE_PROGRESS: AtomicU32 = AtomicU32::new(0); // 0..50
static CUBE_FROM_WS: AtomicU32 = AtomicU32::new(0);
static CUBE_TO_WS: AtomicU32 = AtomicU32::new(0);
const CUBE_STEPS: u32 = 50;

pub fn start_transition(from: usize, to: usize) {
    CUBE_ACTIVE.store(true, Ordering::SeqCst);
    CUBE_PROGRESS.store(0, Ordering::SeqCst);
    CUBE_FROM_WS.store(from as u32, Ordering::SeqCst);
    CUBE_TO_WS.store(to as u32, Ordering::SeqCst);
}

pub fn is_transitioning() -> bool {
    CUBE_ACTIVE.load(Ordering::SeqCst)
}

/// Renderiza transicao crossfade entre dois workspaces (sem float)
pub fn render_crossfade(fb: &mut DoubleBuffer, _tick: u64) {
    if !CUBE_ACTIVE.load(Ordering::SeqCst) { return; }

    let step = CUBE_PROGRESS.load(Ordering::SeqCst);
    if step >= CUBE_STEPS {
        CUBE_ACTIVE.store(false, Ordering::SeqCst);
        return;
    }
    CUBE_PROGRESS.store(step + 1, Ordering::SeqCst);

    // Crossfade via inteiros: alpha = step*255 / CUBE_STEPS
    let t = theme::current();
    let w = fb.info.width;
    let h = fb.info.height;

    // Linha de separacao animada = split_x = w * step / CUBE_STEPS
    let split_x = (w as u32 * step) / CUBE_STEPS;
    if split_x < w as u32 {
        fb.fill_rect(split_x as usize, 0, w - split_x as usize, h,
            t.accent.0, t.accent.1, t.accent.2);
        fb.fill_rect(0, 0, split_x as usize, h,
            t.bg.0, t.bg.1, t.bg.2);
    }
}
