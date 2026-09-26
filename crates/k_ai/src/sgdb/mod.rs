//! ADR-0063 SGDB — quality jump Q1–Q5 + facade AIOS.

pub mod art;
pub mod bench;
pub mod bq;
pub mod crdt_merge;
pub mod crdt_sync;
pub mod e2e_smoke;
pub mod hamming_dispatch;
// CI gate de interop TKLV byte-exata contínua (k_nano ↔ neural-sgdb).
pub mod interop_tklv;
pub mod layers;
pub mod memory_doc;
pub mod metrics;
pub mod store;
// neural-sgdb v1.2.1 compila no_std (SSE2 gateado off x86_64-unknown-none
// no upstream) — bridge real incondicional, sem stub.
pub mod tickv_adapter;
pub mod nsgdb_bridge;

pub use art::ArtIndex;
pub use bq::{hamming, hamming_path, quantize_f32, BqFlatIndex};
pub use hamming_dispatch::{path_name as hamming_kernel_name, select_best_hamming_kernel};
pub use layers::{
    art_prefix, ensure_ready, index_skill, prompt_slice, rag_context, recall_semantic,
    remember_exchange, remember_exchange_full, remember_fact, remember_semantic,
};
pub use memory_doc::{MemoryDoc, MemoryDocView, MemoryLayer, VectorClock};
pub use e2e_smoke::memory_checkpoint_e2e_smoke;
pub use metrics::report_line as metrics_report;
pub use store::{
    backend, boot_init, boot_init_deferred, boot_sgdb_heavy_pending, checkpoint_working,
    force_heavy_index_boot, get_doc, get_hanr, get_kv, get_pkg_body, get_pkg_meta, hw_get, ns,
    prune_working_ram, put_doc, put_hanr, put_kv, put_pkg_body, put_pkg_meta, put_skill_blob, ready,
    status as store_status, with_store,
};

use alloc::vec::Vec;

