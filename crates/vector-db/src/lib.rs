#![no_std]
#![allow(dead_code)]

extern crate alloc;
use alloc::vec;

// ─── vector-db: In-kernel TF-IDF Vector Store for RAG ──────────────────────
// ADR-0064: Lightweight no_std vector database using TF-IDF embeddings
// and cosine similarity. No external dependencies beyond spin + alloc.
//
// Design:
//   - Vocabulary is built incrementally as documents are inserted.
//   - TF-IDF computed via compute_tfidf_from_indices for each document.
//   - Search does O(N * V) cosine similarity (N = docs, V = vocab size).
//   - JSON serialization omits embeddings (rebuilt on deserialize).
//
// ponytail:
//   - Linear search (no ANN index). Add HNSW or IVF when N > 10k docs.
//   - Dense f32 vectors. Quantize to f16/i8 when memory > 10MB.
//   - Single Mutex (readers block writers). Switch to RwLock if contention.

pub mod tokenize;
pub mod tfidf;
pub mod store;
pub mod json;

pub use store::{VectorStore, VectorEntry};
pub use tfidf::{cosine_similarity, ln_f32, sqrt_f32, compute_tfidf, compute_tfidf_from_indices};
pub use tokenize::tokenize;
pub use json::{to_json, from_json};

/// Self-test: exercises tokenize, tfidf, store, search, json round-trip.
///
/// Returns `true` if all checks pass. Can be called from kernel init
/// to verify the vector DB is functional.
pub fn demo() -> bool {
    let mut store = VectorStore::new();
    let mut m = alloc::collections::BTreeMap::new();
    m.insert("agent".into(), "test".into());

    store.insert("Rust bare metal OS development", m.clone());
    store.insert("Python scripting and data science", m.clone());
    store.insert("Bare metal kernel development in Rust", m.clone());

    let results = store.search("Rust kernel", 2);
    if results.is_empty() {
        return false;
    }
    let entries = store.all_entries();
    if results[0].1 >= entries.len() {
        return false;
    }
    let top_text = entries[results[0].1].text.as_str();
    if !top_text.contains("Rust") && !top_text.contains("kernel") {
        return false;
    }

    // cosine identity
    let v = vec![1.0_f32, 2.0, 3.0];
    if (cosine_similarity(&v, &v) - 1.0).abs() > 0.001 {
        return false;
    }

    // cosine orthogonal
    if cosine_similarity(&[1.0, 0.0], &[0.0, 1.0]).abs() > 0.001 {
        return false;
    }

    // tokenize
    let t = tokenize::tokenize("Hello World test");
    if t.len() < 2 {
        return false;
    }

    // json round-trip
    let json = json::to_json(&store);
    if json.is_empty() {
        return false;
    }
    let restored = match json::from_json(&json) {
        Ok(s) => s,
        Err(_) => return false,
    };
    if restored.len() != store.len() {
        return false;
    }

    true
}
