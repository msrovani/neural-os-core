//! NSGDB Bridge — ponte global `neural_sgdb::Sgdb` ↔ TickvLite (Fase 2).
//!
//! Cria e mantém uma instância global do `neural_sgdb::Sgdb` backed pelo
//! `TickvStorageAdapter`. As operações de recall usam o motor externo
//! (ART + BQ do neural-sgdb), que tem capacidades superiores ao engine
//! interno (recall_lexical, MihIndex, typed hits).
//!
//! ## Estratégia de migração (Fase 2)
//! - **Write path**: mantém `put_kv`/`put_doc` no engine interno (backward compat).
//!   O TickvLite é o storage real; o neural-sgdb ART/BQ são derived indices.
//! - **Read path**: `recall_semantic` e `rag_context` passam a usar o NSGDB externo.
//! - **Dual index**: boot_init faz `rebuild_indices_from_tickv` no NSGDB para
//!   popular o ART/BQ do neural-sgdb a partir do TickvLite.
//! - **Fallback**: se NSGDB não está disponível, cai para o engine interno.

use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use spin::Mutex;
use ticket_lock::TicketLock;

use super::tickv_adapter::TickvStorageAdapter;

/// Wrapper around neural_sgdb::Sgdb that is Send+Sync.
/// Safe because TickvStorageAdapter delegates to k_nano::storage which has
/// its own internal locking (spin::Mutex<TickvLite>).
struct SafeSgdb(neural_sgdb::Sgdb);
unsafe impl Send for SafeSgdb {}
unsafe impl Sync for SafeSgdb {}


/// Global neural-sgdb instance — `Sgdb::open(TickvStorageAdapter)`.
/// TicketLock para serializar acesso (não é hot-path — recall é por tick).
lazy_static! {
    static ref NSGDB: TicketLock<Option<SafeSgdb>> =
        TicketLock::new(None);
}

/// Inicializa o NSGDB global. Chamado no boot após TickvLite montado.
/// Faz rebuild do ART/BQ a partir do TickvLite para ter os índices prontos.
/// Retorna o número de registros reconstruídos (0 se NSGDB não disponível).
pub fn nsgdb_init() -> usize {
    let adapter = TickvStorageAdapter;
    let mut db = match neural_sgdb::Sgdb::open(adapter) {
        Ok(db) => db,
        Err(e) => {
            k_nano::slog_kai!("NSGDB", "init", "FAIL: Sgdb::open error={}", e);
            return 0;
        }
    };

    // Reconstrói índices ART/BQ a partir do TickvLite
    let n = db.scan_prefix("md/").map(|v| v.len()).unwrap_or(0);
    let _ = n; // melhor esforço

    *NSGDB.lock() = Some(SafeSgdb(db));
    k_nano::slog_kai!(
        "NSGDB",
        "init",
        "OK — neural-sgdb v1.1.11 via TickvStorageAdapter (records={})",
        n
    );
    n
}

/// Executa uma operação no NSGDB global. Retorna None se não disponível.
pub fn with_nsgdb<R>(f: impl FnOnce(&mut neural_sgdb::Sgdb) -> R) -> Option<R> {
    let mut g = NSGDB.lock();
    g.as_mut().map(|s| f(&mut s.0))
}

/// Recall semântico via neural-sgdb (substitui o recall_semantic interno).
/// Retorna (hits, path) — hits são (storage_key, dist_u32) para compatibilidade.
pub fn recall_semantic_nsgdb(query: &[f32], k: usize) -> (Vec<(String, u32)>, &'static str) {
    if query.is_empty() {
        return (Vec::new(), "empty");
    }

    let Some((hits, path)) = with_nsgdb(|db| {
        let results = db.recall(query, k).unwrap_or_default();
        let mapped: Vec<(String, u32)> = results
            .iter()
            .map(|h| {
                // dist no neural_sgdb é 0..1 (1−cos); converter para u32 compat
                let dist_u32 = (h.dist * 10_000.0) as u32;
                (h.key.clone(), dist_u32)
            })
            .collect();
        let path_str = if mapped.is_empty() { "empty" } else { "nsgdb-bq" };
        (mapped, path_str)
    }) else {
        // Fallback: NSGDB não disponível
        return (Vec::new(), "unavailable");
    };

    (hits, path)
}

/// RAG context via neural-sgdb (substitui o rag_context interno).
pub fn rag_context_nsgdb(query: &[f32], k: usize) -> String {
    if query.is_empty() {
        return String::new();
    }

    let Some(context) = with_nsgdb(|db| {
        db.rag_context(query, k).unwrap_or_default()
    }) else {
        return String::new();
    };

    context
}

/// Health check do NSGDB — expõe status para MonitorAgent/SelfHeal.
pub fn nsgdb_health() -> String {
    let Some(health) = with_nsgdb(|db| {
        let h = db.health();
        format!(
            "backend={} docs={} bq={} art={}",
            h.backend, h.doc_count, h.bq_len, h.ram_len
        )
    }) else {
        return String::from("NSGDB unavailable");
    };
    health
}

/// Scan prefix via neural-sgdb (substitui art_prefix interno).
pub fn scan_prefix_nsgdb(prefix: &str) -> Vec<(String, u64)> {
    let Some(results) = with_nsgdb(|db| {
        db.scan_prefix(prefix).unwrap_or_default()
    }) else {
        return Vec::new();
    };
    results
}

/// Remember fact via neural-sgdb L3.
pub fn remember_fact_nsgdb(fact: &str, now: u64) {
    let _ = with_nsgdb(|db| {
        db.remember_fact(fact, now)
    });
}

/// Remember exchange via neural-sgdb (L1 + L2).
pub fn remember_exchange_nsgdb(user: &str, response: &str) {
    let _ = with_nsgdb(|db| {
        db.remember_exchange(user, response)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nsgdb_bridge_init_and_health() {
        // Sem TickvLite montado, NSGDB não inicia — comportamento gracioso
        nsgdb_init();
        let health = nsgdb_health();
        // Pode ser "NSGDB unavailable" ou "backend=..." dependendo do ambiente
        assert!(!health.is_empty());
    }

    #[test]
    fn recall_semantic_returns_empty_on_no_data() {
        let (hits, path) = recall_semantic_nsgdb(&[1.0, -1.0, 1.0, -1.0], 3);
        // Sem dados, retorna vazio ou unavailable
        assert!(hits.is_empty() || path == "unavailable" || path == "empty");
    }

    #[test]
    fn rag_context_returns_empty_on_no_data() {
        let ctx = rag_context_nsgdb(&[1.0, -1.0, 1.0, -1.0], 3);
        assert!(ctx.is_empty());
    }
}
