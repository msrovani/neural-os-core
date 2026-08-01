# crates/hermes — Ring 3 Orchestration Crate Map

## Responsibility

hermes is the Ring 3 (R3) orchestration layer of neural-os-core. It is the
"consciousness" of the OS: everything above the hardware rings (k_nano R0,
k_hal R1, cortex/k_ai R2). Concretely it provides:

- **Intent routing** — parse user input into typed `Command`s (`hermes::parse_command`,
  `Command` enum), classify intents (`intent_bus::Intent`, `cognitive_bridge::route_user_intent`),
  and dispatch to skills, LLM, or native agents.
- **ReAct loop** — 7-phase Observe→Think→Plan→Build→Execute→Verify→Learn
  (`hermes::ReActPhase`), `WorkflowEngine` (Think→Plan→Execute→Verify→Refine),
  `IntentCache`, `SelfCritique`, SDD (Structured Decision Document).
- **Agent runtime** — concrete `agent_core::Agent` implementations
  (`agents.rs`: HermesAgent, NetAgent, CortexAgent, InputAgent, ConsoleAgent,
  boot-phase agents, SpecialistAgent/HwSpecialistAgent, AutoLearnAgent,
  SleepCycleAgent, FsBridgeAgent, GpuDriverAgent; plus `agents/mouse_agent.rs`,
  `agents/log_analyst_agent.rs`). `actor_registry.rs`, `native_agents.rs`,
  `hub.rs` (Observability ring buffer), `orchestrator.rs` (multi-agent workflows).
- **WASM runtime (ADR-0059)** — `wasmi_rt.rs` (wasmi sandbox, fuel, CapGate-gated
  `aios::*` host ABI), `wasm_build.rs` (op-IR → valid wasm assembler),
  `app_factory.rs` (A/B/C backend selector: wasmi / Cranelift JIT / Rust-subset
  native, gated by isolation ring), `wasi_host.rs` (WASI Preview 1 stubs),
  `gguf_wasm.rs`, `micropython_wasm.rs`, `dynskill.rs`, `elf_loader.rs`.
- **Skills ecosystem** — `skill_loader.rs` (SKILL.md manifests + embedded skills),
  `skill_manifest.rs`, `skill_gen.rs` (self-generated skills), `skill_observer.rs`
  (pattern/observation capture), `skill_sync.rs` (mesh sync via P2P), `skill_market.rs`,
  `skill_marketplace.rs`, `skill_opt.rs`, `expert_skills.rs`, `dynskill.rs`,
  `self_evolve.rs`/`evolve.rs`.
- **Package hub** — `package_hub.rs` (ADR-0051/0052 packages under
  `/mnt/neural/ecosystem/`), `app_store.rs` (AppForge), `marketplace.rs`,
  `plugin_hub.rs`, `self_update.rs` (A/B dual-slot), `git_thin.rs`.
- **Networking FE** — `net.rs` (NIC mirrors `NET_CONFIG`, `NETSTACK`), `net_bridge.rs`
  (registered fn pointers into the kernel NETSTACK), `netstack.rs` (DNS over
  SLIP/raw), `netfs.rs` (remote FS via `tcp_xfer`), `netdiag.rs`, `net_fallback.rs`,
  `network_agent.rs`, `wifi_agent.rs`/`wifi_protocol.rs`/`wpa2_hs.rs`,
  `ntp.rs`, `tls.rs`, `browser_agent.rs`, `search_agent.rs`, `rss_agent.rs`,
  `email_agent.rs`, `mcp.rs`/`mcp_client.rs`/`mcp_server.rs`, `cf_challenge.rs`.
- **FS/VFS** — `vfs/` (mount table + path resolution), `fs/` (FilesystemAgent
  trait + concrete agents), `neural_fs/` (CoW filesystem), `memory_store.rs`
  (USER/MEMORY/SOUL/PERSONA), `sgdb_agent.rs`.
- **Security/safety** — `membrane.rs` (zero ambient authority), `permission_gate.rs`
  (HITL escalation), `approval.rs` (ApprovalGate), `safety.rs`, `security.rs`,
  `quarantine.rs`, `jail.rs`, `notification_gate.rs`.
- **Personality layer** — `affect.rs`, `emotion.rs`, `soul.rs`, `executive.rs`
  (ExecutiveSupervisor, PonderNet), `proactive.rs`, `chat_tree.rs`,
  `graph_engine.rs`, `matrix_learn.rs`.

`no_std` + `alloc`. Depends on k_nano (R0), k_hal (R1), cortex + k_ai (R2),
agent-core, event-bus, skill-registry, ticket-lock, smoltcp, wasmi (optional
cranelift-codegen behind feature `jit-cranelift`).

