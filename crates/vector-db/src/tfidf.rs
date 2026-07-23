//! ln_f32 / sqrt_f32 sem libm + compute_tfidf (ADR-0064 §2.3–2.5).

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

/// ln via IEEE754 bit tricks + Pade (≈4 dígitos).
pub fn ln_f32(x: f32) -> f32 {
    if x <= 0.0 {
        return f32::NEG_INFINITY;
    }
    let bits = x.to_bits();
    let e = ((bits >> 23) & 0xFF) as i32 - 127;
    let m_bits = (bits & 0x007F_FFFF) | 0x3F80_0000;
    let m = f32::from_bits(m_bits);
    let t = m - 1.0;
    let ln_m = t * (1.0 - t * (0.5 - t * (1.0 / 3.0 - t * (0.25 - t * 0.2))));
    let ln2: f32 = 0.693_147_2;
    (e as f32) * ln2 + ln_m
}

/// sqrt Newton-Raphson (2 iterações).
pub fn sqrt_f32(x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    let bits = x.to_bits();
    let guess_bits = (bits >> 1) + 0x1FC0_0000;
    let mut guess = f32::from_bits(guess_bits);
    guess = 0.5 * (guess + x / guess);
    guess = 0.5 * (guess + x / guess);
    guess
}

/// TF-IDF denso: tamanho = vocab_size. IDF = ln(N/df)+1.
pub fn compute_tfidf(
    tokens: &[String],
    vocabulary: &BTreeMap<String, usize>,
    df: &[u32],
    doc_count: u32,
) -> Vec<f32> {
    let vocab_size = vocabulary.len();
    let mut vec = vec![0.0f32; vocab_size];
    if tokens.is_empty() || vocab_size == 0 {
        return vec;
    }
    let mut tf_counts: BTreeMap<usize, u32> = BTreeMap::new();
    for t in tokens {
        if let Some(&idx) = vocabulary.get(t) {
            *tf_counts.entry(idx).or_insert(0) += 1;
        }
    }
    let total = tokens.len() as f32;
    let n = doc_count.max(1) as f32;
    for (idx, count) in tf_counts {
        let tf = count as f32 / total;
        let dfi = df.get(idx).copied().unwrap_or(1).max(1) as f32;
        let idf = ln_f32(n / dfi) + 1.0;
        if idx < vocab_size {
            vec[idx] = tf * idf;
        }
    }
    vec
}
