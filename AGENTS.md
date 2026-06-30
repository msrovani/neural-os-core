# ════════════════════════════════════════════════════════
#   PLANO DIRETOR — neural-os-core v0.62.1 🏆
#   VFS + MHI BRIDGE + STORAGE AGENTS
#   ARC-style tiering (ZFS-inspired), 96 arquivos Rust
# ════════════════════════════════════════════════════════

# Role and Purpose
You are a Senior Systems and AI Engineer building "neural-os-core", an AI-native bare-metal OS from scratch. You operate with one foundational principle: **everything is an Agent or a Skill**. There are no tasks, no services, no standalone drivers — only agents with manifests, capabilities, and lifecycle.

# Core Architecture & Constraints
1. **Bare-Metal Rust:** `no_std` + `no_main`. No std, no POSIX, no Linux legacy.
2. **Agent/Skill-First:** Every entity is an Agent (autonomous, stateful, persistent) that exposes Skills (stateless request-response capabilities). Current 8 `async fn` daemons are being migrated to Agent instances. See `IDEA_BANK.md` Section 1.28.
3. **Hardware Rings:** Ring 0 (NPU — intent routing, context memory), Ring 1 (GPU — tensor execution), Ring 2 (CPU — agents and skills).
4. **Emulation First:** QEMU `qemu-system-x86_64` before any physical hardware.

# Agent/Skill-First Design Principles

### 1. Unificação Ontológica
Toda entidade executante é um `Agent`. Drivers (rtl8139, xhci) viram `DriverAgent`. Daemons (system_daemon, cortex_llm) viram `InferenceAgent`, `RouterAgent`, etc. Skills são interfaces — não entidades separadas.

### 2. Manifesto Explícito
Cada agente declara: nome, tipo (System/Driver/Inference/Router/Console/Network/Skill), capacidades, schedule, trust tokens. Nada é implícito.

### 3. Boot = Agent Activation Chain
```
bootloader → kernel_main
  ConsoleAgent (VGA+Serial)
  → SystemAgent (IDT+GDT+heap+SIMD)
  → PCIAgent (PCI scan) → ACPIAgent (MADT) → SMPAgent (AP boot)
  → HwDiscoverAgent (inventário) → NetDriverAgent | UsbDriverAgent
  → HermesAgent (input+intent+output)
  → CortexAgent (LLM transformer)
  → AgentScheduler::run()
```

### 4. Skills Pertencem a Agentes
Cada skill tem `agent` field — o dono. SkillRegistry vira catálogo indexado de `(agent, skill)`. `/add_skill` pergunta "qual agente vai expor esta skill?" — default é SkillManagerAgent.

### 5. Trust é por Agente
TrustAgent centraliza autorização. `(token, agent, skill)` — não só `(token, skill)`. Um agente pode executar skills de outro agente só se autorizado.

# Current Agent Landscape (v0.59.2 — 173 agents — HW Agents + The Agency)

