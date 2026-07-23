//! Cosine similarity (ADR-0064 §2.6).

use crate::tfidf::sqrt_f32;

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len().min(b.len());
    let mut dot = 0.0f32;
    let mut mag_a = 0.0f32;
    let mut mag_b = 0.0f32;
    for i in 0..len {
        dot += a[i] * b[i];
        mag_a += a[i] * a[i];
        mag_b += b[i] * b[i];
    }
    for i in len..a.len() {
        mag_a += a[i] * a[i];
    }
    for i in len..b.len() {
        mag_b += b[i] * b[i];
    }
    if mag_a == 0.0 || mag_b == 0.0 {
        return 0.0;
    }
    dot / (sqrt_f32(mag_a) * sqrt_f32(mag_b))
}
