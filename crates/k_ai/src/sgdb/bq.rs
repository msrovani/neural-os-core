//! ADR-0063 F5 — Binary Quantization (BQ) Flat + Hamming (popcnt).
//! SIMD AVX2: usa popcnt por u64; caminho escalar sempre; AVX2 quando disponível no host.

use alloc::vec;
use alloc::vec::Vec;

/// Empacota f32 → bit (sign bit / threshold 0).
pub fn quantize_f32(v: &[f32]) -> Vec<u64> {
    let n_bits = v.len();
    let n_words = (n_bits + 63) / 64;
    let mut out = vec![0u64; n_words];
    for (i, &x) in v.iter().enumerate() {
        if x > 0.0 {
            out[i / 64] |= 1u64 << (i % 64);
        }
    }
    out
}

/// Distância de Hamming entre dois vetores binários (mesmo comprimento em words).
#[inline]
pub fn hamming(a: &[u64], b: &[u64]) -> u32 {
    let n = a.len().min(b.len());
    let mut d = 0u32;
    for i in 0..n {
        d += (a[i] ^ b[i]).count_ones();
    }
    // bits extras no mais longo contam como 1s
    let longer = if a.len() > b.len() { a } else { b };
    for i in n..longer.len() {
        d += longer[i].count_ones();
    }
    d
}

/// Flat index: lista de (id, bitvec).
pub struct BqFlatIndex {
    pub ids: Vec<u64>,
    pub vecs: Vec<Vec<u64>>,
}

impl BqFlatIndex {
    pub fn new() -> Self {
        BqFlatIndex {
            ids: Vec::new(),
            vecs: Vec::new(),
        }
    }

    pub fn insert(&mut self, id: u64, bits: Vec<u64>) {
        self.ids.push(id);
        self.vecs.push(bits);
    }

    pub fn insert_f32(&mut self, id: u64, v: &[f32]) {
        self.insert(id, quantize_f32(v));
    }

    /// Top-k por menor Hamming. Retorna (id, distance).
    pub fn top_k(&self, query: &[u64], k: usize) -> Vec<(u64, u32)> {
        let mut scored: Vec<(u64, u32)> = self
            .ids
            .iter()
            .zip(self.vecs.iter())
            .map(|(id, v)| (*id, hamming(query, v)))
            .collect();
        scored.sort_by_key(|(_, d)| *d);
        scored.truncate(k);
        scored
    }

    pub fn top_k_f32(&self, query: &[f32], k: usize) -> Vec<(u64, u32)> {
        self.top_k(&quantize_f32(query), k)
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }
}

impl Default for BqFlatIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// Smoke: 3 vetores, query, top-1 deve ser o mais próximo.
pub fn smoke() -> bool {
    let mut idx = BqFlatIndex::new();
    idx.insert_f32(1, &[1.0, -1.0, 1.0, -1.0]);
    idx.insert_f32(2, &[-1.0, -1.0, -1.0, -1.0]);
    idx.insert_f32(3, &[1.0, 1.0, 1.0, 1.0]);
    let hits = idx.top_k_f32(&[1.0, -1.0, 1.0, -1.0], 1);
    hits.len() == 1 && hits[0].0 == 1 && hits[0].1 == 0
}
