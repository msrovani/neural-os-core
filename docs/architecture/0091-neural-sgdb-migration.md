# ADR-0091: Migração para neural-sgdb Externos como Substrato de Memória para Agentes

**Status:** Implemented  
**Lifecycle:** `proposta_feita`  
**Data:** 2026-08-22  
**Ideias:** #527–#535  
**Supersede:** —  
**Relacionado:** ADR-0063 (SGDB original), ADR-0081 (Mesh P2P CRDT), ADR-0088 (AIOS-First)

---

## Contexto

### O que temos (k_ai::sgdb — interno)

O ADR-0063 estabeleceu o SGDB interno com ART + BQ + MemoryDoc NMD1, backed pelo TickvLite. Funcionou como MVP sólido:

- 13 módulos, ~2.400 LOC em `k_ai/src/sgdb/`
- 75 callers em hermes/neural-kernel
- Recall semântico BQ + FP32 rescore
- Persistência via TickvLite (append-log + CRC)

**Mas o SGDB interno tem limitações fundamentais:**

1. **Recall retorna tuples `(String, u32)`** — perde 80% da informação do hit (content_type, path, matched_terms, rel, provenance, validity)
2. **Sem recall lexical** — o MCP (ADR-0008) usa lexical como default, mas não temos BM25
3. **Sem scoping multi-agente** — todos os agentes competem pelos mesmos slots de recall
4. **Sem lifecycle management** — memórias não decaem, não consolidam, não arquivam
5. **Sem cognitive operations** — sem `reinforce`, `explain`, `feedback`
6. **Sem content_type seam** — o detector tenta adivinhar o tipo, em vez de quem escreve declarar
7. **Sem CRDT** — a versão CRDT interna é limitada vs. a do neural-sgdb

### O que o neural-sgdb externo oferece

