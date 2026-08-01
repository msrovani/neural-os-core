# k_ai/src/sgdb — codemap

**Responsibility:** ADR-0063/0064 cognitive path database — namespaced KV/doc store over `k_nano::storage` (TickvLite) + RAM arena, with ART radix and BQ flat-Hamming indices and HANR/episodic/RAG layer API. The self-heal checkpoint also persists here (`sys/checkpoint`).

**Key symbols:**
- `store.rs` — facade `put_kv/get_kv/put_doc/get_doc/put_hanr/get_hanr/put_pkg_meta/put_pkg_body/put_skill_blob/ready/boot_init/checkpoint_working/prune_working_ram/predict_all_pci`; `ns` namespace consts (`hanr/ md/ pkg/ skill/ audit/ vdb/ sys/`); gates on `k_nano::storage::is_ready`.
- `engine.rs` — `AiosDatabaseEngine` (static `ENGINE`): L0/L1 → RAM arena (indexed ART/BQ, `id_to_sk`), L2+ → TickvLite `md/Lx/key`; `bq_top_k_f32` recall; `init_global`/`with_engine`.
- `memory_doc.rs` — `MemoryDoc`/`MemoryDocView` binary encode (L0–L7, `VectorClock`).
- `layers.rs` — cognitive API: `remember_fact`, `remember_semantic`, `remember_exchange(_full)`, `recall_semantic`, `rag_context`, `prompt_slice`, `index_skill`, `ensure_ready`.
- `art.rs` / `bq.rs` / `hamming_dispatch.rs` — Node4/16/48/256 radix index (leaf tombstones); BQ flat index + `hamming`/`quantize_f32`; scalar/AVX2/AVX-512 hamming dispatcher (`select_best_hamming_kernel`).
- `crdt_sync.rs` — ADR-0081 C4 CRDT memory sync (vector-clock merge).
- `metrics.rs` / `bench.rs` / `e2e_smoke.rs` — `metrics_report`, TSC micro-bench, L1 put→checkpoint→prune→remount→get smoke.

**Integration:** consumed by hermes (`cognitive_bridge`, `memory_store`, `sgdb_agent`, `package_hub`, net/wifi/skill persistence) and bin (`boot_init`, `demo`, `predict_all_pci`); `update_with_replay()` coordinates with `cortex::r3` replay (SleepCycle PRUNE).
