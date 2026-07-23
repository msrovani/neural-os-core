# SESSION_174 — SGDB Quality Jump (Q1–Q5)

**Data:** 2026-07-23  
**Plano:** `sgdb_quality_jump` (não editar o plan file)

## Q1 TickvLite
- Stats `live/dead/corrupt/compactions`; GC `compact()`; recover avança em CRC bad.
- Smokes: `gc_smoke`, `corrupt_smoke` (+ `power_loss` existente).
- Boot logs `[TICKV] gc|corrupt|stats`.

## Q2 Índices
- ART Node4/16/48/256 + `delete` tombstone.
- `rebuild_indices_from_tickv` no `boot_init`.
- BQ flat contíguo + `hamming` POPCNT se `allow_avx2`, senão scalar.
- Bench 10k/1k em `metrics_report`.

## Q3 Audit AUD2
- Flush com signature 64B + prev_hash; load restaura; `verify_chain` no subset.

## Q4 MemoryDocView
- Overlay parse NMD1 sem clonar payload; `demo()` valida view.

## Q5 Docs
- ADR-0063: Aceite intermediário vs DoD pleno.
- INDEX/STATE/IDEA parcial #491–#505.

## Gate
- `cargo check --release -p k_ai -p hermes -p neural-kernel --features fat-boot-log` = 0
