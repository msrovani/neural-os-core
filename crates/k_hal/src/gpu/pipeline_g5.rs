//! GPU inference pipeline stages PoC (ADR-0047-GPU G5).
//! Prefill → Decode loop with timing; compute still CPU until HW shaders exist.
//! Target 50μs/token is aspirational — gate reports measured μs honestly.
//!
//! M10: medição em **TSC µs** (não `TIMER_TICKS` — ~18 Hz PIT, resolução 55 ms
//! incapaz de medir um matmul de microssegundos) e gate com matmul **8×8**
//! (2×2 era ~ruído de chamada; 8×8 dá sinal mínimo sem custar nada).

use core::sync::atomic::{AtomicU64, Ordering};
use cortex::tensor::Tensor;

static PREFILL_US: AtomicU64 = AtomicU64::new(0);
static DECODE_US: AtomicU64 = AtomicU64::new(0);
static DECODE_TOKENS: AtomicU64 = AtomicU64::new(0);

pub enum PipeStage {
    Prefill,
    Decode,
}

/// Run one CPU matmul as stand-in decode step; records timing (TSC µs).
pub fn decode_step_cpu(a: &Tensor, b: &Tensor) -> Option<Tensor> {
    let t0 = k_nano::tsc::now_us();
    let out = a.matmul(b);
    let t1 = k_nano::tsc::now_us();
    DECODE_US.fetch_add(t1.saturating_sub(t0), Ordering::Relaxed);
    DECODE_TOKENS.fetch_add(1, Ordering::Relaxed);
    out
}

/// `us` — TSC wall-clock (API renomeada: ticks mentiam resolução).
pub fn record_prefill_us(us: u64) {
    PREFILL_US.store(us, Ordering::Relaxed);
}

pub fn avg_decode_us() -> u64 {
    let tok = DECODE_TOKENS.load(Ordering::Relaxed);
    let us = DECODE_US.load(Ordering::Relaxed);
    if tok == 0 {
        0
    } else {
        us / tok
    }
}

/// Gate: exercise one 8×8 matmul; report measured TSC µs (honest).
pub fn gate_status() -> &'static str {
    let mut va = alloc::vec::Vec::with_capacity(64);
    let mut vb = alloc::vec::Vec::with_capacity(64);
    for i in 0..8usize {
        for j in 0..8usize {
            va.push(if i == j { 1.0f32 } else { 0.0 });
            vb.push(((i * 8 + j) % 7) as f32 * 0.1);
        }
    }
    let Some(a) = Tensor::from_row_major((8, 8), va) else {
        k_nano::slog_hal!("ADR", "0047-G5", "pipeline=CPU FAIL tensor_a");
        return "CPU_PIPELINE_FAIL";
    };
    let Some(b) = Tensor::from_row_major((8, 8), vb) else {
        k_nano::slog_hal!("ADR", "0047-G5", "pipeline=CPU FAIL tensor_b");
        return "CPU_PIPELINE_FAIL";
    };
    let _ = decode_step_cpu(&a, &b);
    let avg = avg_decode_us();
    k_nano::slog_hal!("ADR", "0047-G5", "pipeline=CPU decode_avg_us={} target_us=50 (HW shader deferred)", avg);
    "CPU_PIPELINE"
}
