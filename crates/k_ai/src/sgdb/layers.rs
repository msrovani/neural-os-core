//! ADR-0063 F6/E2 — ponte Hermes/Cortex ↔ camadas MemoryDoc L0–L7.
//! Não substitui TF-IDF (0064) nem BGE; acrescenta working/episodic + recall BQ L4.
//!
//! **s410h — motor único:** toda escrita/leitura passa pelo NSGDB externo
//! (`neural-sgdb` via `nsgdb_bridge`/`store`). O `AiosDatabaseEngine` interno
//! (ART/BQ/RAM arena duplicada) foi eliminado — uma verdade só (ADR-0063 §cut).
//! `ensure_ready` é no-op de compat: o lifecycle do motor fica em
//! `store::boot_init` / `boot_init_deferred` (K33[28] soft-hang honesty).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use super::bq::quantize_f32;
use super::memory_doc::{MemoryDoc, MemoryLayer};

/// Compat: era "cria o engine interno se ausente". Motor único → lifecycle
/// no boot (`store::boot_init`); nada a fazer aqui. Mantida para não tocar
/// todos os callers (store.rs, e2e, tests).
pub fn ensure_ready() {}

/// Pós-turno: L1 working (user) + L2 episódico curto (assistant).
/// s385b/s410h: via `put_doc` (NSGDB motor único + CRDT) — não contornar store.
pub fn remember_exchange(user: &str, response: &str) {
    ensure_ready();
    let u = MemoryDoc::new(
        MemoryLayer::L1Working,
        "last_user",
        user.as_bytes().to_vec(),
    );
    let _ = super::store::put_doc(u);
    let a = MemoryDoc::new(
        MemoryLayer::L2EpisodicShort,
        "last_asst",
        response.as_bytes().to_vec(),
    );
    let _ = super::store::put_doc(a);
    super::nsgdb_bridge::sync_exchange_to_nsgdb(user, response);
}

/// Indexa embedding L4 (BQ). Aceita BGE ou pseudo; `emb` vazio = no-op.
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
    let _ = super::store::put_doc(doc);
    let _ = text;
}

/// Recall L4: NSGDB externo (BQ + BM25/ART do neural-sgdb).
/// path = `nsgdb-bq` | `empty` (motor único — sem fallback interno dual-truth).
pub fn recall_semantic(query: &[f32], k: usize) -> (Vec<(String, u32)>, &'static str) {
    super::nsgdb_bridge::recall_semantic_nsgdb(query, k)
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
    let _ = super::store::put_doc(doc);
    super::nsgdb_bridge::sync_fact_to_nsgdb(fact, ts);
}

