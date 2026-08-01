# k_ai/src/fs — codemap

**Responsibility:** cognitive filesystem agents — virtual files generated on demand and memory-tier placement decisions.

**Key symbols:**
- `inference_fs_agent.rs` — `InferenceFsAgent` implements `k_nano::fs::FilesystemAgent`, mounted at `/inference/`; `read()` synthesizes content from tick/state, `write()` buffers training examples (`TRAINING_BUF`, cap 100).
- `mhi_scheduler.rs` — `mhi_scheduler_tick(tick)` scans `k_nano::mhi::MHI_REGISTRY` every 1000 ticks; promotes hot allocations (access_count > 5 in 500 ticks) / demotes idle (>5000 ticks) via `arc_suggest_tier`; CFS-like fairness counters. Called from OptimizerAgent tick.

**Integration:** `FilesystemAgent` trait from k_nano; scheduler consumes MHI registry owned by k_nano, weights from `crate::profile::ProfileManager`.
