# k_ai — Crate Map (Ring 2, Autonomy)

`no_std` autonomy layer: self-healing, trust, agency, cognitive database (SGDB), training, memory, inventory. Depends on **k_nano** (foundation: EVENT_BUS, SKILL_REGISTRY, storage, memory, interrupts) and **cortex** (BitNet ternary matmul, tensors, delta, HW Expert v4). No dependency on Ring 3 (hermes/jarbas). ~62 `.rs` files; entry point `src/lib.rs`.

## Responsibility

R2 autonomy: the OS heals itself (checkpoint restore, health-issue detection, VID-gated firmware/skill scanning), decides what agents/skills may run (trust cache with graduated enforcement), catalogs the agent fleet (Agency / AgentSpec), and maintains the cognitive path database (SGDB: HANR, audit, pkg meta, skills, episodic, RAG) plus on-device training (ternary fine-tuning, federated gradient sharing) and memory budgeting.

## Design Patterns

| Pattern | Where | Notes |
|---|---|---|
| **SelfHeal pipeline** | `self_heal.rs` + `self_heal_agent.rs` + `self_heal_disk.rs` | Detect → classify (`FailureClass::classify` / `classify_by_code`) → `analyze` → `RecoveryAction` (RestartDaemon / CreateSkill / CheckpointRestore / AwaitLLM) → EventBus. `Checkpoint` (bitmap + heap + CR3 + driver FNV-1a hash) serialized to SGDB `sys/checkpoint`; `restore_checkpoint` is best-effort (only frame-allocator bitmap restored; P09 pending for page tables). `SilentFailureDetector` flags agents that missed heartbeat within threshold. VID-gated scan (`run_vid_gated_scan`, `FW_KNOWN_VIDS`) publishes I3 (firmware) / I4 (skill) HEALTH_ISSUE only for known (VID,class) — never loads FW from R2. |
| **TrustCache** | `trust.rs` | `(token, agent, skill)` triples via `agent_skill_key`; `PermissionMode` (TotalAccess/AskEveryTime/Scoped), `PolicyState` graduated enforcement (Observe→Warn→Contain→Enforce, `escalate()` on violation), TTL entries + denylist + exempt tokens; `check_or_cache` does **not** auto-grant (P05); zero-trust `check_syscall` by `SyscallClass`; path confinement (`PathRule`/`check_path`); `mask_secrets`. Singleton lives in **hermes** `globals.rs` (`TRUST_CACHE`) and bin `main.rs` — k_ai owns the type, consumers own the storage. |
| **AgentSpec manifest / Agency** | `agency.rs` + `agency_importer.rs` + `native_agent_seed.rs` | Data-driven catalog: `AgentSpec {name, division, mission, skills, deliverable}` grouped into `Division`s by `Agency::from_specs`; `for_task`/`delegate` match skills against task text; `llm_context()` renders the fleet for prompts. `native_agent_seed::load_all()` parses 41 embedded `skills/agents/*/SKILL.md` front-matter at compile time (seed source consumed by hermes `PackageHub::seed_embedded_agents`). |
| **SGDB cognitive path** | `sgdb/` | ADR-0063/0064: namespaced KV/doc store (`hanr/ md/ pkg/ skill/ audit/ vdb/ sys/`) over `k_nano::storage` (TickvLite flash) + RAM L0/L1 arena. `AiosDatabaseEngine` = ART radix index + BQ flat Hamming index + `MemoryDoc` binary encode (L0–L7, `VectorClock`). HANR = cognitive path (facts), `remember_exchange` = episodic, BGE embeddings (`memory_systems.rs`) → `rag_context`. CRDT sync for multi-node (ADR-0081 C4). |
| **Agency (runtime)** | `hw_agents.rs`, `boot_log_agent.rs`, `self_learning.rs`, etc. | HW devices become agents with `HwCapability`; `SelfLearningAgent` pipeline DataCollector → TrainingAgent → ModelHub; agents registered by `agent_core::Agent` impls. |

## Data and Control Flow

