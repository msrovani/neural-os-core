//! Shapes W2A8 para família Falcon3 Instruct **1.58bit** (1B / 3B / 7B / 10B).
//!
//! Opções = `cortex::model_fit::Falcon3Kind`. Decode = GEMV M=1.
//! Preferir `loaded_model_header()`; senão SKU preferido (default lab = 3B).
//! NÃO hardcodar BitNet-2B (2560/6912) nem colar 7B no 3B.

use core::sync::atomic::{AtomicU8, Ordering};
use cortex::model_fit::Falcon3Kind;

use crate::gpu::compute_abi::IsaTag;

/// Alias estável p/ packs/golden (mesma enum do fit).
pub type Falcon3Sku = Falcon3Kind;

/// Lab default ADR-0101.
pub const DEFAULT_SKU: Falcon3Sku = Falcon3Kind::Daily3B;

/// SKU preferido quando nenhum modelo está carregado (0=1B … 3=10B; default 1=3B).
static PREFERRED_SKU: AtomicU8 = AtomicU8::new(1);

fn sku_to_u8(k: Falcon3Kind) -> u8 {
    match k {
        Falcon3Kind::Tiny1B => 0,
        Falcon3Kind::Daily3B => 1,
        Falcon3Kind::Goal7B => 2,
        Falcon3Kind::Large10B => 3,
    }
}

fn u8_to_sku(v: u8) -> Falcon3Kind {
    match v {
        0 => Falcon3Kind::Tiny1B,
        2 => Falcon3Kind::Goal7B,
        3 => Falcon3Kind::Large10B,
        _ => Falcon3Kind::Daily3B,
    }
}

/// Define SKU de fallback (1b/3b/7b/10b) para golden/pack sem header carregado.
pub fn set_preferred_sku(sku: Falcon3Sku) {
    PREFERRED_SKU.store(sku_to_u8(sku), Ordering::Release);
}

pub fn preferred_sku() -> Falcon3Sku {
    u8_to_sku(PREFERRED_SKU.load(Ordering::Acquire))
}

/// Infere SKU a partir do header v6 (layers + hidden + FFN).
pub fn sku_from_header(hidden: usize, intermediate: usize, layers: usize) -> Option<Falcon3Sku> {
    for k in Falcon3Kind::ALL {
        if k.hidden() == hidden && k.intermediate() == intermediate && k.layers() == layers {
            return Some(k);
        }
    }
    // 7B vs 10B: mesmo h/FFN — desambigua por layers
    if hidden == 3072 && intermediate == 23040 {
        return Some(if layers >= 36 {
            Falcon3Kind::Large10B
        } else {
            Falcon3Kind::Goal7B
        });
    }
    if hidden == 3072 && intermediate == 9216 {
        return Some(Falcon3Kind::Daily3B);
    }
    if hidden == 2048 && intermediate == 8192 {
        return Some(Falcon3Kind::Tiny1B);
    }
    None
}

/// Par (N, K) de um BitLinear no decode (M=1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GemvShape {
    pub n: u32,
    pub k: u32,
}

impl GemvShape {
    pub const fn new(n: usize, k: usize) -> Self {
        Self {
            n: n as u32,
            k: k as u32,
        }
    }
}

/// Dimensões ativas: header carregado → SKU preferido → lab 3B.
pub fn active_dims() -> (usize, usize) {
    if let Some(h) = cortex::model::loaded_model_header() {
        if h.hidden > 0 && h.intermediate > 0 {
            return (h.hidden, h.intermediate);
        }
    }
    let sku = preferred_sku();
    (sku.hidden(), sku.intermediate())
}

pub fn active_sku() -> Falcon3Sku {
    if let Some(h) = cortex::model::loaded_model_header() {
        if let Some(s) = sku_from_header(h.hidden, h.intermediate, h.num_layers) {
            return s;
        }
    }
    preferred_sku()
}

/// GEMV decode para um SKU explícito (pack/golden por opção).
pub fn decode_gemv_shapes_for(sku: Falcon3Sku) -> [GemvShape; 3] {
    let h = sku.hidden();
    let ffn = sku.intermediate();
    [
        GemvShape::new(h, h),   // attn q/o-like
        GemvShape::new(ffn, h), // gate/up
        GemvShape::new(h, ffn), // down
    ]
}

/// Matrizes BitLinear do LLM ativo (header ou SKU preferido).
pub fn falcon3_decode_gemv_shapes() -> [GemvShape; 3] {
    let (h, ffn) = active_dims();
    [
        GemvShape::new(h, h),
        GemvShape::new(ffn, h),
        GemvShape::new(h, ffn),
    ]
}

