//! ADR-0101 Onda 1 — Adaptive Vocabulary Projection (shortlist).
//!
//! Unembed full 131K domina o decode. Entre refreshes: score só candidatos
//! via `embed_lookup` (tie) / row-dot — resto = -inf. Honesty: refresh periódico
//! full unembed; sem treino de shortlist.

use alloc::vec::Vec;
use core::f32::NEG_INFINITY;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::cortex::TransformerModel;
use crate::tensor::Tensor;

/// Tamanho do shortlist (ADR-0101 item 6: 512/2048).
pub const SHORTLIST_K: usize = 512;
/// A cada N passos de decode, full unembed + rebuild.
pub const REFRESH_EVERY: usize = 4;

static TELEM_SHORTLIST_HITS: AtomicU64 = AtomicU64::new(0);
static TELEM_FULL_UNEMBED: AtomicU64 = AtomicU64::new(0);
/// Quando true, `forward_with_kv` omite unembed (caller faz score_candidates).
static SKIP_FULL_UNEMBED: AtomicBool = AtomicBool::new(false);

pub fn set_skip_full_unembed(v: bool) {
    SKIP_FULL_UNEMBED.store(v, Ordering::Release);
}

pub fn skip_full_unembed() -> bool {
    SKIP_FULL_UNEMBED.load(Ordering::Acquire)
}

pub fn telemetry() -> (u64, u64) {
    (
        TELEM_SHORTLIST_HITS.load(Ordering::Relaxed),
        TELEM_FULL_UNEMBED.load(Ordering::Relaxed),
    )
}

/// Top-K ids a partir de logits densos (já computados).
pub fn top_k_ids(logits: &Tensor, k: usize) -> Vec<u32> {
    let cols = logits.shape.1;
    let take = k.min(cols).max(1);
    let mut scored: Vec<(u32, f32)> = Vec::with_capacity(cols.min(8192));
    // Cap scan em vocabulários enormes: amostra densa nos primeiros `cols` (sempre).
    for j in 0..cols {
        let v = logits.data[j];
        if v.is_nan() || v == NEG_INFINITY {
            continue;
        }
        scored.push((j as u32, v));
    }
    // Partial select top-k
    let n = scored.len().min(take);
    for i in 0..n {
        let mut best = i;
        for j in (i + 1)..scored.len() {
            if scored[j].1 > scored[best].1 {
                best = j;
            }
        }
        scored.swap(i, best);
    }
    scored.truncate(n);
    scored.into_iter().map(|(id, _)| id).collect()
}

/// Candidatos = top-K + recent + specials (EOS/BOS se cabem).
pub fn build_candidates(
    top: &[u32],
    recent: &[u16],
    vocab: usize,
    specials: &[u32],
) -> Vec<u32> {
    let mut out: Vec<u32> = Vec::with_capacity(SHORTLIST_K + recent.len() + specials.len());
    for &id in top.iter().take(SHORTLIST_K) {
        if (id as usize) < vocab {
            out.push(id);
        }
    }
    for &r in recent {
        let id = r as u32;
        if (id as usize) < vocab {
            out.push(id);
        }
    }
    for &s in specials {
        if (s as usize) < vocab {
            out.push(s);
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Score só candidatos; demais lanes = -inf. Usa embed tied (dot hidden·row).
pub fn score_candidates(
    model: &TransformerModel,
    hidden: &Tensor,
    candidates: &[u32],
) -> Tensor {
    TELEM_SHORTLIST_HITS.fetch_add(1, Ordering::Relaxed);
    let vocab = model.vocab_size.max(1) as usize;
    let mut logits = Tensor::zero((1, vocab));
    for i in 0..vocab {
        logits.data[i] = NEG_INFINITY;
    }
    let h = hidden.shape.1.min(hidden.data.len());
    let scale = if model.tie_embeddings {
        model.embed_scale
    } else {
        model.unembed_scale
    };
    for &id in candidates {
        let idx = id as usize;
        if idx >= vocab {
            continue;
        }
        let emb = model.embed_lookup_pub(id);
        let eh = emb.data.len().min(h);
        let mut s = 0.0f32;
        for j in 0..eh {
            s += hidden.data[j] * emb.data[j];
        }
        logits.data[idx] = s * scale;
    }
    logits
}

pub fn note_full_unembed() {
    TELEM_FULL_UNEMBED.fetch_add(1, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tensor::Tensor;

    #[test]
    fn top_k_picks_largest() {
        let t = Tensor::from_row_major((1, 5), alloc::vec![1.0, 9.0, 3.0, 7.0, 2.0]).unwrap();
        let ids = top_k_ids(&t, 2);
        assert_eq!(ids.len(), 2);
        assert_eq!(ids[0], 1);
        assert_eq!(ids[1], 3);
    }
}