**Health issue flow (detect → heal → hot-fix):**
1. Sources: `SelfHealAgent::tick` (PollEvery 1000) drains `KERNEL_ERROR` events; `SelfHeal::run_vid_gated_scan` / `check_device_firmware` / `check_device_skill` publish `HEALTH_ISSUE` (I3/I4); `SilentFailureDetector::detect_silent` publishes `I5:...:silent`.
2. `SelfHealAgent::tick` → `ErrorContext::from_event_bytes` → `SelfHeal::analyze` classifies (`FailureClass`) and, if recoverable, returns `RecoveryAction`.
3. `execute_recovery`: `RestartDaemon` → publishes `DAEMON_RESPAWN`; `CreateSkill` (ResourceFault) → pushes `pending_fixes` + publishes `LLM_REQUEST` (corrective prompting: error + context + lesson history) then `SKILL_CREATE` → **Hermes/LLM** generates the recovery skill → hot-fix; `AwaitLLM` records a `FailedStrategy` lesson (deduped via `already_tried`).
4. Failed verifications are recorded as lessons (`record_failure`) to avoid repeat strategies.

**Trust check flow:** `check_or_cache(token, agent, skill, now, ttl)` → denylist hit = deny; global `Enforce` (non-exempt) = deny; entry present + TTL valid + `state != Enforce` = allow; else transient-allow under Observe/Warn, deny under Contain/Enforce (must `trust_allow*` first). Violations escalate the entry's `PolicyState`.

**SGDB store/retrieve flow:** `put_kv`/`put_doc` → `ensure_ready` → L0/L1 to RAM arena (indexed in ART/BQ, `id_to_sk` logical handle) or L2+ to TickvLite (`md/Lx/key`) → `checkpoint_working` (SleepCycle CONSOLIDATE) flushes RAM → Tickv + compact; `prune_working_ram` + `update_with_replay` (PRUNE, coordination with `cortex::r3`). Retrieve: `get_kv`/`get_doc`/`get_hanr`; semantic recall: `quantize_f32` → `hamming`/`bq_top_k` → `id_to_sk` → doc; `rag_context`/`prompt_slice` feed the LLM. Checkpoint lives at `sys/checkpoint` (serialize/deserialize v3).

## Integration Points

| Consumer | What it uses |
|---|---|
| **hermes** `globals.rs` | `TrustCache` (owns `TRUST_CACHE` static), `k_ai::self_heal::SelfHeal`, `inventory::SystemArchitecture`; `pub use k_ai::{agency, inventory}` |
| **hermes** `package_hub.rs` | `agency::AgentSpec` (`agency_specs()`), `native_agent_seed::load_all()` (`seed_embedded_agents`), `sgdb::{put_pkg_meta, put_pkg_body, put_skill_blob}` |
| **hermes** `cognitive_bridge.rs` | `sgdb::{rag_context, prompt_slice, remember_exchange_full}` |
| **hermes** `memory_store.rs` | `sgdb::{get_hanr, put_hanr, remember_fact}` |
| **hermes** `sgdb_agent.rs` | full SGDB facade (`put_kv`, `art_prefix`, `prompt_slice`, `put_skill_blob`) |
| **hermes** `agents.rs` | `self_heal::ErrorContext`; `sgdb::{checkpoint_working, prune_working_ram, update_with_replay}` (SleepCycle) |
| **neural-kernel** `main.rs` | `self_heal_agent::SelfHealAgent::new()` (agent registry), `sgdb::{boot_init, demo, hamming_kernel_name, metrics_report, memory_checkpoint_e2e_smoke, store::predict_all_pci}` |
| **neural-kernel** `agency.rs` / `inventory.rs` | `pub use k_ai::agency::*;` / `pub use k_ai::inventory::*;` (emagrecer re-exports) |
| **neural-kernel** `agents.rs` | `crate::agency::Agency::from_specs(specs)` in `register_agency_agents` |
| hermes `net.rs`, `wifi_agent.rs`, `skill_market.rs`, `skill_opt.rs`; bin `tls_trust.rs` | `sgdb::store::{put_kv, get_kv}`, `sgdb::put_skill_blob` (persistence) |

