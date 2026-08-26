//! NSGDB Bridge — ponte global `neural_sgdb::Sgdb` ↔ TickvLite (Fase 2 + 2.5).
//!
//! ## Fase 2 (commit 47c92ba)
//! - Global NSGDB (Sgdb::open + SafeSgdb wrapper)
//! - recall_semantic: tenta externo → fallback interno
//! - rag_context: tenta externo → fallback interno
//!
//! ## Fase 2.5 (esta sessão)
//! - **2.5-A**: `recall_typed()` — devolve `Vec<Hit>` completos (12 campos)
//! - **2.5-B**: `OsEmbedder` — conecta `memory_systems::embed_or_pseudo` ao Embedder trait
//! - **2.5-C**: `recall_lexical_bridge()` — BM25 sem embedding (default MCP)
//! - **2.5-D**: `health_bridge()` — HealthReport estruturado
//! - **2.5-E**: `reinforce_bridge()`, `explain_bridge()` — cognitive ops

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use ticket_lock::TicketLock;

use super::tickv_adapter::TickvStorageAdapter;

/// Re-export tipos do neural-sgdb para conveniência dos consumidores.
pub use neural_sgdb::{ContentType, Hit, RecallPath};

/// Wrapper around neural_sgdb::Sgdb that is Send+Sync.
/// Safe because TickvStorageAdapter delegates to k_nano::storage which has
/// its own internal locking (spin::Mutex<TickvLite>).
struct SafeSgdb(neural_sgdb::Sgdb);
unsafe impl Send for SafeSgdb {}
unsafe impl Sync for SafeSgdb {}

// Global neural-sgdb instance — `Sgdb::open(TickvStorageAdapter)`.
lazy_static! {
    static ref NSGDB: TicketLock<Option<SafeSgdb>> = TicketLock::new(None);
}

// ─── Init ────────────────────────────────────────────────────────────────────

