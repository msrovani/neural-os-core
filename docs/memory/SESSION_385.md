# SESSION_385 — SGDB / Tickv / NoProto / BQ / ART / neural-sgdb bughunt

**Sprint:** v1.9.99-s385 TEST  
**Data:** 2026-09-21  
**Foco:** Honesty hunt + High→Low fixes no caminho cognitivo (ADR-0063/0091 + MCP doctrine)

## Premissas revalidadas

- TickvLite = KV persistente; ART+BQ = índices; neural-sgdb = substrato canónico de recall
- Lexical default (ADR-0008) quando semantic/BGE ausente
- Remember AIOS exige índices quentes — `HEAVY_DEFERRED` eterno = cognição morta
- tickv/noproto crates.io = **idea-only** (não deps); neural-sgdb 1.1.20 synced

## Canvas

`canvases/sgdb-tickv-nsgdb-bughunt-s385.canvas.tsx` — 7 High + 4 Med + 3 Low

## Correções aplicadas (ordem)

| ID | Fix |
|----|-----|
| H1 | `logical_key_for_nsgdb` — strip `md/Lx/` antes de ExtDoc::new |
| H2 | `put_doc` → sync_write + `crdt_record_change_global` |
| H3 | `gated_rag_context(text, emb)` semantic→lexical→engine |
| H4 | engine put L2+ sem Tickv → `Err("tickv not ready")` |
| H5 | `TickvLite.degraded` + `is_degraded()` + put warn + status `deg=` |
| H6 | `crdt_record_change_global` wired em put_kv/put_doc |
| H7 | `force_heavy_index_boot` no SleepCycle CONSOLIDATE |
| M1 | `sync_durable` no-op (≠ compact); Durability honesty |
| M2 | `memory_aware_route` lexical fallback |
| M3 | `checkpoint_working` compact só `backend=ram` |
| M4 | OsEmbedder slog pseudo uma vez |
| L* | noproto idea-only doc; dual ART comentário; BQ insert_1024 dim check |

## Gates

- `cargo check -p k-nano -p k_ai -p hermes --release` — 0 erros
- `cargo test -p k_ai --lib sgdb` — 18 pass / 1 ignored

## Upstream

| Comp | Ação |
|------|------|
| tickv 2.0.0 | idea-only — TickvLite in-tree |
| noproto 0.1.0 | idea-only — `k_nano::net::noproto` |
| neural-sgdb 1.1.20 | manter junction; era_report MCP OK |

## Residual

- ADR-0091 Fase 3: cortar dual-write (75 callers → NSGDB primário) — IDEA #537 ainda ⏳ cutover
- Aceite QEMU PHASE 6/7 + heavy SleepCycle em file backend