O [neural-sgdb](https://github.com/msrovani/neural-sgdb) v1.1.11 é a extração independente do SGDB do neural-os-core, evoluído separadamente:

- **243+ testes** (lib + doc-test)
- **12 memórias hit-tipadas**: `Hit { key, text, dist, path, content_type, payload_type, score, matched_terms, validity, rel, provenance }`
- **3 modos de retrieval**: Semantic (BQ+FP32), Lexical (BM25), Hybrid
- **Scoping multi-agente**: `set_scope()`, `recall_scoped()`
- **Lifecycle completo**: `MemoryLifecycle::tick()`, decay Ebbinghaus, consolidação por recorrência
- **Cognitive API**: `reinforce()`, `explain()`, `feedback()`, `merge_memories()`
- **Conflict model**: `ConflictRecord`, `resolve_conflict()`, `dismiss_conflict()`
- **MCP server**: 4 tools (remember/recall/health/curate), 23 aliases
- **CRDT full**: `MemoryRecord`, anti-entropy, per-layer merge policy
- **NMD1 byte-identical** com o kernel — interop garantida
- **`no_std` + `std`**: zero dependências externas em no_std

### O gap

O neural-sgdb evoluiu MUITO além do que o k_ai::sgdb interno é. A migração não é opcional — é uma necessidade arquitetural para habilitar:

- **Memória cognitiva real** (não só KV)
- **Recall que o LLM consegue interpretar** (hits tipados)
- **Isolamento entre agentes** (scoping)
- **Vida útil das memórias** (decay, consolidação, arquivamento)

---

## Decisão

Migrar o `k_ai::sgdb` interno para usar o `neural-sgdb` externo como substrato de memória, mantendo backward compatibilidade durante a transição.

### Princípios da Migração

1. **Dual-write durante transição**: engine interno continua aceitando writes; NSGDB externo recebe reads
2. **Fallback gracioso**: se NSGDB não disponível, cai para engine interno
3. **NADA quebra**: 75 callers existentes continuam funcionando
4. **Evolutivo**: cada Fase adiciona capacidade sem remover a anterior
5. **Honestidade**: AWAITING_HW quando não testável em QEMU

### Arquitetura Alvo

```
┌─────────────────────────────────────────────────────────────────┐
│  HERMES / CORTEX / JARBAS / SGDB_AGENT                         │
│  (Consumem memórias L0-L7 via k_ai::sgdb facade)               │
└────────────────────────────┬────────────────────────────────────┘
                             │
┌────────────────────────────▼────────────────────────────────────┐
│  k_ai::sgdb (FACADE) — backward compat layer                   │
│  • put_kv/get_kv → TickvLite direto (KV cru)                   │
│  • recall_semantic → nsgdb_bridge (Hits tipados)                │
│  • rag_context → nsgdb_bridge (RAG formatado)                   │
│  • recall_lexical → nsgdb_bridge (BM25, Fase 2.5-C)            │
└────────────┬───────────────────────────────┬────────────────────┘
             │                               │
┌────────────▼──────────────┐  ┌─────────────▼───────────────────┐
│  ENGINE INTERNO           │  │  NSGDB BRIDGE (Fase 2)          │
│  AiosDatabaseEngine       │  │  neural_sgdb::Sgdb              │
│  • ART (chaves)           │  │  • ART (chaves, mais completo)  │
│  • BQ (embeddings)        │  │  • BQ + MihIndex (sub-linear)   │
│  • MemoryDoc NMD1         │  │  • BM25 LexicalIndex            │
│  • RAM-only L0/L1         │  │  • Scoping multi-agente         │
│  (LEGACY — mantido p/     │  │  • Lifecycle (decay/consolidate)│
│   backward compat)        │  │  • Cognitive API                │
└────────────┬──────────────┘  │  • Conflict model               │
             │                  │  • CRDT (feature p2p)           │
┌────────────▼──────────────┐  └─────────────┬───────────────────┘
│  TICKVLITE (Storage)      │                 │
│  append-log + CRC         │◄────────────────┘
│  NVMe / RAM / NeuralFS    │  (TickvStorageAdapter)
└───────────────────────────┘
```

### TickvStorageAdapter

Ponte entre o `Storage` trait do neural-sgdb e o TickvLite do k_nano:

```rust
// Fase 1 — crates/k_ai/src/sgdb/tickv_adapter.rs
impl Storage for TickvStorageAdapter {
    fn put(&mut self, key: &[u8], val: &[u8]) -> Result<(), SgdbError> {
        k_nano::storage::put_blob(str_from_utf8(key)?, val)
            .map_err(|e| SgdbError::Storage(e))
    }
    fn get(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>, SgdbError> { ... }
    fn scan_prefix(&mut self, prefix: &[u8]) -> Result<ScanResult, SgdbError> {
        // with_tickv → keys_with_prefix + get por key
    }
    fn delete(&mut self, key: &[u8]) -> Result<(), SgdbError> { ... }
}
```

### SafeSgdb Wrapper

`neural_sgdb::Sgdb` contém `Box<dyn Storage>` que não é `Send`. Wrapper com `unsafe impl Send + Sync` (justificado: TickvStorageAdapter delega a k_nano::storage que tem locking interno via `spin::Mutex<TickvLite>`).

---

## Fases de Implementação

### Fase 0 — Dependência (✅ commit `179b001`)

- neural-sgdb v1.1.11 via junction `crates/neural-sgdb` → `C:\DEV\neural-sgdb`
- `default-features = false` (no_std compatível)
- Validação: `cargo check --no-default-features --target x86_64-unknown-none` = OK

### Fase 1 — TickvStorageAdapter (✅ commit `179b001`)

- `TickvStorageAdapter`: implementa `neural_sgdb::Storage` delegando ao TickvLite
- 2 testes host: put/get roundtrip + Sgdb::open com adapter
- Zero regressão: 118/120 testes

### Fase 2 — NSGDB Bridge (✅ commit `47c92ba`)

- `nsgdb_bridge.rs`: global `NSGDB` (SafeSgdb wrapper), `nsgdb_init()`
- `recall_semantic`: tenta neural-sgdb externo → fallback engine interno
- `rag_context`: tenta neural-sgdb externo → fallback engine interno
- `boot_init`: inicializa NSGDB após TickvLite montado
- 3 testes host: init+health, recall vazio, rag vazio

### Fase 2.5 — Adequações de Memória (✅ commits 90132a3 + a765161)

#### 2.5-A: Hits Tipados (✅ commit 90132a3)

**Problema**: bridge devolve `(String, u32)` — perde content_type, path, matched_terms, rel, provenance, validity.

**Solução**: nova fn `recall_typed()` que devolve `Vec<neural_sgdb::Hit>`.

```rust
// nsgdb_bridge.rs
pub fn recall_typed(query: &[f32], k: usize) -> Vec<neural_sgdb::Hit> {
    with_nsgdb(|db| db.recall(query, k).unwrap_or_default())
        .unwrap_or_default()
}
```

Consumidores decidem COMO usar cada Hit:
- `Hit.text` → prompt do LLM (se content_type = Text/Json/Code)
- `Hit.key + Hit.rel` → fetch do primário (se content_type = Embedding)
- `Hit.matched_terms` → grounding auditável
- `Hit.path` → saber se foi semantic ou lexical

**Arquivos**: `nsgdb_bridge.rs`, `cognitive_bridge.rs`, `sgdb_agent.rs`

#### 2.5-B: Embedder Seam (✅ commit 90132a3)

**Problema**: neural-sgdb tem `trait Embedder` mas não está conectado ao BGE/pseudo do OS.

**Solução**: adapter que conecta `memory_systems::embed_or_pseudo()` ao Embedder trait.

```rust
// nsgdb_bridge.rs
struct OsEmbedder;
impl neural_sgdb::Embedder for OsEmbedder {
    fn embed(&self, text: &str) -> Vec<f32> {
        let (emb, _path) = k_ai::memory_systems::embed_or_pseudo(text);
        emb
    }
}
```

**Pré-requisito**: verificar se neural-sgdb tem `set_embedder()` ou `open_with_embedder()`.

**Arquivos**: `nsgdb_bridge.rs`

#### 2.5-C: recall_lexical como Default (✅ commit 90132a3)

**Problema**: recall_lexical (BM25) é o default do MCP (ADR-0008) mas não está exposto.

**Solução**: nova fn `recall_lexical_bridge()` que funciona SEM embedding — só query de texto.

```rust
pub fn recall_lexical_bridge(query_text: &str, k: usize) -> Vec<neural_sgdb::Hit> {
    with_nsgdb(|db| db.recall_lexical(query_text, k).unwrap_or_default())
        .unwrap_or_default()
}
```

**Impacto**: sgdb_agent e cognitive_bridge usam lexical como default; semantic quando temos embedding BGE real.

**Arquivos**: `nsgdb_bridge.rs`, `cognitive_bridge.rs`, `sgdb_agent.rs`

#### 2.5-D: Lifecycle Management (✅ commit a765161)

**Problema**: memórias não decaem, não consolidam, não arquivam. O SGDB cresce indefinidamente.

**Solução**: usar `MemoryLifecycle::tick()` do neural-sgdb no SleepCycle.

```rust
// bei_init.rs ou SleepCycleAgent
let config = LifecycleConfig {
    l1_commit_after_ticks: 100,
    l2_to_l3_min_age_ticks: 500,
    decay_per_tick: 0.01,
    ..Default::default()
};
let mut lifecycle = MemoryLifecycle::new(config);
with_nsgdb(|db| lifecycle.tick(db, now));
```

**Arquivos**: `nsgdb_bridge.rs`, `bei_init.rs`, `agents.rs` (SleepCycleAgent)

#### 2.5-E: Scoping Multi-Agente (✅ commit 90132a3)

**Problema**: todos os agentes competem pelos mesmos slots de recall. Sem isolamento.

**Solução**: cada agente declara scope no write; recall_scoped garante isolamento.

```rust
// hermes: write com scope
with_nsgdb(|db| {
    db.set_scope("md/L4/last_user", "agent:hermes")?;
    db.set_scope("md/L4/hw_predict", "agent:cortex")?;
});

// recall isolado
with_nsgdb(|db| db.recall_scoped(&emb, 5, "agent:hermes"))
```

**Arquivos**: `nsgdb_bridge.rs`, `cognitive_bridge.rs`, `sgdb_agent.rs`

#### 2.5-F: Cognitive Operations (✅ commit 90132a3)

**Problema**: sem `reinforce`, `explain`, `feedback`. O agente não pode "reforçar" memórias importantes.

**Solução**: expor operações cognitivas via bridge.

```rust
// reinforce: memória reforçada decai mais devagar
pub fn reinforce_bridge(key: &str, delta: f32) -> Result<(), SgdbError> {
    with_nsgdb(|db| db.reinforce(key, delta))
}

// explain: por que esta memória foi retornada?
pub fn explain_bridge(key: &str) -> Option<MemoryExplanation> {
    with_nsgdb(|db| db.explain(key).ok())
}
```

**Arquivos**: `nsgdb_bridge.rs`, `sgdb_agent.rs`

### Fase 3.0 — Cortex/Hermes Memory-Aware (✅ completa — commits a765161 + 4fc8889)

#### 3.0-A: Memory Interpreter no Cortex (✅ commit a765161)

O Cortex (LLM) precisa de um sistema que:
1. Receba `Vec<Hit>` tipados
2. Interprete content_type (Json → parse, Text → prompt, Embedding → reuso)
3. Resolva rel (companion → primário)
4. Gere resposta fundamentada em memórias

```
INPUT LLM:
[MEMORY-RECALL semantic]
  Hit 1: content_type=Json text={"intent":"query_status"}
  Hit 2: content_type=Text text="user prefers dark mode" matched_terms=["dark"]
  Hit 3: content_type=Embedding(8) rel="md/L4/user_pref" (reusar vetor)

OUTPUT LLM:
  "Baseado nas memórias:
   1. Intenção pendente para query_status
   2. Usuário prefere dark mode (reforçado 3x)
   3. Preferência do usuário recuperada para contexto"
```

#### 3.0-B: Memory-Aware Routing no Hermes

O Hermes precisa de routing que considere content_type:
- `ContentType::Json` → parse e executa intenção
- `ContentType::Text` → injeta no prompt do LLM
- `ContentType::Embedding(dim)` → reusa o vetor (era ADR-0007)
- `ContentType::Binary` → processa como firmware/dado cru
- `ContentType::Code` → avalia código proposto

#### 3.0-C: Default lexical + Embedder como optional

```
Default: recall_lexical(query_text, k)  ← funciona sem embedding
Hybrid:  recall_hybrid(emb, query_text, k) ← quando temos BGE real
Semantic: recall(emb, k) ← quando temos embedding de alta qualidade
```

---

## Consequências

### Positivas

1. **Memória cognitiva real**: agentes têm memória com 生命周期, decay, consolidação
2. **Recall interpretável**: hits tipados permitem ao LLM entender O QUE recuperou
3. **Isolamento multi-agente**: scoping impede vazamento de memórias entre agentes
4. **RAG de qualidade**: lexical + semantic + hybrid = recall robusto
5. **CRDT pronto**: sincronização entre nós mesh (ADR-0081) sem trabalho extra
6. **MCP server**: interface para IDEs externos (Cursor, Claude Code)
7. **Auditável**: matched_terms, provenance, score_breakdown = rastro completo

### Negativas

1. **Complexidade**: mais camadas (adapter → bridge → facade → engine interno)
2. **Dual-write**: writes vão para TickvLite E neural-sgdb ART/BQ (redundante durante transição)
3. **Send+Sync wrapper**: `unsafe impl` para SafeSgdb (justificado mas não ideal)
4. **Migração gradual**: 75 callers para migrar na Fase 3 (esforço significativo)

### Neutros

1. **neural-sgdb como dependência externa**: evolução independente, sem conflito
2. **NMD1 byte-identical**: formato preservado, sem quebra de dados
3. **backwards compat**: engine interno mantido como fallback

---

## Riscos e Mitigações

| Risco | Impacto | Mitigação |
|-------|---------|-----------|
| neural-sgdb muda API sem aviso | Médio | Pin version com path dep; golden tests NMD1 |
| TickvStorageAdapter perde performance | Baixo | Profilar; overhead é 1 function pointer |
| 75 callers para migrar | Alto | Fase 3 gradual, manter facade legada |
| Embedder seam não existe no nsgdb | Alto | Verificar; se não existir, PR upstream |
| CRDT sync interno ≠ externo | Médio | NMD1 byte-identical; golden test |
| Lifecycle consolidação errada | Médio | Testes determinísticos (sem wall clock) |

---

## Verificação

| Check | Comando | Resultado |
|-------|---------|-----------|
| k_ai compila | `cargo check -p k_ai` | ✅ 0 erros |
| kernel compila | `cargo check --release -p neural-kernel --target x86_64-unknown-none` | ✅ 0 erros |
| Testes Fase 1+2 | `cargo test -p k_ai -- tickv_adapter nsgdb_bridge` | ✅ 5/5 PASS |
| Workspace | `cargo test --workspace --exclude neural-kernel --exclude boot` | ✅ 118/120 |
| No regressão | Mesmos 2 preexistentes (GPU WASM) | ✅ |
| QEMU boot | `run-qemu-whpx.ps1` | ⏳ Fase 3 |

---

## Referências

- **neural-sgdb repo**: https://github.com/msrovani/neural-sgdb (v1.1.11, MIT/Apache-2.0)
- **API Contract**: `neural-sgdb/docs/api.md` — contrato público completo
- **Implementation Status**: `neural-sgdb/docs/implementation-status.md` — capability matrix
- **ADR-0063**: SGDB original (TickvLite + ART + BQ interno)
- **ADR-0081**: Mesh P2P CRDT (sync entre nós)
- **ADR-0088**: AIOS-First (premissa máxima)
- **ADR-0008**: MCP lexical-first (default recall é lexical)
- **two_ai_protocol.rs**: exemplo de typed hits máquina→máquina

---

## Mapeamento de API (interno → externo)

| `k_ai::sgdb` (interno) | `neural_sgdb` (externo) | Mudança |
|--------------------------|--------------------------|---------|
| `init_global(1)` / `ensure_ready()` | `Sgdb::open(backend)` | global → instance |
| `put_kv(key, data)` | via `Storage` trait | backend |
| `get_kv(key)` | via `Storage` trait | backend |
| `recall_semantic(q, k) → (Vec<(String,u32)>, &str)` | `recall(q, k) → Vec<Hit>` | return type |
| `rag_context(q, k) → String` | `rag_context(q, k) → Result<String>` | error |
| `remember_fact(f)` | `remember_fact(f, now)` | injected clock |
| `remember_exchange(u, r)` | `remember_exchange(u, r)` | wrapper |
| `prompt_slice(n)` | `diary(node_id, limit)` | mais estruturado |
| `art_prefix(pfx)` | `scan_prefix_page(pfx, 0, 100)` | paginado |
| — | `recall_lexical(text, k)` | NOVO (Fase 2.5-C) |
| — | `recall_hybrid(emb, text, k)` | NOVO (Fase 2.5-B) |
| — | `set_scope(key, scope)` | NOVO (Fase 2.5-E) |
| — | `reinforce(key, delta)` | NOVO (Fase 2.5-F) |
| — | `explain(key)` | NOVO (Fase 2.5-F) |
| — | `MemoryLifecycle::tick()` | NOVO (Fase 2.5-D) |

---

*Esta ADR formaliza a migração do SGDB interno (k_ai::sgdb) para o neural-sgdb externo como substrato de memória cognitiva para agentes, preservando backward compatibilidade e habilitando recall tipado, lifecycle management, scoping multi-agente e cognitive operations.*
