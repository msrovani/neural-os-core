//! TrueType Font Engine — Fontes pré-rasterizadas via Python converter.
//! As fontes são convertidas com tools/convert_ttf_to_bitmap.py em tempo de
//! build e embutidas como arrays Rust. Zero parsing TTF em runtime.
//!
//! Uso: python tools/convert_ttf_to_bitmap.py DejaVuSans.ttf --size 16
//! Depois: include_bytes!("../../target/dejavu.rs") no kernel.

use alloc::vec::Vec;
use alloc::vec;
use alloc::string::String;
use alloc::collections::BTreeMap;

use crate::display::fb::DoubleBuffer;
fn read_u32_le(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_i32_le(data: &[u8], offset: usize) -> Option<i32> {
    read_u32_le(data, offset).map(|v| v as i32)
}

fn read_u16_le(data: &[u8], offset: usize) -> Option<u16> {
    let bytes = data.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

/// Um glyph rasterizado: bitmap em grayscale + métricas
pub struct RasterGlyph {
    pub w: u16, pub h: u16,
    pub x: i16, pub y: i16,
    pub pixels: Vec<u8>,
}

/// Fonte TTF pré-rasterizada
pub struct TtfRasterFont {
    pub name: String,
    pub size: u16,
    glyphs: BTreeMap<char, RasterGlyph>,
}

impl TtfRasterFont {
    pub fn new(name: &str, size: u16) -> Self {
        TtfRasterFont { name: String::from(name), size, glyphs: BTreeMap::new() }
    }

    pub fn add_glyph(&mut self, c: char, w: u16, h: u16, x: i16, y: i16, pixels: &[u8]) {
        self.glyphs.insert(c, RasterGlyph { w, h, x, y, pixels: pixels.to_vec() });
    }

    pub fn has_glyph(&self, c: char) -> bool { self.glyphs.contains_key(&c) }
    pub fn glyph_count(&self) -> usize { self.glyphs.len() }

    /// Carrega de um array flat (formato do conversor Python)
    pub fn load_from_bytes(&mut self, data: &[u8]) {
        if data.len() < 8 { return; }
        let Some(count) = read_u32_le(data, 0).map(|v| v as usize) else { return; };
        let _size = read_u16_le(data, 4).unwrap_or(self.size);
        let mut offset = 8;
        for _ in 0..count {
            if offset + 20 > data.len() { break; }
            let Some(codepoint) = read_u32_le(data, offset) else { break; };
            let Some(w_raw) = read_u32_le(data, offset + 4) else { break; };
            let Some(h_raw) = read_u32_le(data, offset + 8) else { break; };
            let Some(x_raw) = read_i32_le(data, offset + 12) else { break; };
            let Some(y_raw) = read_i32_le(data, offset + 16) else { break; };
            let w = w_raw.min(u16::MAX as u32) as u16;
            let h = h_raw.min(u16::MAX as u32) as u16;
            let x = x_raw.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            let y = y_raw.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            offset += 20;
            let Some(pixels_size) = (w as usize).checked_mul(h as usize) else { break; };
            let Some(end) = offset.checked_add(pixels_size) else { break; };
            if end > data.len() { break; }
            let c = char::from_u32(codepoint).unwrap_or('\0');
            self.add_glyph(c, w, h, x, y, &data[offset..end]);
            offset += pixels_size;
        }
        k_nano::slog_jarbas!("TTF", "info", "'{}' carregada: {} glyphs @ {}px", self.name, self.glyph_count(), self.size);
    }

    /// Desenha texto no framebuffer
    pub fn draw_text(&self, fb: &mut DoubleBuffer, mut x: usize, y: usize, text: &str, r: u8, g: u8, b: u8, scr_w: usize) {
        for c in text.chars() {
            if x + self.size as usize > scr_w { break; }
            if let Some(glyph) = self.glyphs.get(&c) {
                for dy in 0..glyph.h as usize {
                    for dx in 0..glyph.w as usize {
                        let alpha = glyph.pixels[dy * glyph.w as usize + dx];
                        if alpha > 0 {
                            let rx = (x as i32 + dx as i32 + glyph.x as i32) as usize;
                            let ry = (y as i32 + dy as i32 + glyph.y as i32) as usize;
                            if rx < fb.info.width && ry < fb.info.height {
                                let blend = |bg: u8, fg: u8, a: u8| -> u8 {
                                    ((bg as u32 * (255 - a as u32) + fg as u32 * a as u32) / 255) as u8
                                };
                                let (br, bg, bb) = (10u8, 10u8, 15u8); // background color
                                fb.set_pixel(rx, ry, blend(br, r, alpha), blend(bg, g, alpha), blend(bb, b, alpha));
                            }
                        }
                    }
                }
                x += glyph.w as usize + 2;
            } else {
                // Fallback: VGA bitmap
                if let Some(bitmap) = crate::display::font::get_char_bitmap(c) {
                    for dy in 0..16 {
                        let row = bitmap[dy];
                        for dx in 0..8 {
                            if (row >> (7 - dx)) & 1 == 1 { fb.set_pixel(x + dx, y + dy + 4, r, g, b); }
                        }
                    }
                    x += 10;
                }
            }
        }
    }
}

/// Gerenciador de fontes
pub struct FontManager {
    pub fonts: BTreeMap<String, TtfRasterFont>,
    pub active: String,
    pub use_ttf: bool,
}

impl FontManager {
    pub fn new() -> Self {
        let mut fm = FontManager { fonts: BTreeMap::new(), active: String::from("vga"), use_ttf: false };
        // Auto-register embedded Latin-1 font for PT-BR accent support
        fm.register_latin1_default();
        fm.activate("latin1"); // TTF ativado por padrão — acentuação PT-BR funciona
        fm
    }

    /// Register embedded Latin-1 raster font covering ASCII 32-255 (incl. à,á,â,ã,é,ê,í,ó,ô,õ,ú,ç)
    fn register_latin1_default(&mut self) {
        let mut font = TtfRasterFont::new("latin1", 16);
        // Build glyphs for Latin-1 range (32-255) using VGA bitmap data + extended chars
        // ponytail: Latin-1 extended chars use decomposed diacritics via bitmap composition
        for code in 32u8..=255u8 {
            let c = code as char;
            let w: u16 = 8;
            let h: u16 = 16;
            let mut pixels = alloc::vec![0u8; (w as usize) * (h as usize)];
            // Base glyph from VGA font data (ASCII range)
            if code >= 32 && code <= 126 {
                if let Some(base) = crate::display::font::get_char_bitmap(c) {
                    for dy in 0..16 {
                        let row = base[dy];
                        for dx in 0..8 {
                            if (row >> (7 - dx)) & 1 == 1 {
                                pixels[dy * (w as usize) + dx] = 255;
                            }
                        }
                    }
                }
            } else {
                // Extended Latin-1: compose from base ASCII + accent marks
                let base_char = match c {
                    'à' | 'á' | 'â' | 'ã' => 'a',
                    'é' | 'ê' => 'e',
                    'í' => 'i',
                    'ó' | 'ô' | 'õ' => 'o',
                    'ú' => 'u',
                    'ç' => 'c',
                    'À' | 'Á' | 'Â' | 'Ã' => 'A',
                    'É' | 'Ê' => 'E',
                    'Í' => 'I',
                    'Ó' | 'Ô' | 'Õ' => 'O',
                    'Ú' => 'U',
                    'Ç' => 'C',
                    _ => continue,
                };
                if let Some(base) = crate::display::font::get_char_bitmap(base_char) {
                    for dy in 0..16 {
                        let row = base[dy];
                        for dx in 0..8 {
                            if (row >> (7 - dx)) & 1 == 1 {
                                pixels[dy * (w as usize) + dx] = 255;
                            }
                        }
                    }
                    // Add accent mark on row 1-2 (´: dx=4, `: dx=2, ^: dx=3, ~: dx=3, ¸: dy=14)
                    let accent_bitmap: [u8; 2] = match c {
                        'á' | 'Á' | 'é' | 'É' | 'í' | 'Í' | 'ó' | 'Ó' | 'ú' | 'Ú' => [0b00010000, 0b00101000], // acute
                        'à' | 'À' => [0b00101000, 0b00010000], // grave
                        'â' | 'Â' | 'ê' | 'Ê' | 'ô' | 'Ô' => [0b00010000, 0b00111000], // circumflex
                        'ã' | 'Ã' | 'õ' | 'Õ' => [0b00000000, 0b00101000], // tilde
                        'ç' | 'Ç' => [0b00000000, 0b10000000], // cedilla at row 14
                        _ => continue,
                    };
                    if matches!(c, 'ç' | 'Ç') {
                        // Cedilla on row 14
                        for dx in 0..8 {
                            if (accent_bitmap[0] >> (7 - dx)) & 1 == 1 {
                                pixels[14 * (w as usize) + dx] = 255;
                            }
                        }
                    } else {
                        // Accent on row 1-2
                        for dy in 0..2 {
                            for dx in 0..8 {
                                if (accent_bitmap[dy] >> (7 - dx)) & 1 == 1 {
                                    pixels[(dy + 1) * (w as usize) + dx] = 255;
                                }
                            }
                        }
                    }
                }
            }
            font.glyphs.insert(c, RasterGlyph { w, h, x: 0, y: 0, pixels });
        }
        self.fonts.insert(String::from("latin1"), font);
    }

    pub fn register(&mut self, name: &str, size: u16, data: &[u8]) {
        let mut font = TtfRasterFont::new(name, size);
        font.load_from_bytes(data);
        self.fonts.insert(String::from(name), font);
    }

    pub fn activate(&mut self, name: &str) {
        if name == "vga" { self.use_ttf = false; self.active = String::from("vga"); }
        else if self.fonts.contains_key(name) { self.use_ttf = true; self.active = String::from(name); }
    }

    pub fn list(&self) -> Vec<String> {
        let mut list = vec![String::from("vga (bitmap)")];
        list.extend(self.fonts.keys().cloned());
        list
    }

    pub fn draw_text(&self, fb: &mut DoubleBuffer, x: usize, y: usize, text: &str, scr_w: usize, r: u8, g: u8, b: u8) {
        if self.use_ttf {
            if let Some(font) = self.fonts.get(&self.active) {
                font.draw_text(fb, x, y, text, r, g, b, scr_w);
                return;
            }
        }
        crate::display::font::draw_text_scaled(fb, x, y, text, 1, scr_w, r, g, b);
    }
}

pub static FONT_MANAGER: spin::Mutex<Option<FontManager>> = spin::Mutex::new(None);
