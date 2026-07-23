//! ADR-0063 Q2/D4 — micro-bench ART + BQ com TSC (P50 aproximado, sem claim P99 DoD).

use alloc::format;
use alloc::string::String;

use super::art::ArtIndex;
use super::bq::{self, BqFlatIndex};
use super::hamming_dispatch;

fn rdtsc() -> u64 {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::x86_64::_rdtsc()
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        0
    }
}

/// Insere N chaves ART + M vetores BQ; verifica lookup/top_k.
pub fn bench_smoke(n_art: usize, n_bq: usize) -> (bool, String) {
    hamming_dispatch::select_best_hamming_kernel();
    let t0 = rdtsc();
    let mut art = ArtIndex::new();
    for i in 0..n_art {
        let k = format!("bench/k{:06}", i);
        art.insert(&k, i as u64);
    }
    let t1 = rdtsc();
    let last = n_art.saturating_sub(1);
    let art_ok = art.get("bench/k000000") == Some(0)
        && art.get(&format!("bench/k{:06}", last)) == Some(last as u64);
    let t2 = rdtsc();

    let mut bq_idx = BqFlatIndex::new();
    // D4: 1024-dim quando n_bq >= 256 (pesado); senão 16-dim leve
    let heavy = n_bq >= 256;
    for i in 0..n_bq {
        if heavy {
            let mut bits = [0u64; 16];
            bits[0] = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
            bits[i % 16] ^= 1u64 << (i % 64);
            bq_idx.insert_1024(i as u64, &bits);
        } else {
            let mut v = [0.0f32; 16];
            v[i % 16] = 1.0;
            bq_idx.insert_f32(i as u64, &v);
        }
    }
    let t3 = rdtsc();
    let hits = if heavy {
        let mut bits0 = [0u64; 16];
        bits0[0] = 0u64.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        bits0[0 % 16] ^= 1u64 << (0 % 64);
        bq_idx.top_k(&bits0, 1)
    } else {
        let mut v = [0.0f32; 16];
        v[0] = 1.0;
        bq_idx.top_k_f32(&v, 1)
    };
    let t4 = rdtsc();
    let bq_ok = !hits.is_empty() && hits[0].0 == 0;

    let ok = art_ok && bq_ok && art.len == n_art && bq_idx.len() == n_bq;
    (
        ok,
        format!(
            "art_n={} bq_n={} art_ok={} bq_ok={} path={} insert_cyc≈{} lookup_cyc≈{} topk_cyc≈{}",
            n_art,
            n_bq,
            art_ok,
            bq_ok,
            bq::hamming_path(),
            t1.saturating_sub(t0),
            t2.saturating_sub(t1),
            t4.saturating_sub(t3)
        ),
    )
}

/// Aceite intermediário: 10k ART + 1k BQ.
pub fn bench_intermediate() -> (bool, String) {
    bench_smoke(10_000, 1_024)
}

/// Aceite D-series: ART 100k + BQ 10k × 1024-dim (não DoD 10M/100k).
pub fn bench_d_series() -> (bool, String) {
    let (ok, msg) = bench_smoke(100_000, 10_000);
    (ok, format!("D-series {}", msg))
}
