//! Split de tempo por estágio de `apply_one_layer` — diagnóstico do gap t/s
//! vs teto de banda (medir antes de otimizar, AGENTS.md/SESSION_329).
//!
//! Atomics lock-free, **zero log no hot path**; `dump_and_reset` 1×/forward,
//! fechado na última camada. Não substitui `matmul_diag` (dispatch de kernel):
//! aqui a pergunta é "onde vão os ~4,3 s/camada", por estágio.

use core::sync::atomic::{AtomicU64, Ordering};

pub const N_STAGES: usize = 7;
pub const S_ATTN_NORM: usize = 0;
pub const S_QKV: usize = 1;
pub const S_KV: usize = 2;
pub const S_ATTN: usize = 3;
pub const S_O_PROJ: usize = 4;
pub const S_FFN_NORM: usize = 5;
pub const S_MLP: usize = 6;

static US: [AtomicU64; N_STAGES] = [const { AtomicU64::new(0) }; N_STAGES];

/// Acumula µs do estágio (soma entre camadas do forward corrente).
#[inline]
pub fn note(stage: usize, us: u64) {
    US[stage].fetch_add(us, Ordering::Relaxed);
}

/// Soma os estágios de um forward, loga 1 linha e zera. Chamar 1×/forward.
pub fn dump_and_reset() {
    let v: [u64; N_STAGES] = core::array::from_fn(|i| US[i].swap(0, Ordering::Relaxed));
    let sum: u64 = v.iter().sum();
    k_nano::slog_cortex!(
        "LayerDiag",
        "ok",
        "stages_us attn_norm={} qkv={} kv={} attn={} o_proj={} ffn_norm={} mlp={} sum={}",
        v[S_ATTN_NORM],
        v[S_QKV],
        v[S_KV],
        v[S_ATTN],
        v[S_O_PROJ],
        v[S_FFN_NORM],
        v[S_MLP],
        sum
    );
}
