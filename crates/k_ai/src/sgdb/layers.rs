//! ADR-0063 F6 — ponte Hermes/Cortex ↔ camadas MemoryDoc L0–L7.
//! Não substitui TF-IDF (0064) nem BGE; acrescenta working/episodic no SGDB.

use alloc::string::String;
use alloc::vec::Vec;

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

/// Prefixo de prompt a partir de docs L1/L2 recentes (TickvLite se montado).
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
            Ok(Some(doc)) => {
                core::str::from_utf8(&doc.payload)
                    .map(|s| String::from(s))
                    .unwrap_or_default()
            }
            _ => String::new(),
        })
        .unwrap_or_default();
        if text.is_empty() {
            continue;
        }
        let line = alloc::format!("  {}={}\n", key, clamp(&text, 120));
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

/// Indexa descrição de skill em L3 (ART) — ADR-0064 F4 skills-RAG leve.
pub fn index_skill(name: &str, description: &str) {
    ensure_ready();
    let _ = with_engine(|e| {
        let key = alloc::format!("skill:{}", name);
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
