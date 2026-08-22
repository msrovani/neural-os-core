# SESSION_284 — Migração neural-sgdb Externos como Substrato de Memória

**Objetivo:** Migrar o SGDB interno (k_ai::sgdb) para usar o neural-sgdb v1.1.11 externo como substrato de memória cognitiva para agentes (ADR-0091).

**Commits:** 9 (`179b001` → `7a42935`)  
**LOC:** ~1.150  
**Testes:** 118/120 (2 preexistentes)  
**Regressão:** zero

---

## Fases Implementadas

### Fase 0+1 — Dependência + Adapter (commit `179b001`)
- neural-sgdb v1.1.11 via junction `crates/neural-sgdb` → `C:\DEV\neural-sgdb`
- `default-features = false` (no_std compatível)
- `TickvStorageAdapter`: implementa `neural_sgdb::Storage` delegando ao TickvLite
- 2 testes host: put/get roundtrip + Sgdb::open com adapter

### Fase 2 — NSGDB Bridge (commit `47c92ba`)
- `nsgdb_bridge.rs`: global `NSGDB` (SafeSgdb wrapper Send+Sync)
- `recall_semantic`: tenta neural-sgdb externo → fallback engine interno
- `rag_context`: tenta neural-sgdb externo → fallback engine interno
- `boot_init`: inicializa NSGDB após TickvLite montado
- 3 testes host: init+health, recall vazio, rag vazio

### Fase 2.5 — Adequações de Memória (commits `90132a3` + `a765161`)
- **2.5-A:** `recall_typed()` — devolve `Vec<Hit>` com 12 campos completos
- **2.5-B:** `OsEmbedder` — conecta `embed_or_pseudo()` ao Embedder trait
- **2.5-C:** `recall_lexical_bridge()` — BM25 sem embedding (default MCP)
- **2.5-D:** `lifecycle_tick()` — decay + expire + consolidate
- **2.5-E:** `set_scope_bridge()` + `recall_scoped_bridge()` — multi-agente
- **2.5-F:** `reinforce_bridge()` + `explain_bridge()` — cognitive ops

### Fase 3.0 — Cortex/Hermes Memory-Aware (commits `a765161` + `4fc8889`)
- **3.0-A:** `gated_rag_context()` reescrito para consumir Hits tipados
- **3.0-B:** `memory_aware_route()` — routing por content_type
- **3.0-C:** `sgdb_agent cmd_recall` com recall_lexical (default MCP)
- **3.0-D:** `lifecycle_tick` integrado ao SleepCycleAgent CONSOLIDATE

---

## Lições Críticas

1. **neural-sgdb não é KV — é substrato de memória.** O `Hit` tem 12 campos (key, text, dist, path, content_type, payload_type, score, matched_terms, validity, rel, provenance, score_breakdown). Reduzir a `(String, u32)` perde 80% da informação.

2. **recall_lexical é o default do MCP (ADR-0008).** Funciona SEM embedding — só query de texto. O semantic recall requer Embedder com MESMO modelo no write e query.

3. **`Sgdb` do neural-sgdb NÃO é genérico.** Usa `Box<dyn Storage>` internamente. `Send` requer wrapper `unsafe impl Send + Sync` (justificado: TickvStorageAdapter delega a k_nano::storage que tem locking interno).

4. **Workspace members não podem ficar fora do root.** Junction `crates/neural-sgdb` → `C:\DEV\neural-sgdb` resolve sem ser workspace member.

5. **ContentType awareness muda o output do RAG.** Embedding/Binary não são prosa — skip. Json/Text/Code renderizam verbatim. O output formatado `[MEMORY-RECALL] #1) [sem] [TXT] d=42%` dá ao LLM informação sobre COMO cada hit foi recuperado.

6. **Lifecycle management é determinístico.** `MemoryLifecycle::tick()` usa `now: u64` explícito — sem wall clock, sem background thread. Chamado no SleepCycleAgent fase CONSOLIDATE.

7. **OsEmbedder conecta BGE ao Embedder seam.** `embed_or_pseudo()` fornece embeddings — BGE quando disponível, pseudo-hash como fallback. A dimensionalidade identifica a era (era ADR-0007).

8. **Dual-write durante migração.** Writes vão para TickvLite E neural-sgdb ART/BQ (redundante mas seguro). Fase 3 (migrar 75 callers) elimina a redundância.

---

## Arquivos Modificados

| Arquivo | LOC | O que |
|---------|-----|-------|
| `k_ai/src/sgdb/tickv_adapter.rs` | 133 | TickvStorageAdapter |
| `k_ai/src/sgdb/nsgdb_bridge.rs` | 370 | NSGDB Bridge completo |
| `k_ai/src/sgdb/mod.rs` | +2 | pub mod tickv_adapter + nsgdb_bridge |
| `k_ai/src/sgdb/store.rs` | +1 | nsgdb_init() no boot_init |
| `k_ai/src/sgdb/layers.rs` | +12 | recall_semantic + rag_context bridge |
| `k_ai/Cargo.toml` | +1 | neural-sgdb dependency |
| `hermes/src/cognitive_bridge.rs` | +69 | memory_aware_route + gated_rag_context reescrito |
| `hermes/src/agents.rs` | +16 | lifecycle_tick no SleepCycleAgent |
| `hermes/src/sgdb_agent.rs` | +33 | cmd_recall com recall_lexical |
| `Cargo.toml` | +1 | (removido — junction resolve) |
| `docs/architecture/0091-neural-sgdb-migration.md` | 405 | ADR completa |

---

## Verificação

| Check | Resultado |
|-------|-----------|
| `cargo check -p k_ai` | ✅ 0 erros |
| `cargo check -p hermes` | ✅ 0 erros |
| `cargo check --release -p neural-kernel --target x86_64-unknown-none` | ✅ 0 erros |
| `cargo test -p k_ai -- nsgdb_bridge` | ✅ 6/6 PASS |
| `cargo test --workspace` | ✅ 118/120 (2 preexistentes) |

---

## Próximos

- **Fase 3 restante:** Migrar 75 callers de `k_ai::sgdb::put_kv/get_kv` para neural-sgdb
- **QEMU boot:** Validar em runtime o lifecycle_tick e recall lexical
- **neural-sgdb 1.2+:** Verificar se `set_embedder()` será adicionado upstream