**Key public exports:** `SelfHealAgent`, `SelfHeal`, `Checkpoint`, `ErrorContext`, `RecoveryAction`, `TrustCache`, `PermissionMode`, `PolicyState`, `SyscallClass`, `Agency`, `AgentSpec`, `AgentSeed`/`native_agent_seed::load_all`, `HardwareInventory`, `SystemArchitecture`, `FederatedTrainer`, `CompressionTier`/`BudgetManager`, and the full `sgdb` facade.

## Submodule Map

| Submodule | Files | Responsibility (one line) |
|---|---|---|
| `arch/` | `x86_64.rs`, `simd.rs` | SIMD kernels + static ISA dispatch for BitNet ternary matmul (Scalar→SSE4.2→AVX2→AVX-512) |
| `fs/` | `inference_fs_agent.rs`, `mhi_scheduler.rs` | On-demand `/inference/` FS agent + MHI tier promotion/demotion scheduler |
| `sgdb/` | `mod.rs`, `store.rs`, `engine.rs`, `memory_doc.rs`, `layers.rs`, `art.rs`, `bq.rs`, `hamming_dispatch.rs`, `crdt_sync.rs`, `metrics.rs`, `bench.rs`, `e2e_smoke.rs` | ADR-0063 cognitive path DB: namespaced KV/doc store, ART + BQ indices, MemoryDoc L0–L7, HANR/episodic/RAG layers, CRDT sync |
| `vision/` | `vit.rs`, `ocr.rs` | SigLIP ViT-B/16 (384px → 768-dim embedding) + text-region detection for LLM reading |

## Top-level modules (non-submodule)

| Module | Responsibility |
|---|---|
| `self_heal.rs` / `self_heal_agent.rs` / `self_heal_disk.rs` | Checkpoint save/restore, failure classification, recovery actions, VID-gated health scan; PollEvery-1000 agent; disk-migration on failure (ADR-0079 M4) |
| `trust.rs` | TrustCache, graduated enforcement, zero-trust syscall classes, secret masking |
| `agency.rs` / `agency_importer.rs` / `native_agent_seed.rs` | AgentSpec catalog, division grouping, embedded SKILL.md seed parsing |
| `inventory.rs` | `HardwareInventory` + `SystemArchitecture::infer` from k_hal device tree (k_hal::device_tree) |
| `audit.rs` / `merkle_audit.rs` | Merkle audit trail: SHA-256 chain + Ed25519 per entry, ring buffer 4096 |
| `memory_systems.rs` / `memory_agent.rs` | BGE embedding load/embed, adaptive memory budgeting (heap/cache/KV by model size) |
| `self_learning.rs` / `training_agent.rs` / `fine_tuning_pipeline.rs` / `fl_trainer.rs` / `data_collector.rs` / `feedback_agent.rs` / `success_engine.rs` | On-device ternary fine-tuning pipeline (DataCollector→TrainingAgent→ModelHub), federated gradient sharing (ADR-0081 C5), feedback loop |
| `cognitive.rs` / `expert_lifecycle.rs` / `router.rs` / `ternary.rs` / `chunker.rs` | Planning/cognition engine, expert metadata lifecycle, INT8 MoE router, bit-packed ternary tensors, CDC Rabin chunking |
| `safety_invariants.rs` / `security_detectors.rs` / `multi_user.rs` / `profile.rs` / `usage.rs` / `workflow_learner.rs` / `self_optimizing_scheduler.rs` | Fail-closed invariants I1–I4, security alert pipeline, multi-user personas, usage profiles, workflow prediction, dynamic resource scaling |
| `economy.rs` / `context_window.rs` / `conversation.rs` / `skill_snapshot.rs` / `shutdown.rs` / `hw_agents.rs` / `hw_capability.rs` / `boot_log_agent.rs` / `model_fit.rs` | Budget management (CompressionTier), LLM context windowing, curated conversation memory, skill state save/rollback, orderly shutdown, HW-as-agent capabilities, HWID→family/fw/agent mapping, boot log persistence, fit-policy re-export |