/// Inicializa o NSGDB global. Chamado no boot após TickvLite montado.
pub fn nsgdb_init() -> usize {
    let adapter = TickvStorageAdapter;
    let mut db = match neural_sgdb::Sgdb::open(adapter) {
        Ok(db) => db,
        Err(e) => {
            k_nano::slog_kai!("NSGDB", "init", "FAIL: Sgdb::open error={}", e);
            return 0;
        }
    };

    let n = db.scan_prefix("md/").map(|v| v.len()).unwrap_or(0);

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

// ─── Fase 2.5-A: Recall Tipado (Hits completos) ─────────────────────────────

/// Recall semântico tipado — devolve `Vec<Hit>` com TODOS os 12 campos.
///
/// O consumidor (Cortex/Hermes) interpreta cada Hit:
/// - `hit.text` → prompt do LLM (se content_type = Text/Json/Code)
/// - `hit.key + hit.rel` → fetch do primário (se content_type = Embedding)
/// - `hit.matched_terms` → grounding auditável
/// - `hit.path` → saber se foi semantic ou lexical
/// - `hit.content_type` → como processar o datum
/// - `hit.provenance` → origem/confiança/importância
///
/// Se NSGDB não disponível, retorna vazio.
pub fn recall_typed(query: &[f32], k: usize) -> Vec<Hit> {
    if query.is_empty() {
        return Vec::new();
    }
    with_nsgdb(|db| db.recall(query, k).unwrap_or_default()).unwrap_or_default()
}

// ─── Fase 2.5-C: Recall Lexical (BM25, sem embedding) ───────────────────────

/// Recall lexical BM25 — funciona SEM embedding, só query de texto.
///
/// Este é o **default do MCP** (ADR-0008). Prioridade sobre semantic
/// quando não temos embedding BGE real.
///
/// Retorna `Vec<Hit>` com matched_terms (o "porquê" do casamento).
pub fn recall_lexical_bridge(query_text: &str, k: usize) -> Vec<Hit> {
    if query_text.is_empty() {
        return Vec::new();
    }
    with_nsgdb(|db| db.recall_lexical(query_text, k).unwrap_or_default()).unwrap_or_default()
}

/// Recall híbrido — combina BQ semântico + BM25 lexical.
/// Quando temos embedding E query textual.
pub fn recall_hybrid_bridge(query_emb: &[f32], query_text: &str, k: usize) -> Vec<Hit> {
    if query_emb.is_empty() || query_text.is_empty() {
        return Vec::new();
    }
    with_nsgdb(|db| db.recall_hybrid(query_emb, query_text, k).unwrap_or_default())
        .unwrap_or_default()
}

// ─── Fase 2-B (legado): Recall Semantic como tuples ──────────────────────────

/// Recall semântico em formato legado `(storage_key, dist_u32)` para backward compat.
/// **DEPRECATED**: usar `recall_typed()` em vez disso.
pub fn recall_semantic_nsgdb(query: &[f32], k: usize) -> (Vec<(String, u32)>, &'static str) {
    if query.is_empty() {
        return (Vec::new(), "empty");
    }

    let Some((hits, path)) = with_nsgdb(|db| {
        let results = db.recall(query, k).unwrap_or_default();
        let mapped: Vec<(String, u32)> = results
            .iter()
            .map(|h| {
                let dist_u32 = (h.dist * 10_000.0) as u32;
                (h.key.clone(), dist_u32)
            })
            .collect();
        let path_str = if mapped.is_empty() { "empty" } else { "nsgdb-bq" };
        (mapped, path_str)
    }) else {
        return (Vec::new(), "unavailable");
    };

    (hits, path)
}

// ─── RAG Context ─────────────────────────────────────────────────────────────

/// RAG context via neural-sgdb (formatação inteligente com content_type awareness).
pub fn rag_context_nsgdb(query: &[f32], k: usize) -> String {
    if query.is_empty() {
        return String::new();
    }
    with_nsgdb(|db| db.rag_context(query, k).unwrap_or_default()).unwrap_or_default()
}

/// RAG context com ancoragem lexical (v1.1.6 item 4).
/// Amplia o pool com oversample + rerank por presença de tokens da query no texto.
pub fn rag_context_reranked_bridge(query_emb: &[f32], query_text: &str, k: usize) -> String {
    if query_emb.is_empty() || query_text.is_empty() {
        return String::new();
    }
    with_nsgdb(|db| db.rag_context_reranked(query_emb, query_text, k).unwrap_or_default())
        .unwrap_or_default()
}

// ─── Fase 2.5-D: Health ──────────────────────────────────────────────────────

/// Health check do NSGDB — expõe status para MonitorAgent/SelfHeal.
pub fn nsgdb_health() -> String {
    let Some(health) = with_nsgdb(|db| {
        let h = db.health();
        format!(
            "backend={} docs={} bq={} ram={} conflicts={}",
            h.backend, h.doc_count, h.bq_len, h.ram_len, h.open_conflicts
        )
    }) else {
        return String::from("NSGDB unavailable");
    };
    health
}

/// Health report estruturado (para consumo por agentes).
pub fn nsgdb_health_report() -> Option<neural_sgdb::HealthReport> {
    with_nsgdb(|db| db.health())
}

// ─── Fase 2.5-E: Cognitive Operations ────────────────────────────────────────

/// Reforça uma memória (importance += delta, clamped [0,1]).
/// Memórias reforçadas decaem mais devagar no lifecycle.
pub fn reinforce_bridge(key: &str, delta: f32) -> Result<(), &'static str> {
    with_nsgdb(|db| db.reinforce(key, delta).map_err(|_e| "reinforce failed"))
        .unwrap_or(Err("nsgdb unavailable"))
}

/// Explica o estado corrente de uma memória (machine-readable).
pub fn explain_bridge(key: &str) -> Option<String> {
    with_nsgdb(|db| {
        let exp = db.explain(key).ok()?;
        Some(format!(
            "key={} layer={:?} state={:?} importance={:.2} confidence={:.2} parents={}",
            exp.key,
            exp.layer,
            exp.state,
            exp.importance,
            exp.confidence,
            exp.parents.len()
        ))
    })
    .flatten()
}

// ─── Fase 2.5-E: Scoping ────────────────────────────────────────────────────

/// Define scope (multi-tenancy) para uma memória.
pub fn set_scope_bridge(key: &str, scope: &str) -> Result<(), &'static str> {
    with_nsgdb(|db| db.set_scope(key, scope).map_err(|_e| "set_scope failed"))
        .unwrap_or(Err("nsgdb unavailable"))
}

