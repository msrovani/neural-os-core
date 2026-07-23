//! ADR-0063 Q5/D4 — métricas SGDB no serial (bench D-series + Tickv stats).

use alloc::format;
use alloc::string::String;

use super::bench;
use super::bq;
use super::hamming_dispatch;

pub fn report_line() -> String {
    hamming_dispatch::select_best_hamming_kernel();
    let (ok, msg) = bench::bench_d_series();
    format!(
        "sgdb.bench.D ok={} {} hamming={} | {}",
        ok,
        msg,
        bq::hamming_path(),
        k_nano::storage::tickv_status()
    )
}

pub fn boot_log_metrics() {
    let line = report_line();
    let _ = line;
}