## Design Patterns

- **Agent/Skill-first** — everything is an agent (implements `agent_core::Agent`
  with a manifest) or a skill (SKILL.md manifest with `required_tokens`).
  `agents.rs` registers boot agents; `register_agency_agents`/
  `register_hw_agents` populate the registry.
- **Intent routing with fallback ladder** — `parse_command` maps slash-commands
  to `Command`; free text becomes `Command::Chat`. Chat dispatch
  (`HermesAgent::tick`) goes: Matrix learning check → skill-creation guard →
  `cognitive_bridge::budget_tick` → `cortex.think` (intent classification) →
  `cognitive_bridge::route_user_intent` → one of Tts/DenyTrust/EscalateLlm/
  ExpertSkill/Structured/Llm. LLM path publishes to `cortex::TOPIC_LLM_REQUEST`
  and transitions to `HermesState::AwaitingLLM`; skill path runs
  `execute_skill` with LLM fallback. `IntentCache` (hash→Command, 1000-tick TTL)
  short-circuits repeated classification.
- **ReAct 7 phases + WorkflowEngine** — `ReActPhase` label/next cycle for the
  top-level loop; `WorkflowEngine::advance(success)` drives multi-step
  execution with retries; `SelfCritique::evaluate` post-verifies output.
- **app_factory A/B/C selector (ADR-0059)** — `analyze_and_recommend(&AppRequest)`
  picks backend by policy: untrusted IA → A (wasmi sandbox); trusted + perf →
  B (Cranelift JIT, still wasm semantics); trusted + wants Rust → C (native
  Rust-subset, self-hosting). `execute()` enforces HW-gate
  (`isolation_ring_available()`, false until `neural-kernel::isolation_ring`
  registers via `register_native_ring` per ADR-0060) and returns
  `FactoryOutcome::{RanWasm, RanNative, AwaitingIsolation, Denied}`. A runs
  today; B/C stay gated. `generate_and_run` is the end-to-end
  generate(op-IR)→build(wasm)→run path.
- **WASM sandboxing** — `wasmi_rt` installs host imports `aios::*`,
  `aios_net::*`, `aios_fs::*`, `wasi_snapshot_preview1` gated by CapGate
  (capability bitmask `CAP_LOG..CAP_SYS`) plus `permission_gate::PermissionGate`
  (which consults `membrane::Verdict`); fuel (`DEFAULT_FUEL`) bounds execution.
- **Skill registry integration** — hermes consumes the singleton
  `k_nano::SKILL_REGISTRY` (re-exported via `globals.rs`; no shadow copy —
  SESSION_217 lesson). `skill_sync::SkillSync` diffs and broadcasts registry
  entries over the mesh.
- **Bridge-over-function-pointer** — `net_bridge.rs` and `globals::VfsBridge`
  register kernel-implemented fns (`register_http_get_url`,
  `register_resolve_and_http_get_safe`, `register_tcp_xfer`, `register_udp_xfer`,
  `register_dns_resolve`, `install_vfs_bridge`) so hermes never depends on the
  neural-kernel bin; unregistered = honest error. Session-152 rule: Browser/
  Search/Market must use `net_bridge`, not the empty `hermes::net` mirror.

## Data and Control Flow

**User intent (WakeWord/console → Hermes → Chat → LLM → skill):**

1. Input (keyboard/voice) lands on the EventBus; `InputAgent`/`HwBridgeAgent`
   publish; `HermesAgent` (Continuous) receives `USER_INTENT` events.
2. `parse_command` → `Command`; free text → `Command::Chat(msg)`.
3. Chat dispatch: `matrix_learn::is_learning_request` intercepts learning;
   `cognitive_bridge::is_skill_creation_request` forces the SKILL_WRITER
   guard; `budget_tick` enforces iteration budget.
4. `cortex::cortex::think` classifies the message (Greeting/Chat → LLM;
   others → `skill_name()`); `route_user_intent(msg, token, tick, skill)`
   returns a `Route { kind, skill, expert, reason, approval_id, emotion }`.
5. `RouteKind::ExpertSkill|Structured` → `execute_skill(sk, payload, token)`;
   on error, fallback publishes `TOPIC_LLM_REQUEST` and waits
   (`HermesState::AwaitingLLM`). `RouteKind::Llm` publishes
   `TOPIC_LLM_REQUEST` directly; CortexAgent/LLM answers on the bus.
6. Skill creation: `skill_gen::maybe_auto_skill` promotes observed
   `TaskPattern`s (≥3 uses) into SKILL.md; `skill_observer::watch_task`/
   `watch_correction` record patterns; `PENDING_SKILL` holds generated
   (name, desc) until registered into `SKILL_REGISTRY`.