/// Todos os GEMV únicos da família (7B≡10B em shape → um só).
pub fn unique_family_gemv_shapes() -> alloc::vec::Vec<GemvShape> {
    use alloc::vec::Vec;
    let mut out: Vec<GemvShape> = Vec::new();
    for sku in [
        Falcon3Kind::Tiny1B,
        Falcon3Kind::Daily3B,
        Falcon3Kind::Goal7B, // cobre também 10B (mesmo h/FFN)
    ] {
        for s in decode_gemv_shapes_for(sku) {
            if !out.contains(&s) {
                out.push(s);
            }
        }
    }
    out
}

pub fn w2a8_profile_for_isa(isa: IsaTag) -> &'static str {
    // Espelha compute_abi::features_for_isa — gfx90c = MadInt8 (SEM WMMA).
    match isa {
        IsaTag::Sm61 | IsaTag::Sm70 | IsaTag::Sm75 | IsaTag::Sm80 | IsaTag::Sm86 | IsaTag::Sm89 => {
            "dp4a_w2a8"
        }
        IsaTag::Gen9 | IsaTag::Sm52 | IsaTag::Gfx90c => "mad_int8",
        IsaTag::Dg2 => "dp4a_w2a8",
        IsaTag::Gfx1030 | IsaTag::Gfx1036 => "dot_int8",
        IsaTag::Gfx1103 => "wmma_i8",
        IsaTag::None => "cpu_fallback",
    }
}

pub fn packed_weight_bytes(shape: GemvShape) -> usize {
    let elems = (shape.n as usize).saturating_mul(shape.k as usize);
    (elems + 3) / 4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_skus_have_distinct_or_documented_shapes() {
        let s1 = decode_gemv_shapes_for(Falcon3Kind::Tiny1B);
        let s3 = decode_gemv_shapes_for(Falcon3Kind::Daily3B);
        let s7 = decode_gemv_shapes_for(Falcon3Kind::Goal7B);
        let s10 = decode_gemv_shapes_for(Falcon3Kind::Large10B);
        assert_eq!(s1[0], GemvShape::new(2048, 2048));
        assert_eq!(s1[1], GemvShape::new(8192, 2048));
        assert_eq!(s3[0], GemvShape::new(3072, 3072));
        assert_eq!(s3[1], GemvShape::new(9216, 3072));
        assert_eq!(s7[1], GemvShape::new(23040, 3072));
        // 7B e 10B: mesmo GEMV; layers diferem (28 vs 40)
        assert_eq!(s7, s10);
        assert_ne!(Falcon3Kind::Goal7B.layers(), Falcon3Kind::Large10B.layers());
        // Nunca BitNet-2B
        assert_ne!(s3[0].k, 2560);
    }

    #[test]
    fn preferred_sku_roundtrip() {
        set_preferred_sku(Falcon3Kind::Tiny1B);
        assert_eq!(preferred_sku(), Falcon3Kind::Tiny1B);
        set_preferred_sku(Falcon3Kind::Large10B);
        assert_eq!(preferred_sku(), Falcon3Kind::Large10B);
        set_preferred_sku(DEFAULT_SKU);
        assert_eq!(preferred_sku(), Falcon3Kind::Daily3B);
    }

    #[test]
    fn sku_from_header_disambiguates_7_vs_10() {
        assert_eq!(
            sku_from_header(3072, 23040, 28),
            Some(Falcon3Kind::Goal7B)
        );
        assert_eq!(
            sku_from_header(3072, 23040, 40),
            Some(Falcon3Kind::Large10B)
        );
        assert_eq!(
            sku_from_header(3072, 9216, 22),
            Some(Falcon3Kind::Daily3B)
        );
        assert_eq!(
            sku_from_header(2048, 8192, 18),
            Some(Falcon3Kind::Tiny1B)
        );
    }

    #[test]
    fn unique_family_covers_1b_3b_7b() {
        let u = unique_family_gemv_shapes();
        assert!(u.contains(&GemvShape::new(2048, 2048)));
        assert!(u.contains(&GemvShape::new(3072, 3072)));
        assert!(u.contains(&GemvShape::new(23040, 3072)));
        assert!(!u.iter().any(|s| s.k == 2560));
    }

    #[test]
    fn gfx90c_profile_is_mad_not_wmma() {
        assert_eq!(w2a8_profile_for_isa(IsaTag::Gfx90c), "mad_int8");
        assert_eq!(w2a8_profile_for_isa(IsaTag::Gfx1103), "wmma_i8");
        assert_eq!(w2a8_profile_for_isa(IsaTag::Gen9), "mad_int8");
    }
}
