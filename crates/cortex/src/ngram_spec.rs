//! N-gram speculative decoding (llama.cpp-style): rolling LCG hash → draft M tokens → verify.
//! Zero new deps. History map is last-writer-wins by continuation start index.
//! Includes empirical accept-rate counters for speedup estimate (ADR-0047 bench).

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use crate::tensor::Tensor;
use crate::cortex::argmax_row;

/// Context window size for the n-gram key.
pub const N: usize = 8;
/// Max draft tokens proposed from a past match.
pub const M: usize = 4;
const MUL: u64 = 6364136223846793005;

static SPEC_STEPS: AtomicU64 = AtomicU64::new(0);
static SPEC_TOKENS: AtomicU64 = AtomicU64::new(0);
static CLASSIC_STEPS: AtomicU64 = AtomicU64::new(0);
static SPEC_FORWARDS: AtomicU64 = AtomicU64::new(0);
static CLASSIC_FORWARDS: AtomicU64 = AtomicU64::new(0);
static SPEC_HITS: AtomicU64 = AtomicU64::new(0);
static SPEC_MISSES: AtomicU64 = AtomicU64::new(0);

pub fn record_spec_hit(tokens_accepted: u64) {
    SPEC_STEPS.fetch_add(1, Ordering::Relaxed);
    SPEC_HITS.fetch_add(1, Ordering::Relaxed);
    SPEC_TOKENS.fetch_add(tokens_accepted, Ordering::Relaxed);
    SPEC_FORWARDS.fetch_add(1, Ordering::Relaxed); // one batch forward
}

pub fn record_spec_bonus_forward() {
    SPEC_FORWARDS.fetch_add(1, Ordering::Relaxed);
}

pub fn record_spec_tokens(n: u64) {
    SPEC_TOKENS.fetch_add(n, Ordering::Relaxed);
}

pub fn record_classic_step() {
    CLASSIC_STEPS.fetch_add(1, Ordering::Relaxed);
    CLASSIC_FORWARDS.fetch_add(1, Ordering::Relaxed);
    SPEC_MISSES.fetch_add(1, Ordering::Relaxed);
}

pub fn bench_stats() -> (u64, u64, u64, u64, u64, u64) {
    (
        SPEC_HITS.load(Ordering::Relaxed),
        SPEC_MISSES.load(Ordering::Relaxed),
        SPEC_TOKENS.load(Ordering::Relaxed),
        SPEC_FORWARDS.load(Ordering::Relaxed),
        CLASSIC_FORWARDS.load(Ordering::Relaxed),
        CLASSIC_STEPS.load(Ordering::Relaxed),
    )
}

/// Estimated speedup vs pure AR: tokens_out / forwards (classic = 1.0).
pub fn speedup_estimate() -> f32 {
    let (_, _, tokens, spec_fwd, classic_fwd, _) = bench_stats();
    let fwd = spec_fwd + classic_fwd;
    let tok = tokens + classic_fwd; // classic: 1 token per forward
    if fwd == 0 {
        return 1.0;
    }
    tok as f32 / fwd as f32
}

/// Microbench without model: repetitive pattern → propose rate.
pub fn microbench_accept_pattern() -> (usize, usize, f32) {
    let mut spec = NgramSpeculator::new();
    // Pattern ABC repeated — after first cycle, propose should return continuations
    let pat: [u16; 3] = [10, 20, 30];
    let mut proposed = 0usize;
    let mut nonempty = 0usize;
    for i in 0..64 {
        let t = pat[i % 3];
        let draft = spec.propose();
        proposed += 1;
        if !draft.is_empty() {
            nonempty += 1;
        }
        spec.feed(t);
    }
    let rate = if proposed > 0 {
        nonempty as f32 / proposed as f32
    } else {
        0.0
    };
    (nonempty, proposed, rate)
}

pub fn log_bench_gate() {
    let (hits, misses, tokens, spec_fwd, classic_fwd, _) = bench_stats();
    let (nonempty, proposed, rate) = microbench_accept_pattern();
    let est = speedup_estimate();
    k_nano::slog_cortex!("ADR", "0047-NGRAM", "hits={} misses={} tokens={} fwd_spec={} fwd_ar={} speedup_est={:.2}x micro_hit={}/{} ({:.0}%)",
        hits, misses, tokens, spec_fwd, classic_fwd, est, nonempty, proposed, rate * 100.0);
}

fn hash_window(tokens: &[u16]) -> u64 {
    let mut h = 0u64;
    for &t in tokens {
        h = h.wrapping_mul(MUL).wrapping_add(t as u64);
    }
    h
}

/// Speculator: token history + hash→continuation-start (last-writer-wins).
pub struct NgramSpeculator {
    history: Vec<u16>,
    /// (hash, index in history where continuation starts after a past n-gram)
    entries: Vec<(u64, usize)>,
}

impl NgramSpeculator {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            entries: Vec::new(),
        }
    }

    fn upsert(&mut self, hash: u64, cont_start: usize) {
        for e in self.entries.iter_mut() {
            if e.0 == hash {
                e.1 = cont_start;
                return;
            }
        }
        self.entries.push((hash, cont_start));
    }

    /// Record token; when N+1 history exists, map n-gram ending at len-2 → continuation at len-1.
    pub fn feed(&mut self, token: u16) {
        self.history.push(token);
        let len = self.history.len();
        if len >= N + 1 {
            let cont_start = len - 1;
            let ngram_start = cont_start - N;
            let h = hash_window(&self.history[ngram_start..cont_start]);
            self.upsert(h, cont_start);
        }
    }

    pub fn feed_slice(&mut self, tokens: &[u16]) {
        for &t in tokens {
            self.feed(t);
        }
    }

    /// Propose up to M tokens that followed the last occurrence of the current N-gram.
    pub fn propose(&self) -> Vec<u16> {
        let len = self.history.len();
        if len < N {
            return Vec::new();
        }
        let h = hash_window(&self.history[len - N..]);
        for &(eh, cont_start) in self.entries.iter() {
            if eh == h && cont_start < len {
                let end = (cont_start + M).min(len);
                if cont_start < end {
                    return self.history[cont_start..end].to_vec();
                }
            }
        }
        Vec::new()
    }
}

/// Verify draft[1..] against per-position logits from a parallel forward of `drafts`.
/// Caller must already have verified drafts[0] against the previous-step logits.
pub fn verify_draft(all_logits: &Tensor, drafts: &[u16]) -> (usize, u16) {
    let mut accept = 0usize;
    for i in 0..drafts.len().saturating_sub(1) {
        let predicted = argmax_row(all_logits, i);
        if predicted == drafts[i + 1] {
            accept += 1;
        } else {
            return (accept, predicted);
        }
    }
    let next = if drafts.is_empty() {
        1 // EOS fallback
    } else {
        argmax_row(all_logits, drafts.len() - 1)
    };
    (accept, next)
}
