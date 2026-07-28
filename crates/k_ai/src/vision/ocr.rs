//! OCR — detecção de regiões de texto em imagens.
//! Não usa modelo separado: o texto é lido visualmente pelo SigLIP + Pro LLM.
//!
//! Pipeline:
//!   1. Binarização adaptativa (Otsu-like simples)
//!   2. Projeção horizontal → linhas de texto
//!   3. Projeção vertical → palavras
//!   4. Cada região é cropada e passada ao VisionEncoder
//!   5. Pro LLM recebe embedding e "lê" o texto visualmente

use alloc::vec::Vec;
use alloc::vec;

/// Uma região de texto detectada: (x, y, w, h)
#[derive(Debug, Clone, Copy)]
pub struct TextRegion {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

pub struct OcrEngine;

impl OcrEngine {
    pub fn new() -> Self {
        Self
    }

    /// Detecta regiões de texto numa imagem RGBA.
    /// Retorna bounding boxes ordenadas de cima para baixo, esquerda para direita.
    pub fn detect_text(&self, rgba: &[u8], width: u32, height: u32) -> Vec<TextRegion> {
        if width == 0 || height == 0 || rgba.len() < (width * height * 4) as usize {
            return Vec::new();
        }

        // 1. Binarização: luminância média como threshold
        let gray = self::to_grayscale(rgba, width, height);
        let threshold = self::otsu_threshold(&gray);
        let binary: Vec<bool> = gray.iter().map(|&v| v < threshold).collect(); // true = foreground (texto)

        // 2. Projeção horizontal → linhas
        let row_sums: Vec<u32> = (0..height)
            .map(|y| {
                let start = (y * width) as usize;
                binary[start..start + width as usize]
                    .iter()
                    .filter(|&&b| b)
                    .count() as u32
            })
            .collect();

        let text_rows = self::find_bands(&row_sums, width / 20); // min 5% de largura
        if text_rows.is_empty() {
            return Vec::new();
        }

        // 3. Para cada linha, projeção vertical → palavras
        let mut regions = Vec::new();
        for &(y0, y1) in &text_rows {
            let line_h = y1 - y0 + 1;
            let col_sums: Vec<u32> = (0..width)
                .map(|x| {
                    let mut sum = 0u32;
                    for y in y0..=y1 {
                        let idx = (y * width + x) as usize;
                        if binary[idx] {
                            sum += 1;
                        }
                    }
                    sum
                })
                .collect();

            let word_cols = self::find_bands(&col_sums, height / 20);
            for &(x0, x1) in &word_cols {
                // Filtra regiões muito pequenas (ruído)
                if x1 - x0 < 4 || line_h < 4 {
                    continue;
                }
                regions.push(TextRegion {
                    x: x0,
                    y: y0,
                    w: x1 - x0 + 1,
                    h: line_h,
                });
            }
        }

        regions
    }
}

// ─── Funções auxiliares ─────────────────────────────────────────────

fn to_grayscale(rgba: &[u8], w: u32, h: u32) -> Vec<u8> {
    let n = (w * h) as usize;
    let mut gray = vec![0u8; n];
    for i in 0..n {
        let px = i * 4;
        // BT.601 luminance: 0.299 R + 0.587 G + 0.114 B
        let r = rgba[px] as f32;
        let g = rgba[px + 1] as f32;
        let b = rgba[px + 2] as f32;
        gray[i] = (0.299 * r + 0.587 * g + 0.114 * b) as u8;
    }
    gray
}

fn otsu_threshold(gray: &[u8]) -> u8 {
    if gray.is_empty() {
        return 128;
    }
    let mut hist = [0u32; 256];
    for &v in gray {
        hist[v as usize] += 1;
    }
    let total = gray.len() as u32;
    let mut sum_b = 0u32;
    let mut w_b = 0u32;
    let mut w_f: u32;
    let mut sum = 0u32;
    for i in 0..256 {
        sum += i as u32 * hist[i];
    }
    let mut max_var = 0.0f32;
    let mut threshold = 128u8;
    for t in 0..256 {
        w_b += hist[t];
        if w_b == 0 {
            continue;
        }
        w_f = total - w_b;
        if w_f == 0 {
            break;
        }
        sum_b += t as u32 * hist[t];
        let m_b = sum_b as f32 / w_b as f32;
        let m_f = (sum - sum_b) as f32 / w_f as f32;
        let var = w_b as f32 * w_f as f32 * (m_b - m_f) * (m_b - m_f);
        if var > max_var {
            max_var = var;
            threshold = t as u8;
        }
    }
    threshold
}

/// Encontra faixas (bandas) onde a soma ultrapassa o limiar.
/// Retorna pares (início, fim) inclusivos.
fn find_bands(sums: &[u32], min_val: u32) -> Vec<(u32, u32)> {
    let mut bands = Vec::new();
    let mut in_band = false;
    let mut start = 0u32;
    for (i, &s) in sums.iter().enumerate() {
        let above = s > min_val;
        if above && !in_band {
            start = i as u32;
            in_band = true;
        } else if !above && in_band {
            bands.push((start, i as u32 - 1));
            in_band = false;
        }
    }
    if in_band {
        bands.push((start, sums.len() as u32 - 1));
    }
    // Une bandas muito próximas (gap < 3px)
    if bands.len() > 1 {
        let mut merged = Vec::with_capacity(bands.len());
        let mut prev = bands[0];
        for &band in bands[1..].iter() {
            if band.0 - prev.1 <= 3 {
                prev.1 = band.1;
            } else {
                merged.push(prev);
                prev = band;
            }
        }
        merged.push(prev);
        merged
    } else {
        bands
    }
}
