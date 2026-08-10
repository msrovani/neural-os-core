# SESSION_257 — neural-sgdb Maturation Sprint v0.3 (2026-08-10)

**Escopo:** Maturação do crate comunitário neural-sgdb (repo separado) de
"memory substrate" funcional para base robusta/determinística/durável/medível,
pronta para a próxima fase arquitetural (Memory Lifecycle, L6, Graph, CRDT
per-layer).
**Status:** ✅ Fechada — 14 commits (`efaa742..96cac70`, +1.475/−87 LOC),
matriz DoD verde, revisão independente (ora-1) aplicada.
**Repo:** github.com/msrovani/neural-sgdb — commits `24aacda`..`96cac70`.

---

## 1. Fases executadas (spec da sprint, 28 seções)

| Fase | Entregue | Evidência |
|---|---|---|
| **P1 Baseline** | `cargo test --no-default-features` QUEBRAVA (30 erros: testes usavam prelude std + backends sem gate) → corrigido: imports alloc, gates `#[cfg(feature="file-storage")]`, mcp_server por feature | commit 24aacda |
| **P2 Correctness** | VectorClock semântico (happens_before/concurrent/merge/PartialEq por mapa); CRDT `MergeVerdict` + `conflicts` (multi-value, self-packet, `own_writes`); FileStorage recovery endurecido (bounds, le32 sem unwrap, tombstone-before-bound, CRC); parsing safety (rd_u32/rd_u64); recall determinístico (dedupe overwrite + tie-break) | b6d2186..dec04bb |
| **P3 Performance** | BQ top-k bounded heap O(N·D/64 + N log k) — bench heap(k=5)=320µs vs full-sort(k=N)=592µs | 9172c8c |
| **P4 Persistence** | Durability explícita (Buffered/Flushed/Durable + fsync); FileStorage `compact()` atômico; `Sgdb::rebuild_indices` público | a06709b..856c83b |
| **P5 Memory semantics** | `MemoryState` (Active/Superseded/Archived/Invalidated) **sem quebrar NMD1** (side-table `sys/state/` via Storage cru); validação de layer central; node_id explícito | 8b953fa |
| **P6 Validation** | Fuzz adversarial determinístico (decode/view/scan/CRDT nunca panics); revisão independente ora-1 → 5 fixes (HIGH tombstone truncado panicava; MED tombstone sem CRC; MED CRDT conflito causal infinito; LOW compact vlen=0; LOW set_state log) | 92b676f, ef668e5 |

## 2. Bugs reais pegos (não contornados)

1. **Baseline**: testes no_std incompatíveis (format!/vec!/std sem import/gate) — 30 erros.
2. **FileStorage tombstone CRÍTICO**: vlen=u32::MAX era tratado como length absurdo antes do bound → chave deletada RESSUSCITAVA no reopen (fix: TOMBSTONE antes do bound).
3. **HIGH (revisão)**: tombstone truncado (klen prometido, key cortada) → slice sem bounds → **panic** no open (o caso de crash-tail que o hardening devia cobrir).
4. **MED (revisão)**: `has_other_state` usava `local_version` (adotado de peers) → sucessor causal do mesmo peer virava Conflict para sempre, mesh nunca convergia (fix: `own_writes`).
5. **BQ overwrite**: recall devolvia a mesma memória 2x (BQ re-insere sem remover id antigo) — dedupe por storage key.
6. **VectorClock igualdade**: PartialEq derivado comparava slots por posição — dois relógios com mesma causalidade em ordem de inserção diferente eram "desiguais" (fix: igualdade por mapa).

## 3. Revisão independente (ora-1)

Segundo passe do oracle sobre o diff da sprint: 1 HIGH + 2 MED + 3 LOW, todos
confirmados empiricamente e aplicados em `ef668e5`. Áreas verificadas corretas:
scan_volume tombstone in-place, BQ heap (Ord direção), recall dedupe, VectorClock,
MemoryState side-table (não polui `md/`), parsing safety, feature isolation.

## 4. Verificação final (DoD)

- `cargo test`: **66+1** · `--no-default-features`: **44+1** · `--features p2p`: **75+1**
- `cargo check --no-default-features --target x86_64-unknown-none`: **limpo**
- `cargo check` std-only / std+file-storage: **limpos** · examples build: **ok**
- Compatibilidade: API pública preservada (aditiva); NMD1/TKLV byte-idênticos ao
  OS (MemoryState não serializado); zero deps novas; sem threads/async.

## 5. Benchmarks (before/after)

| Medida | Antes | Depois |
|---|---|---|
| BQ top-k 10k×1024 | full sort O(N log N) | heap(k=5)=320µs vs full-sort(k=N)=592µs |
| recall@5 baseline | tautológico (100% falso) | honesto: 0% em dados sintéticos (sign-BQ diverge de cosseno em ruído; rescore FP32 existe p/ isso) |

## 6. Dívida técnica / deferidos

- `TickvFile` GC/compaction + TKCK checkpoint no backend (v0.3+)
- CRDT per-layer merge policy (multi-value pleno) — conflitos preservados e expostos, resolução do caller
- MemoryDelta/MemorySnapshot = abstrações futuras (harmless)
- VectorClock 8 nós fixo (v0.2: dinâmico)
- L6 Associative + Memory Graph, consolidation engine, residual representation (próxima fase — docs/architecture/ 01-06 já existem)

## 7. Referências

- Spec: prompt "Maturation Sprint v0.3" (28 seções, DoD)
- Docs: `docs/architecture/01..06` (Memory Model, Lifecycle, Retrieval, Distributed, Storage, Cognitive API)
- Repo: github.com/msrovani/neural-sgdb @ 96cac70
