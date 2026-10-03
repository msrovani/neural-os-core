//! H2O heavy-hitter + PagedAttention-lite for KvCache (ADR-0047-GPU G4).
//! CPU-first: evict mid-context low-norm KV; keep recent window + top heavy hitters.

use alloc::vec::Vec;
use crate::cortex::{kv_quantize, KvCache, KvPageList};
use core::sync::atomic::{AtomicU64, Ordering};

static TELEM_EVICT_DROPS: AtomicU64 = AtomicU64::new(0);
static TELEM_EVICT_CALLS: AtomicU64 = AtomicU64::new(0);

pub fn record_evict(dropped: usize) {
    TELEM_EVICT_CALLS.fetch_add(1, Ordering::Relaxed);
    TELEM_EVICT_DROPS.fetch_add(dropped as u64, Ordering::Relaxed);
}

pub fn telemetry() -> (u64, u64) {
    (
        TELEM_EVICT_CALLS.load(Ordering::Relaxed),
        TELEM_EVICT_DROPS.load(Ordering::Relaxed),
    )
}

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

    // Score older positions by L2 of K on layer 0 (dequantized INT8)
    let s0: &[f32] = match cache.k_scale.get(0) {
        Some(s) => s.as_slice(),
        None => return 0,
    };
    let layer0 = match cache.k.get(0) {
        Some(list) => list.dequant(s0, list.len()),
        None => return 0,
    };
    let mut scores: Vec<(usize, f32)> = Vec::with_capacity(older);
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
    // M6: partial select → sort_unstable (scores ~centenas de posições; a
    // seleção manual O(older×heavy) era quadrática no caminho hot do decode).
    // Ordem decrescente de score; NaN não ocorre (k_dim guarded) — Equal fallback.
    scores.sort_unstable_by(|a, b| {
        b.1.partial_cmp(&a.1).unwrap_or(core::cmp::Ordering::Equal)
    });
    let keep_h = heavy.min(scores.len());
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
        let old_ks = core::mem::take(&mut cache.k_scale[l]);
        let old_vs = core::mem::take(&mut cache.v_scale[l]);
        let dk = old_k.dequant(&old_ks, old_k.len());
        let dv = old_v.dequant(&old_vs, old_v.len());
        let Some(cap) = new_len.checked_mul(k_dim) else {
            cache.k[l] = old_k;
            cache.v[l] = old_v;
            cache.k_scale[l] = old_ks;
            cache.v_scale[l] = old_vs;
            continue;
        };
        let mut nk: Vec<f32> = Vec::new();
        let mut nv: Vec<f32> = Vec::new();
        if nk.try_reserve_exact(cap).is_err() || nv.try_reserve_exact(cap).is_err() {
            cache.k[l] = old_k;
            cache.v[l] = old_v;
            cache.k_scale[l] = old_ks;
            cache.v_scale[l] = old_vs;
            continue;
        }
        for &pos in &keep_idx {
            let base = pos * k_dim;
            if base + k_dim <= dk.len() {
                nk.extend_from_slice(&dk[base..base + k_dim]);
            }
            if base + k_dim <= dv.len() {
                nv.extend_from_slice(&dv[base..base + k_dim]);
            }
        }
        let (kq, ksc) = kv_quantize(&nk);
        let (vq, vsc) = kv_quantize(&nv);
        let mut klist = KvPageList::new();
        let mut vlist = KvPageList::new();
        // s439: página TALC indisponível → restaura a camada (mesmo padrão do
        // try_reserve/checked_mul acima): eviction parcial honesta, cache
        // nunca pela metade.
        if !klist.push_i8(&kq) || !vlist.push_i8(&vq) {
            cache.k[l] = old_k;
            cache.v[l] = old_v;
            cache.k_scale[l] = old_ks;
            cache.v_scale[l] = old_vs;
            continue;
        }
        cache.k[l] = klist;
        cache.v[l] = vlist;
        cache.k_scale[l] = ksc;
        cache.v_scale[l] = vsc;
    }
    let dropped = len - new_len;
    cache.len = new_len;
    if dropped > 0 {
        record_evict(dropped);
    }
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
    k_nano::slog_cortex!("ADR", "0047-G4", "kv_len={} {} h2o=ready",
        cache.len,
        pages.status());
}

/// Boot smoke: empty cache + optional synthetic append/evict.
pub fn gate_smoke() -> &'static str {
    let mut cache = KvCache::new(2, 16, 16);
    // Fake 32 tokens of noise so h2o has work
    for _ in 0..32 {
        let k = crate::tensor::Tensor::from_row_major((1, 16), alloc::vec![0.1f32; 16]).unwrap();
        let v = crate::tensor::Tensor::from_row_major((1, 16), alloc::vec![0.05f32; 16]).unwrap();
        assert!(cache.append(0, &k, &v));
        assert!(cache.append(1, &k, &v));
        cache.advance(1);
    }
    let before = cache.len;
    let dropped = h2o_evict(&mut cache, 8, 4);
    log_g4_gate(&cache);
    k_nano::slog_cortex!("ADR", "0047-G4", "h2o before={} after={} dropped={}", before, cache.len, dropped);
    "OK"
}
