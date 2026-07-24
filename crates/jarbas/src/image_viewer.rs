//! Image viewer dither MVP — PBM ASCII P1 (Labor 43).

use alloc::vec::Vec;

/// Parse PBM P1 (bitmap ASCII) → pixels 0/1 row-major. Max 64×64.
pub fn parse_pbm_p1(data: &[u8]) -> Result<(usize, usize, Vec<u8>), &'static str> {
    let s = core::str::from_utf8(data).map_err(|_| "utf8")?;
    let mut lines = s.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty());
    let magic = lines.next().ok_or("eof")?.trim();
    if magic != "P1" {
        return Err("not_p1");
    }
    let dim = lines.next().ok_or("dim")?.trim();
    let mut it = dim.split_whitespace();
    let w: usize = it.next().ok_or("w")?.parse().map_err(|_| "w")?;
    let h: usize = it.next().ok_or("h")?.parse().map_err(|_| "h")?;
    if w == 0 || h == 0 || w > 64 || h > 64 {
        return Err("size");
    }
    let mut pix = Vec::with_capacity(w * h);
    for line in lines {
        for tok in line.split_whitespace() {
            let v: u8 = tok.parse().map_err(|_| "pix")?;
            pix.push(if v != 0 { 1 } else { 0 });
            if pix.len() >= w * h {
                break;
            }
        }
        if pix.len() >= w * h {
            break;
        }
    }
    if pix.len() < w * h {
        return Err("short");
    }
    Ok((w, h, pix))
}

pub fn boot_smoke() -> bool {
    let sample = b"P1\n2 2\n0 1\n1 0\n";
    match parse_pbm_p1(sample) {
        Ok((2, 2, p)) if p.len() == 4 => {
            k_nano::slog_jarbas!(
                "IMG",
                "info",
                "step=pbm status=OK VERDICT=PARTIAL reason=parse_p1_mvp"
            );
            true
        }
        _ => {
            k_nano::slog_jarbas!("IMG", "info", "step=pbm status=FAIL VERDICT=FAIL");
            false
        }
    }
}