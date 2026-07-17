//! GPU inference pipeline stages PoC (ADR-0047-GPU G5).
//! Prefill → Decode loop with timing; compute still CPU until HW shaders exist.
//! Target 50μs/token is aspirational — gate reports measured ticks honestly.

use core::sync::atomic::{AtomicU64, Ordering};
use cortex::tensor::Tensor;

static PREFILL_TICKS: AtomicU64 = AtomicU64::new(0);
static DECODE_TICKS: AtomicU64 = AtomicU64::new(0);
static DECODE_TOKENS: AtomicU64 = AtomicU64::new(0);

pub enum PipeStage {
    Prefill,
    Decode,
}

/// Run one CPU matmul as stand-in decode step; records timing.
pub fn decode_step_cpu(a: &Tensor, b: &Tensor) -> Option<Tensor> {
    let t0 = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed);
    let out = a.matmul(b);
    let t1 = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed);
    DECODE_TICKS.fetch_add(t1.wrapping_sub(t0) as u64, Ordering::Relaxed);
    DECODE_TOKENS.fetch_add(1, Ordering::Relaxed);
    out
}

pub fn record_prefill_ticks(ticks: u64) {
    PREFILL_TICKS.store(ticks, Ordering::Relaxed);
}

pub fn avg_decode_ticks() -> u64 {
    let tok = DECODE_TOKENS.load(Ordering::Relaxed);
    let ticks = DECODE_TICKS.load(Ordering::Relaxed);
    if tok == 0 {
        0
    } else {
        ticks / tok
    }
}

/// Gate: exercise one tiny matmul; report μs estimate if timer ~1kHz (honest: ticks only).
pub fn gate_status() -> &'static str {
    // 2x2 identity-ish matmul smoke
    let a = Tensor::from_row_major((2, 2), alloc::vec![1.0, 0.0, 0.0, 1.0]).unwrap();
    let b = Tensor::from_row_major((2, 2), alloc::vec![1.0, 0.0, 0.0, 1.0]).unwrap();
    let _ = decode_step_cpu(&a, &b);
    let avg = avg_decode_ticks();
    k_nano::serial_println!(
        "[ADR-0047-G5] pipeline=CPU decode_avg_ticks={} target_us=50 (HW shader deferred)",
        avg
    );
    "CPU_PIPELINE"
}
