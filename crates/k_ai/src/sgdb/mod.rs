//! ADR-0063 SGDB — F2–F7 + SgdbStore facade (adoção AIOS).

pub mod art;
pub mod bench;
pub mod bq;
pub mod engine;
pub mod layers;
pub mod memory_doc;
pub mod store;

pub use art::ArtIndex;
pub use bq::{hamming, quantize_f32, BqFlatIndex};
pub use engine::{
    init_global, remember_text, with_engine, AiosDatabaseEngine, ENGINE,
};
pub use layers::{art_prefix, ensure_ready, index_skill, prompt_slice, remember_exchange};
pub use memory_doc::{MemoryDoc, MemoryLayer, VectorClock};
pub use store::{
    backend, boot_init, get_doc, get_hanr, get_kv, get_pkg_body, get_pkg_meta, get_vdb_blob, ns,
    put_doc, put_hanr, put_kv, put_pkg_body, put_pkg_meta, put_skill_blob, put_vdb_blob, ready,
    status as store_status, with_store,
};

use alloc::vec::Vec;

/// Self-test F2–F5 (+ F7 micro-bench + facade KV se TickvLite pronto).
pub fn demo() -> bool {
    let mut doc = MemoryDoc::new(MemoryLayer::L1Working, "hello", b"world".to_vec());
    doc.clock.tick(1);
    let enc = doc.encode();
    let dec = match MemoryDoc::decode(&enc) {
        Ok(d) => d,
        Err(_) => return false,
    };
    if dec.key != "hello" || dec.payload.as_slice() != b"world" {
        return false;
    }

    let mut art = ArtIndex::new();
    art.insert("md/L1/a", 10);
    art.insert("md/L1/b", 20);
    art.insert("md/L2/c", 30);
    if art.get("md/L1/a") != Some(10) || art.get("md/L1/b") != Some(20) {
        return false;
    }
    if art.scan_prefix("md/L1/").len() < 2 {
        return false;
    }

    if !bq::smoke() {
        return false;
    }

    init_global(1);
    let ok = with_engine(|e| {
        let d = MemoryDoc::new(MemoryLayer::L1Working, "smoke", b"sgdb".to_vec());
        if e.put(d).is_err() {
            return false;
        }
        let mut floats = Vec::new();
        for x in [1.0f32, -1.0, 1.0, -1.0] {
            floats.extend_from_slice(&x.to_le_bytes());
        }
        let mut d4 = MemoryDoc::new(MemoryLayer::L4Semantic, "emb1", floats);
        d4.bitvec = Some(quantize_f32(&[1.0, -1.0, 1.0, -1.0]));
        if e.put(d4).is_err() {
            return false;
        }
        let hits = e.bq_top_k_f32(&[1.0, -1.0, 1.0, -1.0], 1);
        hits.len() == 1 && hits[0].1 == 0
    });
    if ok != Some(true) {
        return false;
    }

    layers::remember_exchange("ping", "pong");
    let _ = layers::prompt_slice(512);

    if store::ready() {
        if store::put_hanr("demo", "ok").is_err() {
            return false;
        }
        match store::get_hanr("demo") {
            Ok(Some(s)) if s == "ok" => {}
            _ => return false,
        }
    }

    let (b_ok, _) = bench::bench_smoke(64, 32);
    b_ok
}

pub fn status_line() -> alloc::string::String {
    let (b_ok, b_msg) = bench::bench_smoke(16, 8);
    alloc::format!(
        "SGDB facade={} backend={} bq={} bench={}",
        store::status(),
        engine::backend_note(),
        if bq::smoke() { "ok" } else { "fail" },
        if b_ok { b_msg.as_str() } else { "FAIL" }
    )
}
