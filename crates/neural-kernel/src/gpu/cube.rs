//! Desktop Cube — transicao crossfade entre workspaces.
//! Fallback CPU quando GPU blitter nao disponivel.

use crate::display::fb::DoubleBuffer;
use crate::display::theme;
use crate::display::workspace::LayoutMode;

static mut CUBE_ACTIVE: bool = false;
static mut CUBE_PROGRESS: f32 = 0.0;
static mut CUBE_FROM_WS: usize = 0;
static mut CUBE_TO_WS: usize = 0;

pub fn start_transition(from: usize, to: usize) {
    unsafe {
        CUBE_ACTIVE = true;
        CUBE_PROGRESS = 0.0;
        CUBE_FROM_WS = from;
        CUBE_TO_WS = to;
    }
}

pub fn is_transitioning() -> bool {
    unsafe { CUBE_ACTIVE }
}

/// Renderiza transicao crossfade entre dois workspaces
pub fn render_crossfade(fb: &mut DoubleBuffer, tick: u64) {
    unsafe {
        if !CUBE_ACTIVE { return; }
        CUBE_PROGRESS += 0.02; // 50 ticks para completar (~2.7s)
        if CUBE_PROGRESS >= 1.0 {
            CUBE_ACTIVE = false;
            CUBE_PROGRESS = 1.0;
            return;
        }
        // Crossfade: alpha blend entre workspace atual e proximo
        // Em producao: renderizar os dois workspaces em buffers separados e blend
        let t = theme::current();
        let alpha = CUBE_PROGRESS;
        let inv = 1.0 - alpha;
        let w = fb.info.width;
        let h = fb.info.height;
        // Linha de separacao animada (simula rotacao)
        let split_x = (w as f32 * alpha) as usize;
        if split_x < w {
            // Area direita = workspace destino (cor do tema)
            fb.fill_rect(split_x, 0, w - split_x, h,
                (t.accent.0 as f32 * alpha) as u8,
                (t.accent.1 as f32 * alpha) as u8,
                (t.accent.2 as f32 * alpha) as u8);
            // Area esquerda = workspace origem (cor de fundo)
            fb.fill_rect(0, 0, split_x, h,
                (t.bg.0 as f32 * inv) as u8,
                (t.bg.1 as f32 * inv) as u8,
                (t.bg.2 as f32 * inv) as u8);
        }
    }
}
