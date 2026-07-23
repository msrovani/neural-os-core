//! ADR-0063 F6/E2 — ponte Hermes/Cortex ↔ camadas MemoryDoc L0–L7.
//! Não substitui TF-IDF (0064) nem BGE; acrescenta working/episodic + recall BQ L4.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use super::bq::quantize_f32;
use super::engine::{with_engine, ENGINE};
use super::memory_doc::{MemoryDoc, MemoryLayer};

pub fn ensure_ready() {
    let mut g = ENGINE.lock();
    if g.is_none() {
        *g = Some(super::engine::AiosDatabaseEngine::new(1));
    }
}

/// Pós-turno: L1 working (user) + L2 episódico curto (assistant).
pub fn remember_exchange(user: &str, response: &str) {
    ensure_ready();
    let _ = with_engine(|e| {
        let u = MemoryDoc::new(
            MemoryLayer::L1Working,
            "last_user",
            user.as_bytes().to_vec(),
        );
        let _ = e.put(u);
        let a = MemoryDoc::new(
            MemoryLayer::L2EpisodicShort,
            "last_asst",
            response.as_bytes().to_vec(),
        );
        e.put(a)
    });
}

/// Indexa embedding L4 (BQ) quando BGE/latent disponível — não inventa dims.
pub fn remember_semantic(key: &str, text: &str, emb: &[f32]) {
    if emb.is_empty() {
        return;
    }
    ensure_ready();
    let mut payload = Vec::with_capacity(emb.len() * 4);
    for x in emb {
        payload.extend_from_slice(&x.to_le_bytes());
    }
    let mut doc = MemoryDoc::new(MemoryLayer::L4Semantic, key, payload);
    doc.bitvec = Some(quantize_f32(emb));
    let _ = with_engine(|e| e.put(doc));
    let _ = text;
}

/// Fato L3 (ART) — usado por memory_store::remember.
pub fn remember_fact(fact: &str) {
    ensure_ready();
    let ts = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
    let key = MemoryDoc::sortable_ts_key(ts);
    let doc = MemoryDoc::new(
        MemoryLayer::L3EpisodicLong,
        &key,
        fact.as_bytes().to_vec(),
    );
    let _ = with_engine(|e| e.put(doc));
}

/// Recall semântico L4 via BQ; path = `bq` | `empty`.
pub fn recall_semantic(query: &[f32], k: usize) -> (Vec<(String, u32)>, &'static str) {
    ensure_ready();
    if query.is_empty() {
        return (Vec::new(), "empty");
    }
    let Some((hits, n_bq)) = with_engine(|e| {
        let n = e.bq_len();
        let hits = e.bq_top_k_f32(query, k);
        let mut out = Vec::new();
        for (id, dist) in hits {
            if let Some(sk) = e.storage_key_of(id) {
                out.push((String::from(sk), dist));
            }
        }
        (out, n)
    }) else {
        return (Vec::new(), "empty");
    };
    if n_bq == 0 || hits.is_empty() {
        (hits, "empty")
    } else {
        (hits, "bq")
    }
}

/// Prefixo de prompt a partir de docs L1/L2 recentes.
pub fn prompt_slice(max_chars: usize) -> String {
    ensure_ready();
    let mut out = String::from("[SGDB-L1/L2]\n");
    let mut n = out.len();
    let layers = [
        (MemoryLayer::L1Working, "last_user"),
        (MemoryLayer::L2EpisodicShort, "last_asst"),
    ];
    for (layer, key) in layers {
        let text = with_engine(|e| match e.get(layer, key) {
            Ok(Some(doc)) => core::str::from_utf8(&doc.payload)
                .map(|s| String::from(s))
                .unwrap_or_default(),
            _ => String::new(),
        })
        .unwrap_or_default();
        if text.is_empty() {
            continue;
        }
        let line = format!("  {}={}\n", key, clamp(&text, 120));
        if n + line.len() > max_chars {
            break;
        }
        n += line.len();
        out.push_str(&line);
    }
    if out.len() <= "[SGDB-L1/L2]\n".len() {
        String::new()
    } else {
        out
    }
}

fn clamp(s: &str, max: usize) -> String {
    if s.len() <= max {
        String::from(s)
    } else {
        let mut t = String::from(&s[..max]);
        t.push('…');
        t
    }
}

/// Indexa descrição de skill em L3 (ART).
pub fn index_skill(name: &str, description: &str) {
    ensure_ready();
    let _ = with_engine(|e| {
        let key = format!("skill:{}", name);
        let doc = MemoryDoc::new(
            MemoryLayer::L3EpisodicLong,
            &key,
            description.as_bytes().to_vec(),
        );
        e.put(doc)
    });
}

/// Lookup ART por prefixo de storage key.
pub fn art_prefix(prefix: &str) -> Vec<(String, u64)> {
    ensure_ready();
    with_engine(|e| e.art.scan_prefix(prefix)).unwrap_or_default()
}
