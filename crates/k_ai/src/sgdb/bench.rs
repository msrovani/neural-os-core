//! ADR-0063 F7 — micro-bench ART + BQ (contagem de ops; sem ciclo TSC obrigatório).

use alloc::format;
use alloc::string::String;

use super::art::ArtIndex;
use super::bq::BqFlatIndex;

/// Insere N chaves ART + M vetores BQ; verifica lookup/top_k.
/// Retorna (ok, mensagem status).
pub fn bench_smoke(n_art: usize, n_bq: usize) -> (bool, String) {
    let mut art = ArtIndex::new();
    for i in 0..n_art {
        let k = format!("bench/k{:04}", i);
        art.insert(&k, i as u64);
    }
    let art_ok = art.get("bench/k0000") == Some(0)
        && art.get(&format!("bench/k{:04}", n_art.saturating_sub(1)))
            == Some((n_art.saturating_sub(1)) as u64);

    let mut bq = BqFlatIndex::new();
    for i in 0..n_bq {
        let mut v = [0.0f32; 16];
        v[i % 16] = 1.0;
        bq.insert_f32(i as u64, &v);
    }
    let q = {
        let mut v = [0.0f32; 16];
        v[0] = 1.0;
        v
    };
    let hits = bq.top_k_f32(&q, 1);
    let bq_ok = !hits.is_empty() && hits[0].0 == 0;

    let ok = art_ok && bq_ok && art.len == n_art && bq.len() == n_bq;
    (
        ok,
        format!(
            "art_n={} bq_n={} art_ok={} bq_ok={}",
            n_art, n_bq, art_ok, bq_ok
        ),
    )
}
