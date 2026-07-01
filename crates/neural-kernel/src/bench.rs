//! Benchmarks — framework de benchmarking estilo AxiomOS.
//! Mede: boot time, tick latency, alloc throughput, render FPS.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

static BENCH_START_TICK: AtomicU64 = AtomicU64::new(0);

pub struct Benchmark {
    pub name: String,
    pub start_tick: u64,
    pub end_tick: u64,
    pub result: String,
}

pub fn start_bench(name: &str) {
    let tick = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
    BENCH_START_TICK.store(tick, Ordering::Relaxed);
    crate::serial_println!("[BENCH] Starting: {} @ tick {}", name, tick);
}

pub fn end_bench(name: &str) -> Benchmark {
    let tick = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
    let start = BENCH_START_TICK.load(Ordering::Relaxed);
    let elapsed_ticks = tick - start;
    let elapsed_ms = elapsed_ticks * 55; // 18.2Hz → ~55ms per tick

    let result = alloc::format!("{} ticks ({} ms)", elapsed_ticks, elapsed_ms);
    crate::serial_println!("[BENCH] {}: {}", name, result);

    Benchmark {
        name: String::from(name),
        start_tick: start,
        end_tick: tick,
        result,
    }
}

/// Benchmarks pre-definidos
pub fn run_all_benches() -> Vec<Benchmark> {
    let mut results = Vec::new();

    // Boot tick measurement
    let boot_ticks = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
    results.push(Benchmark {
        name: String::from("boot_ticks"),
        start_tick: 0,
        end_tick: boot_ticks,
        result: alloc::format!("{} ticks to boot", boot_ticks),
    });

    // Alloc throughput
    start_bench("alloc_throughput");
    let mut allocs = 0u64;
    for _ in 0..1000 {
        let _v = alloc::vec![0u8; 64];
        allocs += 1;
    }
    let b = end_bench("alloc_throughput");
    results.push(b);

    results
}
