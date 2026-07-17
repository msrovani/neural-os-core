//! H2O heavy-hitter + PagedAttention-lite for KvCache (ADR-0047-GPU G4).
//! CPU-first: evict mid-context low-norm KV; keep recent window + top heavy hitters.

use alloc::vec::Vec;
use crate::cortex::KvCache;

/// Keep last `recent` tokens always; among older positions keep top `heavy` by ||k||.
pub fn h2o_evict(cache: &mut KvCache, recent: usize, heavy: usize) -> usize {
    let len = cache.len;
    if len == 0 || cache.k.is_empty() {
        return 0;
    }
    let k_dim = cache.k_dim();
    if k_dim == 0 {
        return 0;
    }
    let keep_recent = recent.min(len);
    let older = len.saturating_sub(keep_recent);
    if older == 0 {
        return 0;
    }

    // Score older positions by L2 of K on layer 0
    let mut scores: Vec<(usize, f32)> = Vec::with_capacity(older);
    let layer0 = &cache.k[0];
    for pos in 0..older {
        let base = pos * k_dim;
        if base + k_dim > layer0.len() {
            break;
        }
        let mut acc = 0.0f32;
        for d in 0..k_dim {
            let v = layer0[base + d];
            acc += v * v;
        }
        scores.push((pos, acc));
    }
    // Partial select: keep top `heavy` by score
    let keep_h = heavy.min(scores.len());
    for i in 0..keep_h {
        let mut best = i;
        for j in (i + 1)..scores.len() {
            if scores[j].1 > scores[best].1 {
                best = j;
            }
        }
        scores.swap(i, best);
    }
    let mut keep_idx: Vec<usize> = scores.iter().take(keep_h).map(|(p, _)| *p).collect();
    keep_idx.sort_unstable();
    // Append recent positions
    for pos in older..len {
        keep_idx.push(pos);
    }
    keep_idx.sort_unstable();
    keep_idx.dedup();

    if keep_idx.len() >= len {
        return 0;
    }

    let new_len = keep_idx.len();
    let num_layers = cache.k.len();
    for l in 0..num_layers {
        let old_k = core::mem::take(&mut cache.k[l]);
        let old_v = core::mem::take(&mut cache.v[l]);
        let mut nk = Vec::with_capacity(new_len * k_dim);
        let mut nv = Vec::with_capacity(new_len * k_dim);
        for &pos in &keep_idx {
            let base = pos * k_dim;
            if base + k_dim <= old_k.len() {
                nk.extend_from_slice(&old_k[base..base + k_dim]);
            }
            if base + k_dim <= old_v.len() {
                nv.extend_from_slice(&old_v[base..base + k_dim]);
            }
        }
        cache.k[l] = nk;
        cache.v[l] = nv;
    }
    let dropped = len - new_len;
    cache.len = new_len;
    dropped
}

/// PagedAttention-lite: logical pages of `page_size` tokens (metadata only + optional compact).
pub struct KvPages {
    pub page_size: usize,
    pub num_pages: usize,
    pub tokens: usize,
}

impl KvPages {
    pub fn from_len(len: usize, page_size: usize) -> Self {
        let ps = page_size.max(1);
        KvPages {
            page_size: ps,
            num_pages: (len + ps - 1) / ps,
            tokens: len,
        }
    }

    pub fn status(&self) -> alloc::string::String {
        alloc::format!(
            "pages={} page_size={} tokens={}",
            self.num_pages, self.page_size, self.tokens
        )
    }
}

pub fn log_g4_gate(cache: &KvCache) {
    let pages = KvPages::from_len(cache.len, 16);
    k_nano::serial_println!(
        "[ADR-0047-G4] kv_len={} {} h2o=ready",
        cache.len,
        pages.status()
    );
}

/// Boot smoke: empty cache + optional synthetic append/evict.
pub fn gate_smoke() -> &'static str {
    let mut cache = KvCache::new(2, 16, 16);
    // Fake 32 tokens of noise so h2o has work
    for _ in 0..32 {
        let k = crate::tensor::Tensor::from_row_major((1, 16), alloc::vec![0.1f32; 16]).unwrap();
        let v = crate::tensor::Tensor::from_row_major((1, 16), alloc::vec![0.05f32; 16]).unwrap();
        cache.append(0, &k, &v);
        cache.append(1, &k, &v);
        cache.advance(1);
    }
    let before = cache.len;
    let dropped = h2o_evict(&mut cache, 8, 4);
    log_g4_gate(&cache);
    k_nano::serial_println!(
        "[ADR-0047-G4] h2o before={} after={} dropped={}",
        before, cache.len, dropped
    );
    "OK"
}
