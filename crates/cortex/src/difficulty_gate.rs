//! ADR-0101 Onda 2 — Difficulty gate (compute tier) para Falcon3 decode.
//!
//! Honesty: **não** é early-exit treinado com KL(P_inter‖P_final). É política
//! de budget (cheap/normal/full) sobre soft_stride + max_gen. PonderNet/BEI
//! continuam orquestração Hermes — este gate é o gancho no forward 3B.

use core::sync::atomic::{AtomicUsize, Ordering};

/// 0 = auto (legado hidden≥2048 → stride 3); senão força stride.
static SOFT_STRIDE_OVERRIDE: AtomicUsize = AtomicUsize::new(0);
/// 0 = auto; senão força max_gen (microbench QEMU tok/s).
static FORCE_MAX_GEN: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComputeTier {
    /// Greeting / prompt curto — menos layers, max_gen baixo.
    Cheap,
    /// Default chat.
    Normal,
    /// Pedido longo / não-greeting — stride=1 quando possível.
    Full,
}

impl ComputeTier {
    pub fn name(self) -> &'static str {
        match self {
            ComputeTier::Cheap => "cheap",
            ComputeTier::Normal => "normal",
            ComputeTier::Full => "full",
        }
    }
}

/// Classifica sem modelo treinado: heurística honesta (comprimento + greeting).
pub fn classify(prompt: &str, is_greeting: bool, hidden: usize) -> ComputeTier {
    let chars = prompt.len();
    if is_greeting || chars < 48 {
        return ComputeTier::Cheap;
    }
    if hidden >= 2048 && chars > 200 {
        return ComputeTier::Full;
    }
    ComputeTier::Normal
}

/// soft_stride: Full→1; Normal→2 (se heavy); Cheap→3 (legado soft-float).
pub fn soft_stride_for(tier: ComputeTier, hidden: usize) -> usize {
    if hidden < 2048 {
        return 1;
    }
    match tier {
        ComputeTier::Full => 1,
        ComputeTier::Normal => 2,
        ComputeTier::Cheap => 3,
    }
}

pub fn max_gen_for(tier: ComputeTier, hidden: usize, use_bpe: bool, is_greeting: bool) -> usize {
    let forced = FORCE_MAX_GEN.load(Ordering::Acquire);
    if forced > 0 {
        return forced;
    }
    if hidden < 2048 {
        return 16;
    }
    match tier {
        ComputeTier::Cheap => {
            if use_bpe {
                if is_greeting {
                    8
                } else {
                    6
                }
            } else {
                4
            }
        }
        ComputeTier::Normal => {
            if use_bpe {
                12
            } else {
                8
            }
        }
        ComputeTier::Full => {
            if use_bpe {
                24
            } else {
                12
            }
        }
    }
}

/// Microbench / HITL: força max_gen (0 = limpar).
pub fn set_force_max_gen(n: usize) {
    FORCE_MAX_GEN.store(n, Ordering::Release);
}

pub fn set_soft_stride_override(stride: usize) {
    SOFT_STRIDE_OVERRIDE.store(stride, Ordering::Release);
}

pub fn clear_soft_stride_override() {
    SOFT_STRIDE_OVERRIDE.store(0, Ordering::Release);
}

/// Resolve stride efetivo (override HITL/runtime ou auto).
pub fn effective_soft_stride(hidden: usize) -> usize {
    let o = SOFT_STRIDE_OVERRIDE.load(Ordering::Acquire);
    if o > 0 {
        return o;
    }
    // Legado: heavy model soft-float budget.
    if hidden >= 2048 {
        3
    } else {
        1
    }
}

/// Aplica tier → override de stride (chamar antes do forward; limpar depois).
pub fn apply_tier(tier: ComputeTier, hidden: usize) {
    let s = soft_stride_for(tier, hidden);
    set_soft_stride_override(s);
    k_nano::slog_cortex!(
        "DiffGate",
        "ok",
        "tier={} soft_stride={} (ADR-0101 Onda2 policy, not KL)",
        tier.name(),
        s
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greeting_is_cheap() {
        assert_eq!(classify("oi", true, 3072), ComputeTier::Cheap);
    }

    #[test]
    fn full_long_prompt() {
        let p = "a".repeat(250);
        assert_eq!(classify(&p, false, 3072), ComputeTier::Full);
        assert_eq!(soft_stride_for(ComputeTier::Full, 3072), 1);
    }
}