/// Prefixo de prompt a partir de docs L1/L2 recentes (get via NSGDB).
pub fn prompt_slice(max_chars: usize) -> String {
    ensure_ready();
    let mut out = String::from("[SGDB-L1/L2]\n");
    let mut n = out.len();
    let layers = [
        (MemoryLayer::L1Working, "last_user"),
        (MemoryLayer::L2EpisodicShort, "last_asst"),
    ];
    for (layer, key) in layers {
        let text = match super::nsgdb_bridge::get_doc_nsgdb(layer, key) {
            Ok(Some(doc)) => core::str::from_utf8(&doc.payload)
                .map(|s| String::from(s))
                .unwrap_or_default(),
            _ => String::new(),
        };
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

/// Pós-turno completo: texto L1/L2 constantes + L2 timestamped + L4 BQ temporal.
/// s385b/s410h: todos os puts via `put_doc` (motor único NSGDB + CRDT).
pub fn remember_exchange_full(
    user: &str,
    response: &str,
    emb_u: &[f32],
    emb_a: &[f32],
    tick: u64,
) {
    ensure_ready();
    let ts = MemoryDoc::sortable_ts_key(tick);
    let ts_u = alloc::format!("{}/u", ts);
    let ts_a = alloc::format!("{}/a", ts);

    // L1/L2 constant keys (prompt_slice compat)
    let _ = super::store::put_doc(MemoryDoc::new(
        MemoryLayer::L1Working,
        "last_user",
        user.as_bytes().to_vec(),
    ));
    let _ = super::store::put_doc(MemoryDoc::new(
        MemoryLayer::L2EpisodicShort,
        "last_asst",
        response.as_bytes().to_vec(),
    ));

    // L2 timestamped text (acumula para recall RAG)
    let _ = super::store::put_doc(MemoryDoc::new(
        MemoryLayer::L2EpisodicShort,
        &ts_u,
        user.as_bytes().to_vec(),
    ));
    let _ = super::store::put_doc(MemoryDoc::new(
        MemoryLayer::L2EpisodicShort,
        &ts_a,
        response.as_bytes().to_vec(),
    ));

    // L4 timestamped embeddings
    remember_semantic(&ts_u, user, emb_u);
    remember_semantic(&ts_a, response, emb_a);

    // NSGDB remember_exchange (episódico tipado / lexical)
    super::nsgdb_bridge::sync_exchange_to_nsgdb(user, response);
}

/// RAG context: NSGDB (content_type aware); vazio → formata hits do recall
/// buscando o texto L2 irmão por storage key.
/// path = `recall_semantic`.
pub fn rag_context(query: &[f32], k: usize) -> String {
    let ext_ctx = super::nsgdb_bridge::rag_context_nsgdb(query, k);
    if !ext_ctx.is_empty() {
        return ext_ctx;
    }
    let (hits, _path) = recall_semantic(query, k);
    if hits.is_empty() {
        return String::new();
    }
    let header = alloc::format!("[SGDB-RAG top-{}]\n", hits.len());
    let mut out = String::new();
    for (i, (sk, dist)) in hits.iter().enumerate() {
        let text = match text_for_storage_key(sk) {
            Some(t) if !t.is_empty() => t,
            _ => continue,
        };
        let line = alloc::format!("  #{}) d={} {}\n", i + 1, dist, clamp(&text, 200));
        out.push_str(&line);
    }
    if out.is_empty() {
        return String::new();
    }
    alloc::format!("{}{}", header, out)
}

/// Busca payload-texto para um storage key de hit (`md/L4/x` → doc por (layer,key)).
fn text_for_storage_key(sk: &str) -> Option<String> {
    // Formato canônico `md/Lx/<key>` → parse layer + key e fetch no motor único.
    let rest = sk.strip_prefix("md/")?;
    if rest.len() < 3 {
        return None;
    }
    let b = rest.as_bytes();
    if b[0] != b'L' || b.get(2) != Some(&b'/') {
        return None;
    }
    let layer_num = (b[1] as char).to_digit(10)?;
    let key = &rest[3..];
    let layer = match layer_num {
        0 => MemoryLayer::L0Sensory,
        1 => MemoryLayer::L1Working,
        2 => MemoryLayer::L2EpisodicShort,
        3 => MemoryLayer::L3EpisodicLong,
        4 => MemoryLayer::L4Semantic,
        5 => MemoryLayer::L5Procedural,
        6 => MemoryLayer::L6Reserved,
        _ => MemoryLayer::L7Identity,
    };
    // Texto irmão: mesmo key na camada L2 (hits L4 guardam embedding;
    // o texto do exchange vive em L2 com o mesmo key timestamped).
    match super::nsgdb_bridge::get_doc_nsgdb(layer, key) {
        Ok(Some(doc)) => core::str::from_utf8(&doc.payload).map(String::from).ok(),
        _ => None,
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

/// Indexa descrição de skill em L3 (motor único — via put_doc NSGDB).
pub fn index_skill(name: &str, description: &str) {
    ensure_ready();
    let key = format!("skill:{}", name);
    let doc = MemoryDoc::new(
        MemoryLayer::L3EpisodicLong,
        &key,
        description.as_bytes().to_vec(),
    );
    let _ = super::store::put_doc(doc);
}

/// Lookup ART por prefixo de storage key — delega ao ART do NSGDB externo.
pub fn art_prefix(prefix: &str) -> Vec<(String, u64)> {
    ensure_ready();
    super::nsgdb_bridge::scan_prefix_nsgdb(prefix)
}