| Código | Agente | Status | Tipo | Função |
|---|---|---|---|---|
| A-001 | **SystemAgent** | ✅ Agent | System (Oneshot) | Init, SYSTEM_READY, EchoSkill |
| A-002 | MonitorAgent | ✅ Agent | System (Oneshot) | Publica SYSTEM_READY |
| A-003 | HwBridgeAgent | ✅ Agent | Router (Continuous) | Scancode IRQ bridge |
| A-004 | NetAgent | ✅ Agent | Network (Continuous) | smoltcp poll + HTTP |
| A-005 | InputAgent | ✅ Agent | Console (Continuous) | Keyboard buffer |
| A-006 | CortexAgent | ✅ Agent | Inference (Continuous) | LLM generate_text() + Medusa |
| A-007 | HermesAgent | ✅ Agent | Router (Continuous) | Intent routing + skills |
| A-008 | **DisplayAgent** | ✅ Agent | Console (Continuous) | **Framebuffer BGRA32** |
| A-009 | NetDriverAgent | ✅ Agent | Driver (Oneshot) | RTL8139 + VirtIO-net |
| A-010 | UsbDriverAgent | ✅ Agent | Driver (Oneshot) | xHCI port scan |
| A-011 | **BootSelfHealAgent** | ✅ Agent | System (Oneshot) | SelfHeal init |
| A-012 | **BootTrustAgent** | ✅ Agent | System (Oneshot) | TrustCache init |
| A-013 | **PlatformAgent** | ✅ Agent | System (Oneshot) | PCI+ACPI+APIC+SMP |
| A-014 | **MemoryAgent** | ✅ Agent | System (Oneshot) | MHI + SystemArchitecture |
| A-015 | **GpuDriverAgent** | ✅ Agent | Driver (Oneshot) | **VirtIO-GPU detect** |
| A-016 | **HwDetectAgent** | ✅ Agent | System (Oneshot) | HwIdentifySkill |
| A-017 | **CronAgent** | ✅ Agent | System (Continuous) | Cron Scheduler (#232) |
| A-018 | **SecurityAgent** | ✅ Agent | System (Continuous) | Security Pipeline (#260) |
| A-019 | **SafetyAgent** | ✅ Agent | System (Continuous) | Safety Interceptor (#270) |
| A-020 | **OptimizerAgent** | ✅ Agent | System (Continuous) | Self-Optimization |

**Bloco 11 (Sprints 39-42):** Bloco único consolidado. Agent/Skill-First completo.
**Bloco 12 (Sprints 43-44):** Network Evolution — DHCP, ARP, VirtIO-net manual, NetPhy unificada.
**Sprint 45 (v0.43-0.45):** Display subsystem + VirtIO-GPU + bugfix estrutural (H3-H12).
**Bloco 12v2 (Sprint 48):** x2APIC, Huge Pages, PCI bridges, Cron Scheduler, MCP Server.
**Bloco 13 (Sprints 49-50):** Trust & Security — Ed25519, Security Pipeline, Mask Secrets.
**Bloco 14 (Sprint 51+):** Hermes Cognitive + Self-Optimization — SDD, ReAct, Council, Usage Analyzer, Dynamic Scaling.
**Bloco 15 (Sprint 52+):** Memory Systems — MemoryTree v2, Dedup, Privacy, Hybrid Search, Atkinson-Shiffrin.
**Bloco 16 (Sprint 53+):** Ecosystem Integration — SuperContext, SkillIndex, TokenJuice.
**Bloco 17 (Sprint 54+):** Cortex LLM v2 — Sampling, Codebook VQ, Medusa speculative decode.
**Bloco 18 (Sprint 55+):** Ecosystem Batch — Pipeline, DAG, Dashboard.
**Bloco 19 (Sprint 56+):** HW Real — Boot HW, USB xHCI, FAT12, ATA.
**Bloco 20 (Sprint 57+):** Bootloader 0.11 + Framebuffer UEFI.
**Bloco 21 (Sprint 58):** The Agency (147 agents) + HW Agents.
**Bloco 22 (Sprint 59):** Ecosystem Batch 3 — 12 repos portados (redox, Theseus, Embassy, Tock, Swarm, Swarms, SuperAGI, RagaAI).

Status: ✅ Agent = agente nativo (Agent trait), ✅ struct = struct/módulo existente, 🟡 wrapper = LegacyTaskAgent (migrar), 📝 = módulo avulso

# Operational Rules & Guardrails
- **Zero Hallucination Policy:** State explicitly if you don't know a low-level hardware interaction. Do not invent `no_std`-incompatible crates.
- **Agent-First Refactoring:** Always prefer: "should this be an Agent?" over "should this be a function/module/task?" If it has identity, state, or lifecycle — it's an Agent. If it's stateless request-response — it's a Skill.
- **Strict Testing:** `cargo check --release` (0 errors) + QEMU boot verify. Dead-code warnings are EXPECTED per Known Warnings Policy.
- **Boot sequence:** Rely on `bootloader` crate for UEFI/BIOS handoff.

# Memory & Documentation (ADR Protocol)
- Every architectural decision gets an ADR in `/docs/architecture/`.
- Maintain `/docs/memory/STATE.md` with current kernel state.
- `/docs/memory/IDEA_BANK.md` is the project cerebellum — 275 items cataloged, each with status. **Consult it before any architectural decision.**

# Premissa: Ciclo de Progresso Pós-Tarefa
Após cada rodada de tarefas com sucesso:
1. **Aprenda** — Documente dificuldades, erros, correções, lateralizações.
2. **Memorize** — Atualize `AGENTS.md`, `IDEA_BANK.md`.
3. **Documente** — `README.md`, `CHANGELOG.md`, `STATE.md`, `SESSION_NNN.md`.
4. **Versione** — `cargo check --release` (0 erros 0 warnings).
5. **Git** — Commit convencional + push + tag `v0.{sprint}.{item}+build{build}`.
6. **Merge/Review** — Se houver versão remota, leia e incorpore antes de continuar.

# Premissa Básica: Toda Ideia Tem Destino
- **Toda ideia discutida DEVE ter destino em `IDEA_BANK.md`.** Nada é descartado sem registro.
- Estados: ✅ implementada, 🟡 agendada, ⏳ pós-MVP, 💰 sponsor, ❌ descartada.
- Consulte o `IDEA_BANK.md` antes de toda decisão arquitetural.

# Code Style & Versioning
- Adhere strictly to idiomatic Rust. Use `clippy` configurations.
- Commit messages must follow Conventional Commits (e.g., `feat: implement memory allocator`, `fix: resolve page fault in qemu`).
- Comment complex unsafe blocks extensively, explaining *why* the `unsafe` keyword is necessary for that specific hardware interaction.

# Known Warnings Policy
- **Dead code / unused fields warnings are INTENTIONAL and EXPECTED.** We build bottom-up: PCI scan stores BARs (Sprint 18) before any driver exists (Sprint 23+), SMP stores PerCpu/AP_ONLINE before the scheduler (Sprint 24+), Slab allocator exists before any consumer migrates from LockedHeap.
- **All "unused" code is real hardware interaction** — CF8/CFC PCI config, MSR writes (EFER/GS.base), LAPIC ICR, page table walks via CR3. Nothing is mocked or simulated.
- **Zero-warning policy is NOT a goal.** These will resolve naturally when downstream consumers are implemented. Suppressing them with `#[allow(dead_code)]` would hide useful reminders of what needs wiring.
- **`#[allow(dead_code)]` is used only when Rust would warn on inherently unused statics** (e.g., `AP_ONLINE`, `CPU_TYPE_E_CORE`, `ap_entry_count()`) to avoid noise without suppressing legitimate warnings.

### Sprint 23 (v0.23.3–v0.23.4) — RTL8139 + Neural Network Agent (Block 6)
`rtl8139.rs` — Bare-metal driver via I/O ports (Port\<T\>), 4 descritores TX fixos, RX ring buffer circular (CAPR/CBR), TX funcional (ICMP/UDP/TCP). `init_driver_rtl8139()` substitui init do e1000. `network_agent.rs` — async task neural que classifica raw packets (ARP/ICMP/UDP/TCP), responde automaticamente (ARP reply, ICMP echo reply), mantém timeline `[NET @t=NN]`. Mini TCP stack manual: SYN→SYN-ACK→ACK→HTTP GET→FIN. Sem versionamento linear: adotado `v0.{sprint}.{item}+build{build}`.

### Sprint 24 (v0.24.0–v0.24.1) — smoltcp + e1000 removal + SMP fix (Block 7)
`netstack.rs` — smoltcp 0.13.1 integrado via Device trait (Rtl8139Phy). API HTTP não-bloqueante: `http_new()` + `http_poll()` (1 estado/tick). `time_utils::datetime()` — UNIX→data BR global. **e1000 removido** — arquivo deletado, init removido, proto.rs limpo. **SMP fix crítico:** `OffsetPageTable::map_to()` substitui raw PTE write que corrompia dados da BIOS quando PD[0] é HUGE_PAGE. 3 APs estáveis, page fault APIC eliminado.

### Sprint 25 (v0.25.0) — Neural Cortex in Hermes (Block 8)
`cortex.rs` — `Cortex::think()` classifica texto em 12 intenções. `intent_router_daemon` substitui `INTENT_MLP` (hand-crafted 16→8→3) por dispatch neural com skills. Pipeline completo: teclado → EVENT_BUS → Cortex → SkillRegistry → VGA. MemPalace 3.5.0 instalado para memória persistente.

### Sprint 26 (v0.26.0) — Transformer Engine (Block 9)
`cortex.rs` expandido com `TransformerModel`: Attention Q/K/V/O com causal mask, 4 camadas BitNet (RMSNorm → Attention → residual → RMSNorm → SiLU FFN → residual), tokenizer char-level, `generate_text()` autoregressivo. Model loader `.bitnet` (magic 0xBE11BE11). Python `gen_micro_model.py` para gerar pesos — 68 KB, ~272K params ternários.

### Sprint 27 (v0.27.0) — Cortex LLM Daemon (Block 10)
`cortex_llm_daemon` — 8ª task no executor cooperativo. Subscribe `LLM_REQUEST` → `generate_text()` → publish `LLM_RESPONSE`. Transformer carregado no boot sem travamentos. 9600+ ticks estável. 8 tasks: system, monitor, hw_bridge, network_agent, input, cortex_llm, intent_router, hermes_console.

### Sprint 28 (v0.28.0) — HW-Aware Cortex LLM + HwIdentifySkill
PCI ID database (23.858 entradas) → dataset → treino PyTorch → modelo .bitnet (loss 1.39) → kernel carrega via `load_model()`. `HwIdentifySkill`: `/hw` → PCI scan → LLM identifica cada dispositivo por vendor/device. Pipeline de treino: `tools/prepare_hw_dataset.py` + `tools/train_hw_model.py`.

### Sprint 31 (v0.31.0) — Hardware Capabilities
25 pares de capabilities (class → tipo → skills → MHI → driver). Modelo sabe o que fazer com cada hardware: "USB class 08 → Mass Storage: armazenamento. MHI: HDD. Driver: padrão."

### Sprints 32-36 (v0.32.0–v0.36.0) — Self-Healing Kernel (Bloco Único)
Panic handler → FailureClass::classify() → SelfHeal::analyze() → RecoveryAction (RestartDaemon, CreateSkill, LogAndContinue). KERNEL_ERROR no EventBus + EventLog. Failure Taxonomy com 5 classes (Memory, Execution, Resource, Logic, External). Exception handlers (Page Fault, Double Fault, GPF) com SelfHeal. RESPAWN_QUEUE para o executor recriar tasks. Corrective prompting: erro → LLM_REQUEST → LLM sugere recuperação. Feedback loop: lessons → already_tried() → estratégias alternativas. **5 mini-sprints em 1 bloco coeso.**

## Key Architectural Decisions
- **VGA address** computed at runtime (`0xB8000 + physical_memory_offset`)
- **`Mutex<Option<Writer>>`** for VGA (not `lazy_static!`) — depends on runtime BootInfo
- **`lazy_static!` for Serial** — SerialPort init is safe at compile time
- **GDT recreated (not extended)** — bootloader GDT is minimal
- **IST for Double Fault** — 20KB static buffer prevents Triple Fault
- **`OffsetPageTable` via Cr3** — reads CR3 for L4 table addr, no recursive mapping
- **Heap at `0x4444_4444_0000`** — high address, safe from kernel/bootloader range
- **Ternary ADD/SUB kernel** — zero FPU multiplications in weight matmul
- **2-bit packing** — 4 ternary weights per byte, `quantize_to_packed()` calibration pass

## Boot Sequence
```
cargo build --release → python tools/build_image.py --bios → qemu-system-x86_64
  └─ bootloader 0.11 → kernel_main
  ├─ serial::probe_port()        (tenta 0x3F8 → 0x2F8 → 0x3E8 → 0x2E8)
  ├─ fb::probe_bootloader_fb()   (BootInfo.framebuffer, 1280×720)
  ├─ interrupts::init_idt()      (GDT + TSS + IDT — 32 handlers 0-31)
  ├─ memory::init_memory(offset) (OffsetPageTable)
  ├─ BootInfoFrameAllocator::init
  ├─ allocator::init_heap()      (LockedHeap 16MB)
  ├─ simd::enable_simd()         (CR0/CR4)
  ├─ AgentRegistry::init_phase() (8 boot agents):
  │    PlatformAgent    → PCI + ACPI + APIC + SMP + x2APIC
  │    MemoryAgent      → Arch + MHI
  │    BootSelfHealAgent
  │    BootTrustAgent   → Ed25519 keys
  │    NetDriverAgent   → VirtIO-net → RTL8139
  │    UsbDriverAgent   → xHCI (USB HID)
  │    GpuDriverAgent   → VirtIO-GPU (PCI caps)
  │    HwDetectAgent    → HwIdentifySkill
  ├── AgentRegistry::register_agency_agents() → +147 agents
  ├── AgentRegistry::register_hw_agents()     → +6 HW agents
  └── AgentRegistry::run() (16+ runtime agents):
       SystemAgent       → SYSTEM_READY + EchoSkill
       HwBridgeAgent     → scancode bridge
       NetAgent          → DHCP + smoltcp poll (RTL8139)
       InputAgent        → keyboard (PS/2 + USB xHCI)
       CortexAgent       → LLM transformer + Medusa speculative
       HermesAgent       → intent routing + ReAct + Council + Handoff
       DisplayAgent      → Framebuffer NeuralConsole 1280×720
       CronAgent         → Cron Scheduler
       SecurityAgent     → Security Pipeline
       SafetyAgent       → Asimov 4 Laws interceptor
       OptimizerAgent    → Self-Optimization
       + The Agency (147 specialist agents, passive)
       + HW agents (~6, activate_for_intent)
```

## Active Dependencies (neural-kernel)
| Crate | Version |
|---|---|
| bootloader | 0.11.15 (bootloader_api) |
| spin | 0.9 |
| lazy_static | 1.4 (spin_no_std) |
| uart_16550 | 0.2 |
| x86_64 | 0.14.11 |
| linked_list_allocator | 0.9 |
| libm | 0.2 |
| pic8259 | 0.10 |
| smoltcp | 0.13 (alloc, medium-ethernet, proto-ipv4, socket-tcp, socket-udp) |
| ed25519-compact | 2.3.1 (no_std puro) |
| embedded-graphics | 0.8 |
| event-bus | workspace (path) |
| skill-registry | workspace (path) |
| ticket-lock | workspace (path) |

## Workspace Crates
| Crate | Status |
|---|---|
| `neural-kernel` | v0.59.2 — kernel bare-metal + framebuffer + 173 agents + RTL8139 + smoltcp + SelfHeal |
| `agent-core` | v0.1.0 — Agent trait, AgentRegistry, AgentScheduler, Pipeline, DAG, Dashboard, TimerWheel, TypedAgent |
| `skill-registry` | v0.1.0 — MCP Layer: Skill trait, McpManifest, Registry com validação de token |
| `event-bus` | v0.1.0 — IPC publish/subscribe + MemoryTree + KnowledgeGraph + Scheme + Ecosystem (dedup, privacy, hybrid, metacognitive, supercontext, skill_index, tokenjuice) |
| `ticket-lock` | v0.1.0 — TicketLock FIFO (AtomicUsize + UnsafeCell) |

## Next Sprint (Bloco 23 — Ecosystem Polishing)
GGUF loader (#278) research, SmileyOS shell + temas (#279), double buffering framebuffer (anti-cintilação),
USB-MSC BOT driver (~400 LOC) para FAT12 log persistente.

## Network Strategy (ADR-0016)
Rede via RTL8139 (I/O) + VirtIO-net (manual) + smoltcp DHCP. HW real: planejar e1000/r8169 (~300 LOC).

## Monorepo Structure
- `crates/neural-kernel/` — kernel bare-metal (bootloader 0.11, VGA, serial, framebuffer, IDT, memory, SIMD, tensor, NN, async executor, xHCI, FAT12, ATA, The Agency, HW Agents)
- `crates/agent-core/` — AgentProcess trait + scheduler + Pipeline + DAG + Dashboard + TimerWheel + TypedAgent
- `crates/skill-registry/` — Skill trait + MCP Layer (Skill, McpManifest, SkillRegistry com validação Zero-Trust)
- `crates/event-bus/` — EventBus IPC + CapabilityToken + MemoryTree + KG + Scheme + Ecosystem
- `crates/ticket-lock/` — TicketLock FIFO (AtomicUsize ticket/serving, spin loop justo)

## Roadmap
See `docs/roadmap.md` (Fases 3–7, atualizado com SotA 2026: TL/I2_S, Padé, MatMul-free).

## References
- ADR-0013: Executive Summary / Estado da Arte 2026 (MerlionOS, FairyFuse/Bitnet.cpp, ASA/eBPF)
- ADR-0014: Ideias de Evolução de Hardware (SMP, APIC, USB neural, AI-driven arch)
- ADR-0016: Network Strategy
- ADR-0025: Tier 3 Security Patterns
- ADR-0026: Ecosystem Batch 3 Analysis
- IDEA_BANK.md Section 1.28: Agent/Skill-First Architecture (280+ items total)

<!-- context7 -->
## Rust Crate Ecosystem — Always Use Context7 + crates.io

Rust crates (distributed via crates.io) evolve rapidly. Always use Context7 to fetch current docs for these essential categories:

### Searching crates.io
When a user mentions a Rust crate or library feature not in Context7, search **crates.io** via its search API:
- URL format: `https://crates.io/api/v1/crates?q={query}&per_page=5`
- Or browse: `https://crates.io/search?q={query}`
- Use `WebFetch` to read crate pages for version info, features, and docs links
- Cross-reference with `docs.rs` for API docs: `https://docs.rs/{crate-name}/{version}`

### Async & Network
- **Tokio** — async runtime, network I/O, timers. Main library for async Rust.
- **Reqwest** — HTTP client (GET, POST, consume APIs).
- **Actix-web** — high-performance actor-based web framework.
- **Rocket** — type-safe, ergonomic web framework.

### Serialization & Data
- **Serde** — industry standard for serialization/deserialization (JSON, YAML, BSON, etc.).
- **SQLx** — async SQL with compile-time query checking (PostgreSQL, MySQL, SQLite).
- **Diesel** — ORM/Query Builder with compile-time SQL validation.

### Parallelism & Error Handling
- **Rayon** — data parallelism across CPU cores.
- **Thiserror** — ergonomic custom error types.

### CLI & Terminal
- **Ratatui** — TUI (Text User Interface) framework for rich terminal UIs.
- **Clap** — CLI argument parser with subcommands, flags, auto-help.

## Steps

1. Always start with `resolve-library-id` using the library name and the user's question, unless the user provides an exact library ID in `/org/project` format
2. Pick the best match (ID format: `/org/project`) by: exact name match, description relevance, code snippet count, source reputation (High/Medium preferred), and benchmark score (higher is better). If results don't look right, try alternate names or queries (e.g., "next.js" not "nextjs", or rephrase the question). Use version-specific IDs when the user mentions a version
3. `query-docs` with the selected library ID and the user's full question (not single words)
4. Answer using the fetched docs

# Ecosystem Analysis Reference (Tiers 0-5 Complete, 141 repos, 111 ideias)

## Key Portable Patterns from Agent Frameworks (Tier 4)

When implementing Hermes daemon features, reference these patterns from Cline (63.9k ★):

### AgentRuntime Pattern (Cline)
- **Hook lifecycle**: 7 hook points — beforeRun, afterRun, beforeModel, afterModel, beforeTool, afterTool, onEvent
- **Tool policies**: `{ enabled: bool, autoApprove: bool }` per tool with wildcard `"*"` fallback
- **Completion terminal tools**: `lifecycle.completesRun` marks terminal skills  
- **Turn-based iteration**: `maxIterations` guard, inner loop: generate → parse → execute → check
- **Streaming tool assembly**: Accumulates JSON arguments, reports parse errors, merges metadata

### CronRunner Pattern (Cline)
- **Claim-based scheduling**: Atomic claim with lease heartbeat, prevents double-execution
- **Resource limiter**: Per-spec maxParallel concurrency
- **Timeout handling**: spec.timeoutSeconds → withTimeout → abort → mark failed
- **Report generation**: Markdown reports per run

### Event-Sourced Conversation (OpenHands)
- **Immutable event log**: `VecDeque<ConversationEvent { type, payload, timestamp }>` — pause, resume, fork, replay
- **Agent as pure function**: `f(history) -> next event`

### Other Portable Patterns
- **Ebbinghaus decay** (Tier 3): ~20 LOC formula for memory decay
- **SHA-256 dedup** (Tier 3): ~50 LOC for content-based deduplication (5-min window)
- **Auto-compact** (opencode/Crush): Summarize buffer when approaching context limit
- **Graph orchestration** (MS Agent): sequential/concurrent/handoff between daemons
- **Plugin Hub** (Agent Zero): Remote MCP index with AI-driven security scanning

## Tier 3b — Security, Sandbox & Filesystem (ADR-0025, 5 repos, complete)
**Repo URLs for future reference:**
- https://github.com/InnerWarden/innerwarden — 159★, 2057 commits, 7900+ tests — eBPF safety, 82 detectors, 69 correlation rules, knowledge graph
- https://github.com/akitaonrails/ai-jail — 595★ — Multi-OS sandbox wrapper: bwrap + Landlock + seccomp
- https://github.com/lspecian/vexfs — 24★ — Linux kernel-native vector search filesystem (FUSE + API + Dashboard)
- https://github.com/ckanthony/Chisel — 12★ — Rust file tools with kernel-enforced path confinement
- https://github.com/cori-do/cori-kernel — 17★ — Safe kernel principles for AI agents

### 12 portable patterns → 7 viable Sprints 24-27 (~1310 LOC), 3 future Sprint 28+, 6 discarded.
Full analysis: `docs/architecture/0025-tier3-sandbox-security-analysis.md`

## Sprint 23 (Immediate) Items
- #228 Tool Policy Registry (~80 LOC) — SkillRegistry `{ enabled, autoApprove }`
- #229 Usage Tracker (~50 LOC) — metrics accumulator for hardware_context_tensor()
- #230 Auto-Compact Hermes Buffer (~60 LOC) — summarize_context after 3+ cycles
- #231 Event-Sourced Conversation (~100 LOC) — VecDeque<ConversationEvent>
<!-- context7 -->