/// Self-test F2–F7 + view + facade.
pub fn demo() -> bool {
    // Q1: MemoryDoc encode/decode
    let mut doc = MemoryDoc::new(MemoryLayer::L1Working, "hello", b"world".to_vec());
    doc.clock.tick(1);
    let enc = doc.encode();
    let view = match MemoryDocView::parse(&enc) {
        Ok(v) => v,
        Err(_) => {
            k_nano::slog_kai!("SGDB", "fail", "Q1 FAIL: MemoryDocView::parse error");
            return false;
        }
    };
    if view.key() != "hello" || view.payload() != b"world" {
        k_nano::slog_kai!("SGDB", "fail", "Q1 FAIL: view key/payload mismatch");
        return false;
    }
    let dec = match MemoryDoc::decode(&enc) {
        Ok(d) => d,
        Err(_) => {
            k_nano::slog_kai!("SGDB", "fail", "Q1 FAIL: MemoryDoc::decode error");
            return false;
        }
    };
    if dec.key != "hello" || dec.payload.as_slice() != b"world" {
        k_nano::slog_kai!("SGDB", "fail", "Q1 FAIL: decoded key/payload mismatch");
        return false;
    }
    k_nano::slog_kai!("SGDB", "ok", "Q1 PASS: MemoryDoc roundtrip");

    // Q2: ART smoke (3 inserts + get + scan_prefix + delete)
    let mut art = ArtIndex::new();
    art.insert("md/L1/a", 10);
    art.insert("md/L1/b", 20);
    art.insert("md/L2/c", 30);
    if art.get("md/L1/a") != Some(10) || art.get("md/L1/b") != Some(20) {
        k_nano::slog_kai!("SGDB", "fail", "Q2 FAIL: ART get after insert");
        return false;
    }
    if art.scan_prefix("md/L1/").len() < 2 {
        k_nano::slog_kai!("SGDB", "fail", "Q2 FAIL: ART scan_prefix <2");
        return false;
    }
    let _ = art.delete("md/L1/b");
    if art.get("md/L1/b").is_some() {
        k_nano::slog_kai!("SGDB", "fail", "Q2 FAIL: ART delete did not remove");
        return false;
    }
    k_nano::slog_kai!("SGDB", "ok", "Q2 PASS: ART smoke");

    // Q3: BQ smoke (hamming + top_k)
    if !bq::smoke() {
        k_nano::slog_kai!("SGDB", "fail", "Q3 FAIL: BQ smoke");
        return false;
    }
    k_nano::slog_kai!("SGDB", "ok", "Q3 PASS: BQ smoke");

    // Q4: motor único NSGDB — put/get L1 + L4 BQ top_k
    if !nsgdb_bridge::nsgdb_is_ready() {
        nsgdb_bridge::nsgdb_init();
    }
    let d = MemoryDoc::new(MemoryLayer::L1Working, "smoke", b"sgdb".to_vec());
    let put_ok = nsgdb_bridge::put_doc_nsgdb(d).is_ok();
    let got = nsgdb_bridge::get_doc_nsgdb(MemoryLayer::L1Working, "smoke")
        .ok()
        .flatten();
    let l1_ok = put_ok
        && got.map(|doc| doc.payload.as_slice() == b"sgdb").unwrap_or(false)
        && nsgdb_bridge::ram_len_nsgdb() > 0;
    if !l1_ok {
        k_nano::slog_kai!("SGDB", "fail", "Q4 FAIL: NSGDB put/get L1");
        return false;
    }
    let mut floats = Vec::new();
    for x in [1.0f32, -1.0, 1.0, -1.0] {
        floats.extend_from_slice(&x.to_le_bytes());
    }
    let mut d4 = MemoryDoc::new(MemoryLayer::L4Semantic, "emb1", floats);
    d4.bitvec = Some(quantize_f32(&[1.0, -1.0, 1.0, -1.0]));
    if nsgdb_bridge::put_doc_nsgdb(d4).is_err() {
        k_nano::slog_kai!("SGDB", "fail", "Q4 FAIL: NSGDB put L4");
        return false;
    }
    let (hits, _path) = layers::recall_semantic(&[1.0, -1.0, 1.0, -1.0], 1);
    if hits.len() != 1 {
        k_nano::slog_kai!("SGDB", "fail", "Q4 FAIL: L4 top_k via NSGDB");
        return false;
    }
    k_nano::slog_kai!("SGDB", "ok", "Q4 PASS: NSGDB motor único + BQ");

    // Q5: remember_exchange + prompt_slice
    layers::remember_exchange("ping", "pong");
    let _ = layers::prompt_slice(512);
    k_nano::slog_kai!("SGDB", "ok", "Q5 PASS: remember_exchange + prompt_slice");

    // Q6: HANR put/get
    if store::ready() {
        if store::put_hanr("demo", "ok").is_err() {
            k_nano::slog_kai!("SGDB", "fail", "Q6 FAIL: HANR put");
            return false;
        }
        match store::get_hanr("demo") {
            Ok(Some(s)) if s == "ok" => {}
            _ => {
                k_nano::slog_kai!("SGDB", "fail", "Q6 FAIL: HANR get mismatch");
                return false;
            }
        }
        k_nano::slog_kai!("SGDB", "ok", "Q6 PASS: HANR");
    } else {
        k_nano::slog_kai!("SGDB", "warn", "Q6 SKIP: store not ready");
    }

    // Q7: mini-bench 128/64
    let (b_ok, msg) = bench::bench_smoke(128, 64);
    if b_ok {
        k_nano::slog_kai!("SGDB", "ok", "Q7 PASS: bench 128/64 ({})", msg);
    } else {
        k_nano::slog_kai!("SGDB", "fail", "Q7 FAIL: bench 128/64 ({})", msg);
    }
    b_ok
}

/// Ponte entre SleepCycle PRUNE e R3 replay: flushes SGDB memory state so
/// the next `cortex::r3::update_with_replay()` cycle reads fresh cached traces.
/// The actual GRPO/R3 update lives in cortex; this is a coordination signal
/// between memory tiers (SGDB) and routing replay (R3 MoE).
pub fn update_with_replay() {
    if !ready() {
        k_nano::slog_kai!("SGDB", "warn", "update_with_replay: SGDB not ready — skip");
        return;
    }
    // Flush ephemeral L0/L1 docs so R3 replay can pull from persisted
    // working memory rather than stale arena cache.
    let flushed = prune_working_ram();
    k_nano::slog_kai!(
        "SGDB",
        "ok",
        "update_with_replay: flushed {} RAM docs → R3 replay ready",
        flushed
    );
}

pub fn status_line() -> alloc::string::String {
    metrics::report_line()
}
