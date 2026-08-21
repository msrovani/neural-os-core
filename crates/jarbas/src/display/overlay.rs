//! Snapshots de overlay — pintados só em `JarbasDesktop::render()` (SESSION_261).
//! Tick drena EventBus e grava aqui; NUNCA desenha no framebuffer.

use alloc::string::String;
use alloc::vec::Vec;
use k_nano::sync::IrqSafeLock;
use super::tiling::Rect;

const MAX_EMBEDS: usize = 32;
const MAX_RENDERS: usize = 4;

/// Ponto H2/H5 (espaço latente projetado).
#[derive(Clone, Copy)]
pub struct EmbedMark {
    pub x: usize,
    pub y: usize,
    pub color: u32,
    pub splat: bool,
}

/// Pedido RENDER_WINDOW retido até o `render()` executar a skill.
#[derive(Clone)]
pub struct RenderOverlay {
    pub name: String,
    pub data: alloc::vec::Vec<u8>,
    pub rect: Rect,
}

pub static EMBED_MARKS: IrqSafeLock<Vec<EmbedMark>> =
    IrqSafeLock::new(Vec::new());
pub static RENDER_OVERLAYS: IrqSafeLock<Vec<RenderOverlay>> =
    IrqSafeLock::new(Vec::new());

/// Acrescenta um ponto; descarta o mais antigo se estourar o teto.
pub fn push_embed(mark: EmbedMark) {
    let mut g = EMBED_MARKS.lock();
    if g.len() >= MAX_EMBEDS {
        g.remove(0);
    }
    g.push(mark);
}

/// Substitui overlay da skill `name` (um buffer por renderer).
pub fn set_render_overlay(name: &str, data: &[u8], rect: Rect) {
    let mut g = RENDER_OVERLAYS.lock();
    if let Some(ex) = g.iter_mut().find(|o| o.name == name) {
        ex.data = data.to_vec();
        ex.rect = rect;
        return;
    }
    if g.len() >= MAX_RENDERS {
        g.remove(0);
    }
    g.push(RenderOverlay {
        name: String::from(name),
        data: data.to_vec(),
        rect,
    });
}
