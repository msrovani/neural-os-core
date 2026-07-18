extern crate alloc;
use alloc::vec::Vec;
use cortex::tensor::Tensor;
use crate::gpu::backend::gpu_matmul;
use core::sync::atomic::{AtomicU64, Ordering};

static BENCH_DONE: AtomicU64 = AtomicU64::new(0);

pub fn run_benchmark() {
    if BENCH_DONE.load(Ordering::Relaxed) != 0 { return; }
    BENCH_DONE.store(1, Ordering::Relaxed);

    let sizes = [32usize, 64, 128];
    k_nano::slog_hal!("BENCH", "info", "GPU Benchmark (TFLOPS) — matmul ternário");

    for &n in &sizes {
        let a_data: Vec<f32> = (0..n*n).map(|i| (i as f32 / n as f32) - 0.5).collect();
        let b_data: Vec<f32> = (0..n*n).map(|i| if i % 3 == 0 { 1.0 } else { 0.0 }).collect();
        let a_opt = Tensor::from_row_major((n, n), a_data);
        let b_opt = Tensor::from_row_major((n, n), b_data);
        let (a, b) = match (a_opt, b_opt) {
            (Some(a), Some(b)) => (a, b),
            _ => { k_nano::slog_hal!("GPU", "bench", "{}x{}: alloc failed", n, n); continue; }
        };

        let ops = 2.0 * (n as f64) * (n as f64) * (n as f64);

        let warm = gpu_matmul(&a, &b);
        if warm.is_none() { k_nano::slog_hal!("GPU", "bench", "{}x{}: matmul failed", n, n); continue; }

        let mid = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed);
        let runs = 3;
        for _ in 0..runs { let _ = gpu_matmul(&a, &b); }
        let end = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed);

        let ticks = (end - mid) as f64;
        let secs = ticks / 18.2;
        let tflops = (ops * runs as f64 / secs) / 1e12;
        k_nano::slog_hal!("GPU", "bench", "{}x{}: {:.3} TFLOPS ({:.1}s CPU, {} runs)", n, n, tflops, secs, runs);
    }
    k_nano::slog_hal!("BENCH", "info", "GPU backends: CPU fallback (v1.1.1 pipeline OK)");
}
