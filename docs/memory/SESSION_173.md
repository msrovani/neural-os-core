# SESSION_173 — Adoção SGDB como store cognitivo do AIOS

**Data:** 2026-07-23  
**Plano:** `sgdb_aios_adoption` (não editar o plan file)  
**Premissa:** emagreçer bin; FAT/NeuralFS para blobs; SGDB para cognitivo/KV.

## Onda A — Facade
- [`k_ai/sgdb/store.rs`](../../crates/k_ai/src/sgdb/store.rs): `put_kv`/`get_kv`, `put_hanr`/`get_hanr`, pkg/skill/vdb, `boot_init`, `ready`/`backend`.
- Boot: `boot_init` + demo facade; load `vdb/blob` via `get_vdb_blob`.

## Onda B — HANR
- [`hermes/memory_store.rs`](../../crates/hermes/src/memory_store.rs): read SGDB→hydrate VFS; write SGDB+VFS best-effort.
- Log `[sgdb] hanr … sgdb=… vfs=…`.

## Onda C — Audit / Episodic / Skills
- `AuditTrail::flush_to_sgdb` / `load_from_sgdb` (`audit/head` AUD1).
- `EpisodicMemory` → MemoryDoc L2 + `sys/episodic_tail` (não mais “NVMe” mentiroso).
- `skill_opt::promote` + PackageHub skill → `put_skill_blob`.
- `after_exchange`: `put_vdb_blob` + flush audit.

## Onda D — PackageHub
- Meta sempre `pkg/{kind:name}`; body VFS se ok; senão TickvLite ≤4KiB.
- `persist_backend`: `none|sgdb|vfs|both` (honesty).

## Fora
- WiFi CFG, firmware, models, BOOT.LOG, TrustCache.

## Gate
- `cargo check --release -p k_ai -p hermes -p neural-kernel --features fat-boot-log` = 0
