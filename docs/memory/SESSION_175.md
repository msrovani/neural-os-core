# SESSION_175 — SGDB Vision Alignment (D1–D5)

**Data:** 2026-07-23  
**Plano:** `sgdb_vision_alignment` (não editar o plan file)

## Veredito

Visão TicKV+NoProto+ART+BQ permanece a decisão certa; stack ship = TickvLite/NMD1/ART48/hamming_dispatch. Sem port crates upstream nesta série.

## D1 Hamming dispatch
- `k_ai/sgdb/hamming_dispatch.rs`: `scalar | avx2_lut (VPSHUFB) | avx512f` (XOR ZMM + popcnt lanes).
- Boot: `select_best_hamming_kernel()` via `platform_probe`; log `[sgdb] hamming=…`.
- Wire em `bq.rs`; smoke 1024-dim.

## D2 L0/L1 RAM-only
- `engine.put`: L0/L1 → `ram_l0l1` BTreeMap; sem `put_blob` default.
- `checkpoint_l0l1()` HITL/SleepCycle.
- ART value = id lógico; key = `md/Lx/…` (estável pós-GC Tickv).

## D3 Tickv ckpt + stress
- `sys/tickv_ckpt` (TKCK + append + fnv + entries); mount tenta ckpt antes de full recover.
- `compact()` → `write_ckpt()`; `stress_gc_smoke` 1k overwrites → append bounded.
- Boot: `[TICKV] stress_gc`.

## D4 Bench D-series
- `bench_d_series`: ART 100k + BQ 10k × 1024-dim; TSC no serial.
- ADR-0063: seção Aceite D-series + Visão vs Ship.
- **Não** marca DoD 10M/100k.

## D5 Docs
- Esta SESSION; TECNOLOGIAS Hamming dispatch; IDEA #496/#501/#504 avançados.

## Gate
- `cargo check --release -p k-nano -p k_ai -p hermes -p neural-kernel --features fat-boot-log` = 0

## Residual (explícito)
- crates `tickv`/`noproto` oficiais; HNSW; SQL; AEAD; kill-9 HW; AVX-512 `vpopcntdq`; DoD 10M/100k.
