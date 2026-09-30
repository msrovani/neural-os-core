//! Probe logits (ADR-0101 Onda 1) — classificação restrita sobre shortlist.
//!
//! Constrói sobre `vocab_shortlist::score_candidates`: dada uma hidden state e
//! um conjunto de ids candidatos, produz uma distribuição softmax restrita aos
//! candidatos. Usado para probes de baixo custo (classificação) sem o unembed
//! completo. Honestidade: a distribuição é sobre os candidatos, não o vocab.

use alloc::vec::Vec;
use core::f32::NEG_INFINITY;

use crate::cortex::TransformerModel;
use crate::tensor::Tensor;
use crate::vocab_shortlist::score_candidates;

/// Softmax restrita aos logits fornecidos (log-sum-exp com subtração de max).
/// Guard: se `sum(exp) <= 1e-10` (todos -inf / NaN), retorna uniforme `1/N`.
pub fn restricted_softmax(logits: &[f32]) -> Vec<f32> {
    let n = logits.len();
    if n == 0 {
        return Vec::new();
    }
    let mut max = NEG_INFINITY;
    for &v in logits {
        if v > max {
            max = v;
        }
    }
    if !max.is_finite() {
        return alloc::vec![1.0 / n as f32; n];
    }
    let mut exps: Vec<f64> = Vec::with_capacity(n);
    let mut sum = 0.0f64;
    for &v in logits {
        let e = if v == NEG_INFINITY {
            0.0
        } else {
            libm::expf(v - max) as f64
        };
        exps.push(e);
        sum += e;
    }
    if sum <= 1e-10 {
        return alloc::vec![1.0 / n as f32; n];
    }
    exps.iter().map(|e| (e / sum) as f32).collect()
}

/// Resultado de um probe: classe vencedora (índice em `candidate_ids`) + probs.
pub struct ProbeResult {
    pub class: usize,
    pub probs: Vec<f32>,
}

/// Classifica `hidden` sobre `candidate_ids`: score_candidates → calibração
/// (`- alpha * bias`) → softmax restrita → argmax + probs.
pub fn probe_classify(
    model: &TransformerModel,
    hidden: &Tensor,
    candidate_ids: &[u32],
    alpha: f32,
    bias: f32,
) -> ProbeResult {
    let logits = score_candidates(model, hidden, candidate_ids);
    let mut vals: Vec<f32> = Vec::with_capacity(candidate_ids.len());
    for &id in candidate_ids {
        let v = logits.data.get(id as usize).copied().unwrap_or(NEG_INFINITY);
        vals.push(if v == NEG_INFINITY { v } else { v - alpha * bias });
    }
    let probs = restricted_softmax(&vals);
    let mut class = 0usize;
    let mut best = NEG_INFINITY;
    for (i, &v) in vals.iter().enumerate() {
        if v > best {
            best = v;
            class = i;
        }
    }
    ProbeResult { class, probs }
}

/// Entropia de Shannon dos logits (f64 acumulado, log-sum-exp).
/// Uniforme sobre N → ln(N); one-hot → 0. Termos com p < 1e-10 são ignorados.
pub fn logit_entropy(logits: &[f32]) -> f32 {
    let n = logits.len();
    if n == 0 {
        return 0.0;
    }
    let mut max = NEG_INFINITY;
    for &v in logits {
        if v > max {
            max = v;
        }
    }
    if !max.is_finite() {
        return libm::log(n as f64) as f32;
    }
    let mut sum = 0.0f64;
    for &v in logits {
        if v == NEG_INFINITY {
            continue;
        }
        sum += libm::expf(v - max) as f64;
    }
    if sum <= 1e-10 {
        return libm::log(n as f64) as f32;
    }
    let mut h = 0.0f64;
    for &v in logits {
        if v == NEG_INFINITY {
            continue;
        }
        let p = (libm::expf(v - max) as f64) / sum;
        if p < 1e-10 {
            continue;
        }
        h -= p * libm::log(p);
    }
    h as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restricted_softmax_sums_to_one() {
        let p = restricted_softmax(&[1.0, 1.0, 1.0, 1.0]);
        assert_eq!(p.len(), 4);
        for &v in &p {
            assert!((v - 0.25).abs() < 1e-6, "uniforme esperado, got {v}");
        }
        let sum: f32 = p.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5);

        // Dois iguais, resto -inf → 0.5/0.5.
        let p2 = restricted_softmax(&[0.0, 0.0, NEG_INFINITY, NEG_INFINITY]);
        assert!((p2[0] - 0.5).abs() < 1e-6);
        assert!((p2[1] - 0.5).abs() < 1e-6);
        assert_eq!(p2[2], 0.0);
        assert_eq!(p2[3], 0.0);
    }

    #[test]
    fn restricted_softmax_uniform_guard_all_neginf() {
        let p = restricted_softmax(&[NEG_INFINITY, NEG_INFINITY, NEG_INFINITY]);
        for &v in &p {
            assert!((v - 1.0 / 3.0).abs() < 1e-6, "guard uniforme, got {v}");
        }
    }

    #[test]
    fn logit_entropy_uniform_is_ln_n() {
        let h = logit_entropy(&[2.5, 2.5, 2.5, 2.5]);
        let want = libm::log(4.0f64) as f32;
        assert!((h - want).abs() < 1e-5, "uniforme N=4 → ln4={want}, got {h}");
    }

    #[test]
    fn logit_entropy_one_hot_is_zero() {
        let h = logit_entropy(&[0.0, NEG_INFINITY, NEG_INFINITY]);
        assert!(h.abs() < 1e-6, "one-hot → 0, got {h}");
    }
}