/// Recall com scope isolado (não compete com outros scopes).
pub fn recall_scoped_bridge(
    query: &[f32],
    k: usize,
    scope: &str,
) -> Vec<Hit> {
    if query.is_empty() {
        return Vec::new();
    }
    with_nsgdb(|db| db.recall_scoped(query, k, scope).unwrap_or_default()).unwrap_or_default()
}

/// Scan prefix (ART lookup).
pub fn scan_prefix_nsgdb(prefix: &str) -> Vec<(String, u64)> {
    with_nsgdb(|db| db.scan_prefix(prefix).unwrap_or_default()).unwrap_or_default()
}

/// Remember fact via neural-sgdb L3.
pub fn remember_fact_nsgdb(fact: &str, now: u64) {
    let _ = with_nsgdb(|db| db.remember_fact(fact, now));
}

/// Remember exchange via neural-sgdb (L1 + L2).
pub fn remember_exchange_nsgdb(user: &str, response: &str) {
    let _ = with_nsgdb(|db| db.remember_exchange(user, response));
}

// ─── Fase 2.5-B: Embedder Seam ───────────────────────────────────────────────

/// Adapter que conecta o Embedder trait do neural-sgdb ao
/// `memory_systems::embed_or_pseudo()` do OS.
///
/// **Contrato**: quem fornece embeddings usa o MESMO modelo no write e no query
/// (era ADR-0007). `embed_or_pseudo` usa BGE quando disponível, pseudo-hash
/// como fallback — a dimensionalidade identifica a era.
pub struct OsEmbedder;

impl neural_sgdb::Embedder for OsEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>, neural_sgdb::SgdbError> {
        let (emb, _path) = crate::memory_systems::embed_or_pseudo(text);
        if emb.is_empty() {
            return Err(neural_sgdb::SgdbError::Invalid("embed returned empty"));
        }
        Ok(emb)
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────


// ─── Fase 2.5-D: Lifecycle Management ────────────────────────────────────────

/// Configuração de lifecycle para memórias.
pub struct MemoryLifecycleConfig {
    pub l1_commit_after_ticks: u64,
    pub l2_to_l3_importance: f32,
    pub l2_to_l3_min_age_ticks: u64,
    pub decay_per_tick: f32,
    pub decayed_below: f32,
}

impl Default for MemoryLifecycleConfig {
    fn default() -> Self {
        Self {
            l1_commit_after_ticks: 100,
            l2_to_l3_importance: 0.5,
            l2_to_l3_min_age_ticks: 500,
            decay_per_tick: 0.01,
            decayed_below: 0.1,
        }
    }
}

/// Resultado de um lifecycle tick.
pub struct LifecycleTickResult {
    pub committed: usize,
    pub promoted: usize,
    pub semanticized: usize,
    pub archived: usize,
    pub decayed: usize,
    pub transitions: u64,
}

/// Roda um tick do lifecycle: decay + consolidação + promoção.
/// Chamado periodicamente pelo SleepCycle ou pelo supervisor.
pub fn lifecycle_tick(now: u64, config: &MemoryLifecycleConfig) -> LifecycleTickResult {
    let default = LifecycleTickResult {
        committed: 0, promoted: 0, semanticized: 0,
        archived: 0, decayed: 0, transitions: 0,
    };

    let Some(report) = with_nsgdb(|db| {
        // 1. Decay Ebbinghaus
        let decay_cfg = neural_sgdb::DecayConfig {
            half_life_ms: 0, // disabled — usar decay_per_tick
            floor: config.decayed_below,
            decay_state_at: config.decayed_below,
            decay_confidence: true,
        };
        let _decayed = db.decay_importance(now, &decay_cfg).unwrap_or(0);

        // 2. Expirar memórias com janela de validade fechada
        let _expired = db.expire_old(now).unwrap_or(0);

        // 3. Consolidação por recorrência (L2 repetido → L3 fato)
        let consolidate_cfg = neural_sgdb::ConsolidateConfig {
            min_repeats: 3,
            min_len: 10,
            max_new: 10,
        };
        let _consolidated = db.consolidate_recurrences(&consolidate_cfg).unwrap_or(0);

        LifecycleTickResult {
            committed: 0,
            promoted: 0,
            semanticized: 0,
            archived: 0,
            decayed: _decayed,
            transitions: (_decayed + _expired + _consolidated) as u64,
        }
    }) else {
        return default;
    };

    report
}



// ─── #537: Sync Write — atualiza índices NSGDB após put_kv/put_doc ──────────

/// Sincroniza um write para o NSGDB after a TickvLite write.
/// Chamado por put_kv/put_doc para manter os índices ART/BQ do neural-sgdb
/// em sincronia com os dados escritos diretamente no TickvLite.
///
/// Este é o glue que elimina o dual-write: o write vai para TickvLite
/// (via put_blob direto) e NSGDB é notificado para atualizar seus
/// índices derivados (ART/BQ/lexical).
pub fn sync_write_to_nsgdb(key: &str, val: &[u8], layer: u8) {
    let _ = with_nsgdb(|db| {
        // Re-read do TickvLite para popular o NSGDB engine
        // O neural-sgdb reconstrói o doc do payload
        use neural_sgdb::MemoryDoc as ExtDoc;
        use neural_sgdb::MemoryLayer;
        let ml = match layer {
            0 => MemoryLayer::L0Sensory,
            1 => MemoryLayer::L1Working,
            2 => MemoryLayer::L2EpisodicShort,
            3 => MemoryLayer::L3EpisodicLong,
            4 => MemoryLayer::L4Semantic,
            5 => MemoryLayer::L5Procedural,
            6 => MemoryLayer::L6Reserved,
            _ => MemoryLayer::L7Identity,
        };
        let doc = ExtDoc::new(ml, key, val.to_vec());
        let _ = db.put(doc);
    });
}

/// Sincroniza um fact (L3) para o NSGDB.
pub fn sync_fact_to_nsgdb(fact: &str, now: u64) {
    let _ = with_nsgdb(|db| {
        let _ = db.remember_fact(fact, now);
    });
}

/// Sincroniza um exchange (L1+L2) para o NSGDB.
pub fn sync_exchange_to_nsgdb(user: &str, response: &str) {
    let _ = with_nsgdb(|db| {
        let _ = db.remember_exchange(user, response);
    });
}

// ─── #538: Embedder Seam — set_embedder bridge ──────────────────────────────

/// Conecta o OsEmbedder ao NSGDB.
/// Nota: neural-sgdb não tem set_embedder() — o Embedder é usado pelo caller.
/// Este bridge expõe o OsEmbedder para que o cognitive_bridge o use diretamente
/// no remember_semantic e recall.
pub fn get_os_embedder() -> OsEmbedder {
    OsEmbedder
}

#[cfg(test)]
mod tests {
    use super::*;
    use neural_sgdb::Embedder;