**WASM app execution (op-IR → build → run):**

1. LLM/Cortex emits constrained op-IR (`wasm_build::Op`: LocalGet/I32Const/
   I32Add/I32Sub/I32Mul) — the grammar target (ADR-0057 #412).
2. `wasm_build::validate(n_params, ops)` checks stack/locals; `build_run_module`
   assembles a valid wasm module (exported `run(i32, i32) -> i32`) by
   construction — no arbitrary bytes from the LLM.
3. `app_factory::generate_and_run(ops, a, b)` builds an `AppRequest`
   (trusted=false) → `analyze_and_recommend` → A (wasmi) → `execute` →
   `wasmi_rt::run_i32_2(wasm, "run", a, b, caps)` → `FactoryOutcome::RanWasm(i32)`.
   B/C hit `AwaitingIsolation` until `register_native_ring` (ADR-0060).
4. `package_hub` stages generated AgentWasm as a package
   (`PackageSignature::compute` hash) under `/mnt/neural/ecosystem/`,
   publishes `TOPIC_PKG_CHANGED`; later re-run loads from the hub.

**package_hub flow (ADR-0051/0052):** HITL approves via local `ApprovalGate`;
hub keeps pending by id. `PackageKind::{Skill, Agent, AgentWasm, Workflow,
Plugin, Mcp, Model, Firmware, DeviceRecipe}`; validation is deny-by-default
(schema + actions + hash + signature). Cortex catalog lists packages;
install writes staged capsules; `skill_sync`/`skill_marketplace` propagate
skills to mesh peers via `k_nano::net::udp_broadcast` + NoProto `TaskType::Sync`
packets (Master push / Worker promote).

**Mesh P2P consumption:** `k_nano` publishes non-heartbeat P2P packets on
EventBus topic `P2P_PACKET`; `skill_sync::subscribe_p2p`/`poll_p2p` (and the
same pair in `skill_marketplace`) drain them lazily and apply/activate skills.

## Integration Points

**Consumers:**
- `neural-kernel` (bin): wires agents (HermesAgent etc.), calls
  `hermes::net_bridge::register_*` with kernel NETSTACK impls, calls
  `hermes::globals::install_vfs_bridge` with bin VFS, and registers the
  native isolation ring via `app_factory::register_native_ring` when
  ADR-0060 F6 passes. Bin re-exports NIC statics from k_nano (not hermes).
- `jarbas` (R3 display FE): persona/tone via `persona` commands and
  `memory_store::*persona*`; Hermes Chat UI commands via `apps`/APP_REGISTRY.
- `cortex`/`k_ai`: LLM request/response over EventBus topics
  (`TOPIC_LLM_REQUEST`), `TrinityRouter` (`TRINITY`), TrustCache, SelfHeal,
  Agency/hw_agents/inventory via `globals` re-exports.

**Key public exports (from `lib.rs`):**
- Mesh/skills: `skill_sync::{subscribe_p2p, poll_p2p, SkillSync::sync_skills}`,
  `skill_marketplace::{subscribe_p2p, poll_p2p}`.
- WASM runtime: `app_factory::{analyze_and_recommend, execute,
  generate_and_run, register_native_ring, isolation_ring_available,
  AppBackend, FactoryOutcome, self_test}`, `wasmi_rt::{run_i32_2, HostState,
  CAP_*}` (via `wasmi_rt` module), `wasm_build::{Op, validate,
  build_run_module}`, `wasi_host`.
- Packages: `package_hub` (ECOSYSTEM_ROOT, PackageKind, TOPIC_PKG_CHANGED).
- Intent: `hermes::{parse_command, Command, ReActPhase, Sdd, IntentCache,
  WorkflowEngine, SelfCritique}`, `intent_bus::{Intent, IntentCategory}`,
  `cognitive_bridge::{route_user_intent, budget_tick}`.
- Bridges: `net_bridge::{register_http_get_url, register_tcp_xfer,
  register_udp_xfer, register_dns_resolve, ...}`,
  `globals::{install_vfs_bridge, read_vfs, write_vfs, list_vfs, APPROVAL_GATE,
  TRUST_CACHE, TRINITY, SELF_HEAL, PENDING_SKILL}`.
- Security: `membrane::Verdict`, `permission_gate::PermissionGate`,
  `approval::ApprovalGate`.
- Network FE: `net::{NET_CONFIG, NETSTACK, RTL8139, E1000, VIRTIO_DEV,
  run_network_diagnostics}`, `netfs::NetFs`.

## Submodule Map

| Submodule | Files | Responsibility |
|-----------|-------|----------------|
| `src/agents/` | 2 (+`agents.rs`) | Native agent structs: MouseAgent (PS/2 → EventBus), LogAnalystAgent (Cortex log mining); the bulk of agents live in `agents.rs` |
| `src/apps/` | 3 (+`mod.rs`) | `App` trait + `APP_REGISTRY`; HermesApp/SettingsApp/PowerApp expose chat commands (no multi-window) |
| `src/cross_os/` | 3 (+`mod.rs`) | CrossOsAgent + CrossOsDiscoverer (runtime skill search: package hub / P2P / GitHub / crates.io via MCP) + CrossOsIntent classification |
| `src/fs/` | 8 (+`mod.rs`) | `FilesystemAgent` trait + ATA/DevFS/ProcFS/Inference/Hermes/Ram/Log FS agents + `RingBufStore` + `MhiScheduler` tier promotion |
| `src/memory/` | 1 | `MemoryStore` with 8 Atkinson-Shiffrin tiers (L0–L7), read promotion, TTL eviction |
| `src/neural_fs/` | 11 (+`mod.rs`) | NeuralFS: CoW volume, superblock, B-tree, inode/dir/extent, CRC32C checksum tree, journal (WAL), `NeuralFsAgent` (VFS at `/mnt/neural`), tests |
| `src/vfs/` | 1 (+`mod.rs`) | `VFS` registry (mount table, longest-prefix resolve, tree), `path.rs` utils, vector FS mount |
| top-level `net*.rs` | 10 | Network FE: `net.rs` (config/statics), `net_bridge.rs`, `netstack.rs`, `netfs.rs`, `netdiag.rs`, `net_fallback.rs`, `network_agent.rs`, `net.rs`-adjacent `wifi_agent`/`wifi_protocol`/`wpa2_hs`, `ntp.rs`, `tls.rs` |
| top-level runtime | — | `hermes.rs` (core), `app_factory.rs`, `wasmi_rt.rs`, `wasm_build.rs`, `package_hub.rs`, `skill_*`, `memory_store.rs`, `hub.rs`, `orchestrator.rs`, `executive.rs`, `membrane.rs`, `permission_gate.rs`, `approval.rs`, `globals.rs` |

## Notable Top-level Modules

| Module | Purpose |
|--------|---------|
| `hermes.rs` | Core: `Command`/`parse_command`, `ReActPhase`, `Sdd`, `IntentCache`, `WorkflowEngine`, `SelfCritique`, council, bitter pills, context fencing |
| `agents.rs` | All major `Agent` impls: HermesAgent (chat dispatch, skill execution), NetAgent (smoltcp poll), CortexAgent (LLM), InputAgent, ConsoleAgent, boot-phase agents, AutoLearn/SleepCycle |
| `cognitive_bridge.rs` | Intent routing decisions, budget, session records, skill-creation guard |
| `app_factory.rs` / `wasmi_rt.rs` / `wasm_build.rs` / `wasi_host.rs` | ADR-0059 WASM execution pipeline |
| `package_hub.rs` / `app_store.rs` / `marketplace.rs` / `plugin_hub.rs` | Package ecosystem (ADR-0051/0052) |
| `skill_sync.rs` / `skill_marketplace.rs` | ADR-0081 mesh skill propagation (`subscribe_p2p`/`poll_p2p`) |
| `net_bridge.rs` / `globals.rs` | Function-pointer bridges to kernel NETSTACK / VFS |
| `membrane.rs` / `permission_gate.rs` / `approval.rs` / `safety.rs` / `security.rs` | Capability + HITL security stack (ADR-0076) |
| `memory_store.rs` / `memory/` | Persistent USER/MEMORY/SOUL/PERSONA + tiered memory store |
| `vfs/` + `fs/` + `neural_fs/` | Filesystem stack: virtual mount layer over agent-backed FS |
| `mcp.rs` / `mcp_client.rs` / `mcp_server.rs` | JSON-RPC 2.0 MCP server + external tool bridge (ADR-0076 F3/F6) |
| `cross_os/` | Runtime skill discovery/execution across ecosystems |
| `elf_loader.rs` | PE/ELF/Mach-O/APK loaders + syscall-to-skill translation (#306/#307) |
| `self_update.rs` / `git_thin.rs` | A/B dual-slot self-update; git-over-HTTPS thin client (ADR-0074) |
| `executive.rs` / `affect.rs` / `emotion.rs` / `soul.rs` / `proactive.rs` | Executive supervisor (PonderNet), personality, proactive heartbeats |
