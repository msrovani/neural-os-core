//! IconCache — gera icones 16×16 a partir de hints textuais.
//! Fallback geometrico por AgentKind quando LLM nao disponivel.

use alloc::collections::BTreeMap;
use alloc::string::String;
use spin::Mutex;

pub type Icon = [u8; 64]; // 16×16 com 4 paletas (2 bits por pixel)

static ICON_CACHE: Mutex<BTreeMap<&'static str, Icon>> = Mutex::new(BTreeMap::new());

/// Gera um icone 16×16 baseado no hint e AgentKind
pub fn get_icon(name: &str, hint: &str) -> Icon {
    if let Some(icon) = ICON_CACHE.lock().get(name) {
        return *icon;
    }
    let icon = generate_geometric(hint);
    ICON_CACHE.lock().insert(alloc::string::String::leak(String::from(name)), icon);
    icon
}

fn generate_geometric(hint: &str) -> Icon {
    let mut icon = [0u8; 64];

    // Deterministic color based on hint hash
    let color = hint.as_bytes().iter().fold(0u8, |a, b| a.wrapping_add(*b));
    let _palette = [
        (10u8, 10u8, 20u8),        // 00: background (theme bg)
        (color, 128, 255 - color),   // 01: primary
        (color / 2, 200, 128),       // 10: secondary
        (255, 255, 255),             // 11: highlight
    ];

    let pattern = hint.as_bytes();
    for y in 0..16usize {
        for x in 0..16usize {
            let idx = (y * 16 + x) as usize;
            let val = match pattern.get(idx % pattern.len().max(1)) {
                Some(b) if *b > 192 => 3u8, // highlight
                Some(b) if *b > 128 => 1u8, // primary
                Some(b) if *b > 64 => 2u8,  // secondary
                _ => 0u8,                    // bg
            };
            let byte_idx = (y * 16 + x) / 4;
            let bit_shift = (6 - (x % 4) * 2) as u32;
            icon[byte_idx] |= val << bit_shift;
        }
    }
    icon
}

/// Renderiza o icone (2 bits por pixel → 4 paletas)
pub fn render_icon(fb: &mut crate::display::fb::DoubleBuffer, x: usize, y: usize, icon: &Icon) {
    let palette = [(10,10,20), (80,130,220), (0,200,200), (255,255,255)];
    for row in 0..16usize {
        for col in 0..16usize {
            let byte_idx = (row * 16 + col) / 4;
            let shift = (6 - (col % 4) * 2) as u32;
            let val = (icon[byte_idx] >> shift) & 0x03;
            let (r, g, b) = palette[val as usize];
            fb.set_pixel(x + col, y + row, r, g, b);
        }
    }
}