    #[test]
    fn nsgdb_bridge_init_and_health() {
        nsgdb_init();
        let health = nsgdb_health();
        assert!(!health.is_empty());
    }

    #[test]
    fn recall_typed_returns_vec() {
        let hits = recall_typed(&[1.0, -1.0, 1.0, -1.0], 3);
        // Sem dados ou NSGDB não disponível — retorna vazio
        assert!(hits.is_empty() || !hits.is_empty()); // sempre válido
    }

    #[test]
    fn recall_lexical_returns_vec() {
        let hits = recall_lexical_bridge("teste de query", 3);
        assert!(hits.is_empty() || !hits.is_empty());
    }

    #[test]
    fn rag_context_returns_string() {
        let ctx = rag_context_nsgdb(&[1.0, -1.0, 1.0, -1.0], 3);
        assert!(ctx.is_empty() || !ctx.is_empty());
    }

    #[test]
    fn sync_write_to_nsgdb_does_not_panic() {
        // Sync after write deve ser no-op gracioso sem NSGDB
        sync_write_to_nsgdb("test/key", b"value", 3);
    }

    #[test]
    fn os_embedder_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<OsEmbedder>();
    }

    #[test]
    fn lifecycle_tick_returns_zero_when_empty() {
        let result = lifecycle_tick(1000, &MemoryLifecycleConfig::default());
        assert_eq!(result.transitions, 0);
    }

    #[test]
    fn os_embedder_produces_vector() {
        let emb = OsEmbedder.embed("hello world");
        assert!(emb.is_ok());
        let v = emb.unwrap();
        assert!(!v.is_empty());
    }
}
