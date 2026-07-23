# SESSION_172 — Plano unificado ADR-0063 + ADR-0064 (ondas 0–5)

**Data:** 2026-07-23  
**Premissa:** emagreçer bin — lógica em crates.

## Onda 0 — Docs
- Títulos ADR-0063 (SGDB) / ADR-0064 (RAG) corrigidos; INDEX/STATE/`fazendo`; IDEA #486/#487/#491–#505; cross-links.

## Onda 1 — `vector-db`
- Crate [`crates/vector-db`](../../crates/vector-db): tokenize EN+PT, TF-IDF, cosine, `demo()`, persist binário `NVDB`.
- Re-export `cortex::vector_db`; boot `[vectordb] demo PASS|FAIL`.

## Onda 2 — Flash + TickvLite
- [`k_nano/storage/flash.rs`](../../crates/k_nano/src/storage/flash.rs): `FlashController` sobre `NVME_DRIVER` (LBA 2048+) ou RAM 1MB.
- [`tickv.rs`](../../crates/k_nano/src/storage/tickv.rs): TickvLite append-log + CRC; smoke put/get.

## Onda 3 — Ponte
- Boot: load `vdb/blob` de TicKV → `global_load_bytes`.
- Persist schema binário (sem serde_json).

## Onda 4 — Hermes
- `cortex_system_prompt`: TF-IDF RAG primeiro, BGE fallback.
- `after_exchange`: `rag_remember` + `put_blob("vdb/blob")`.

## Onda 5 — SGDB F2–F5 MVP (`k_ai::sgdb`)
- Stub `sgdb_residual` removido → módulo [`crates/k_ai/src/sgdb/`](../../crates/k_ai/src/sgdb/):
  - **F2** `memory_doc.rs`: Magic `NMD1`, L0–L7, VectorClock, encode/decode
  - **F3** `engine.rs`: `AiosDatabaseEngine` put/get ↔ TickvLite + ART/BQ
  - **F4** `art.rs`: ART lite Node4/Node16 (honesty: sem Node48/256)
  - **F5** `bq.rs`: quantize f32→bits, Hamming popcnt, `top_k`
- Boot: `[sgdb] F2-F7 demo PASS|FAIL` após TickvLite smoke.

## F6–F8 (continuação Onda 5)
- **F6** `layers.rs` + Hermes: `remember_exchange` L1/L2; `prompt_slice` no `cortex_system_prompt`; `index_skill` L3.
- **F7** `bench.rs`: micro-bench ART 64 + BQ 32 no `demo()`.
- **F8** `power_loss_smoke`: put → drop TickvLite → remount/recover → get `pl/test`.
- Residual: embeddings BGE no VectorStore; AVX2 BQ dedicado; GC TicKV; ART Node48/256.

## Gate
- `cargo check --release -p k_ai` / `-p hermes` / `-p neural-kernel --features fat-boot-log` = 0
