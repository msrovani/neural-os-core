# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/)
with [Conventional Commits](https://www.conventionalcommits.org/).

## [0.28.0] — 2026-06-26 — HW-Aware Cortex LLM + HwIdentifySkill

### Added
- **HwIdentifySkill** — `/hw` → PCI scan → Cortex LLM identifica dispositivos
- **Intent::HardwareIdentify** — "identifique hardware" → chama a skill
- **PCI ID training pipeline** — 23.858 entradas, 31.436 pares, modelo loss 1.39
- **tools/prepare_hw_dataset.py** + **tools/train_hw_model.py**
- Modelo treinado carregado via `include_bytes!("../micro.bitnet")` + `load_model()`

## [0.27.0] — 2026-06-26 — Cortex LLM Daemon

### Added
- **cortex_llm_daemon** — 8ª task async: subscribe `LLM_REQUEST` → generate → publish `LLM_RESPONSE`
- **LLM_REQUEST/LLM_RESPONSE** — novos tópicos EventBus para comunicação com o LLM
- **8 tasks cooperativas** — system, monitor, hw_bridge, network_agent, input, cortex_llm, intent_router, hermes_console
- **9600+ ticks estável** — transformer carregado sem travamentos

## [0.26.0] — 2026-06-26 — Transformer Engine

### Added
- **Transformer completo** — `cortex.rs`: Attention Q/K/V/O, causal mask, softmax, 4 camadas BitNet
- **Tokenizer char-level** — ASCII 32-126, 99 tokens (BOS/EOS/PAD)
- **generate_text()** — loop autoregressivo argmax, max 32 tokens, para em EOS
- **Model loader .bitnet** — parse do formato binário (magic 0xBE11BE11)
- **Python gen_micro_model.py** — gera modelo de 68 KB (~272K params ternários)
- **Tensor::add() + element_mul()** — operações para resíduos do transformer

## [0.25.0] — 2026-06-25 — Neural Cortex in Hermes

### Added
- **Cortex neural intent router** — `cortex.rs`: `Cortex::think()` classifica texto em 12 intenções (SystemStatus, Echo, HardwareInfo, TrustAllow/Deny, Network, HttpFetch, Help, Conversation, Usage, Greeting, Chat).
- **Pipeline neural completo** — teclado → input_daemon → USER_INTENT → intent_router_daemon → Cortex → SkillRegistry → VGA.
- **Dispatch automático** — intent_router_daemon usa `SKILL_REGISTRY.has_skill()` para rotear para skills existentes.

### Removed
- **INTENT_MLP** — MLP antigo (16→8→3, hand-crafted) removido. Substituído por Cortex.

## [0.24.1] — 2026-06-25 — SMP Huge Page Fix

### Fixed
- **SMP trampoline huge page corruption** — Identidade de página do trampoline usava `pd0 & mask` para obter `pt_base`, mas não verificava HUGE_PAGE (bit 7). Se PD[0] é uma página de 2MB, `pd0 & mask` aponta para dados, não para uma L1 page table. Escrever PTE[64] (offset 0x200) corrompia dados da BIOS/IVT, impedindo boot dos APs e causando page faults com MALFORMED_TABLE no APIC. Substituído por `OffsetPageTable::map_to()` que gerencia todos os tamanhos de página.
- **Page fault no LAPIC EOI** — Causa raiz: mesma corrupção de tabela acima. Eliminado pelo fix do SMP.

## [0.24.0] — 2026-06-25 — smoltcp Network Agent + e1000 Removal

### Added
- **smoltcp 0.13.1 integrado** — `netstack.rs`: Device trait para RTL8139, `NetStack::poll()` via smoltcp Interface.
- **HTTP não-bloqueante** — `NetStack::http_new()` + `http_poll()`: API baseada em estados (Connecting → Sending → Receiving → Done/Failed), 1 poll/tick.
- **time_utils::datetime()** — Conversão UNIX→data-hora BR, disponível globalmente.

### Removed
- **e1000 driver** — Arquivo `e1000.rs` deletado. Substituído por RTL8139 + smoltcp.
- **proto.rs limpo** — Removidas funções E1000-dependentes (icmp_echo_request, dhcp_discover, http_get_request). Mantidos apenas utilitários (eth_header, ip_header, ip_checksum, parsers).

### Changed
- **network_agent.rs reescrito** — 473→113 linhas. Remove classificação raw Ethernet, construtores de pacotes manuais, estado TCP manual. Substituído por: `NetStack` lazy → HTTP connect → poll → done/failed.
- **Agent agora usa smoltcp** — Em vez de drenar RX manualmente, chama `netstack.poll()`.
- **net.rs** — Remove `http_get()`, `ping()` legados (stubs). Remove `E1000` static.

## [0.23.4] — 2026-06-25 — TCP handshake + HTTP GET

### Added
- **Mini TCP stack** — `build_tcp_segment()`: SYN, SYN-ACK, ACK, FIN com checksum TCP via pseudo-header.
- **HTTP GET google.com** — TCP SYN → SYN-ACK → ACK → HTTP GET → FIN. TX len=54 (SYN) funcional, timeout por NAT slirp.
- **Classificação TCP** — `PacketClass::TcpSynAck`, `TcpData` para processar handshake.

## [0.23.3] — 2026-06-25 — RTL8139 Driver + Neural Network Agent

### Added
- **RTL8139 bare-metal driver** — `rtl8139.rs`: I/O ports via Port\<T\>, 4 descritores TX fixos, RX ring buffer circular (CAPR/CBR), MAC via registradores.
- **Neural Network Agent** — `network_agent.rs`: async task que drena RX, classifica pacotes (ARP/UDP/ICMP/TCP), responde automaticamente. Timeline `[NET @t=NN]`.
- **init_driver_rtl8139()** — Scan PCI 0x10EC:0x8139, init, publica HW_NET_RTL8139.
- **ArpSender trait** — Refatoração de proto.rs: `send_arp_inner()` genérica implementada para E1000Driver e Rtl8139Driver.

### Changed
- Boot flow: RTL8139 primeiro, fallback e1000.
- Cargo.toml: versionamento `v0.{sprint}.{item}+build{build}`.
- bootimage run-args: `model=rtl8139`.

## [0.20.2] — 2026-06-25 — Network Sprint: e1000 Fixes + Neural Architecture

### Fixed

- **e1000 TDT write protocol** — `send()` escrevia REG_TDT = idx, mas com TDH=0 ambos iguais → ring empty. Corrigido: TDT = (idx+1) % NUM_DESC.
- **NUM_DESC aumentado 32→48** — 82540EM requer mínimo 48 descritores RX (Linux e1000 driver docs).
- **RXDCTL PTHRESH 0→8** — Prefetch threshold zero impedia RX de receber pacotes. Linux driver recomenda PTHRESH=8.
- **Ordem init RX** — RCTL.EN agora escrito antes de RDT (Intel spec).
- **Offsets estatísticas corrigidos** — TPT=0x0400C, TPR=0x04010 (não 0x10C0/0x1080).
- **SMP desabilitado até segunda ordem** — SMP multi-core com `-smp 4` instável no QEMU TCG.

### Added

- **Arquitetura Neural de Rede** — init_driver_network() mínimo + network_bootstrap() com ARP periódico/hlt + network_health_daemon() async.
- **Debug methods** — debug_mmio_read(), debug_rx_desc(), debug_tx_desc() no e1000 driver.
- **EventBus HW_NET_E1000** — publicado quando e1000 é detectado.
- **Arquivo `NETWORK_DEBUG_HOME.md`** — relatório completo para continuar debug em casa.

### Changed

- Network discovery agora é neural: hardware → evento → daemon → skill.
- `/ping`, `/fetch`, `/netdiag` roteados pelo MLP.
- IP configurado antes do ARP (SPA válido nas requisições).
- `cargo check --release`: 0 erros, ~35 warnings

## [0.20.1] — 2026-06-25 — e1000 DMA Fix + /ping Command

### Fixed

- **e1000 Page Fault** — `allocate_contiguous()` começava do bit 0, alocando frames físicos < 1 MB não mapeados pelo bootloader. Corrigido para iniciar de `next_free_bit`, evitando a região não mapeada. Root cause: bootloader `map_physical_memory` só mapeia regiões `Usable` da UEFI; frames 2-159 (usados para trampoline SMP) não estão no mapa virtual.
- **DHCP removido (temporário)** — Spin loops no QEMU TCG não dão tempo para o slirp processar I/O. IP estático 10.0.2.15 + gateway MAC hardcoded 52:54:00:12:34:56.

### Added

- **Comando `/ping <ip>`** — ICMP Echo Request via e1000. `net::ping()` usa `icmp_echo_request` + `parse_icmp_reply` existentes. Help atualizado.

### Changed

- `src/memory.rs` — `allocate_contiguous()`: `i = 0` → `i = self.next_free_bit`
- Debug prints removidos de `e1000.rs` e `net.rs`
- DHCP/DNS funções marcadas `#[allow(dead_code)]`
- `cargo check --release`: 0 erros, 35 warnings
- Boot QEMU validado: e1000 Init OK, executor 11000+ ticks estável

## [0.20.0] — 2026-06-25 — Sprint 23: Hermes Governance & Agent Memory

### Added

- **#228 Tool Policy Registry** — `SkillRegistry.set_policy()` / `get_policy()` with per-skill `{ enabled, autoApprove }` and `"*"` wildcard fallback. `execute_skill` now gates on `enabled`; `auto_approve` bypasses token validation.
- **#229 Usage Tracker** — `UsageTracker` global with `record_call()`, `snapshot()`, `to_metrics_tensor()`. Tracks per-skill call counts and exec time. Accessible via `/usage` Hermes command.
- **#230 Auto-Compact Hermes Buffer** — `ConversationTracker` auto-compacts conversation after 3 cycles. Summary logged to serial on compact.
- **#231 Event-Sourced Conversation** — `EventLog` with `VecDeque<ConversationEvent>` (max 256), push/iter/summarize. Events recorded for UserInput and HermesResponse. Query via `/conv` Hermes command.
- New Hermes commands: `/usage`, `/conv`
- Help updated to include all new commands
- `cargo check --release`: 0 errors
- Version bump: v0.19.0 → v0.20.0

## [0.19.0] — 2026-06-25 — 🏁 "Hermes Awakening" Milestone

### Milestone: Ecosystem Analysis Complete (Tiers 0-4)

- **136 repositories analyzed** across 5 tiers (Crom 75, Life OS 20, PAI 21, Memory 14, Agent Frameworks 6)
- **249 ideas cataloged** in IDEA_BANK.md, all with status and sprint assignment
- **5 Architecture Decision Records** created (ADRs 0020-0024)
- Documentation chain fully reviewed and repaired: README.md, SUMMARY.md, roadmap.md, ADR-0015, CHANGELOG.md — all consistent
- **99 portable patterns** extracted — from XOR Delta (50 LOC) to Cline AgentRuntime (850 LOC patterns)
- **Key findings confirmed:** Hermes daemon architecture mirrors industry best practices (hook lifecycle, skill registry, event bus, trust cache)
- **Phase transition:** Research → Implementation. Next: Sprint 23 (Network + Tool Policy + Usage Tracker + Event-Sourced Conversation)
- Version bump: v0.18.4 → v0.19.0

## [0.18.4] — 2026-06-25

### Added (Tier 4 Agent Frameworks Analysis — ADR-0024)

- **ADR-0024** — Comprehensive analysis of 6 Agent Frameworks repos (Tier 4)
- **Deep-dive: Cline** (63.9k ★, 293 releases, 6,338 commits) — AgentRuntime, ClineCore, CronRunner source read
- **22 new IDEA_BANK items** (#228-249), classified by complexity:
  - **Sprint 23 (immediate):** Tool Policy Registry (#228), Usage Tracker (#229), Auto-Compact Buffer (#230), Event-Sourced Conversation (#231)
  - **Sprint 24 (low):** Cron Scheduler (#232), Session Checkpoint (#233), Plan/Execute Modes (#234), Graph Orchestration (#235)
  - **Sprint 25 (medium):** Plugin Hub (#236), Completion Terminal Skills (#237), Claim-Based Lease (#238), Time Travel (#239), Context Compaction (#240)
  - **Sprint 26+ (high):** Observability (#241), AI Security Scan (#242), Hub Discovery (#243), Human-in-the-Loop (#244)
  - **Future:** 3 items (#245-247)
  - **Discarded:** 2 items (#248-249 — Docker, Python/.NET)
- **Key portable patterns:** Hook lifecycle (7 points), Tool policies (wildcard + per-tool), Claim-based scheduling with lease heartbeat, Session checkpoint/restore, Event-sourced conversation
- **IDEA_BANK.md** updated to **249 total items**
- **AGENTS.md** updated with Sprint 23 reference patterns
- **Documentation review:** README.md, SUMMARY.md, roadmap.md, ADR-0015 — all updated for 249 items
- **SESSION_025.md** created
- Version bump: v0.18.3 → v0.18.4

## [0.18.3] — 2026-06-25

### Added (Tier 3 Memory Systems Analysis — ADR-0023)

- **ADR-0023** — Comprehensive analysis of 14 Memory Systems repos (Tier 3)
- **Deep-dive: agentmemory** (24k ★, 60+ source files) — SHA-256 dedup, Privacy filter, BM25+Vector+Graph hybrid search, 4-tier consolidation
- **Deep-dive: nexo** (cognitive memory) — Atkinson-Shiffrin 3-tier, Ebbinghaus decay, trust scoring, metacognitive guard
- **14 new IDEA_BANK items** (#214-227), classified by complexity
- Key portable: SHA-256 dedup (~50 LOC), Ebbinghaus decay (~20 LOC), TTL eviction (~40 LOC) — all no_std Rust
- **IDEA_BANK.md** updated to 227 items

## [0.18.2] — 2026-06-25

### Added (Tier 2 PAI Ecosystem Analysis — ADR-0022)

- **ADR-0022** — Comprehensive analysis of 21 Personal AI Assistant repos (Tier 2)
- Deep-dives: OpenClaw (380k ★, Rust), Hermes Agent (202k ★), Lethe (Rust brain-inspired), ZeroClaw (32k ★, Rust)
- **15 new IDEA_BANK items** (#199-213)
- Key portable: Skill Metadata, Audit Ring, Awakening Mode, Context Fencing, Tool Permissions, Lifecycle Hooks

### Added (Tier 1 Life OS Analysis — ADR-0021)

- **ADR-0021** — Comprehensive analysis of 20 Life OS repos
- **13 new IDEA_BANK items** (#177-189)
- Key portable: Spectrum Graph, Runtime SDD, FS as Context, Temporal KG, AppForge, WASM Sandbox

## [0.18.1] — 2026-06-24

### Added (Crom Ecosystem Analysis — ADR-0020 + Ed25519 Identity)

- **ADR-0020** — Comprehensive Rust viability analysis of MrJc01's Crom ecosystem (75 repos)
- **9 actionable items** with `no_std` Rust code models, classified by complexity:
  - **Sprint 24 (immediate):** XOR Delta reconstruction (#164), CDC Rabin Fingerprint (#165)
  - **Sprint 27 (low):** Multi-mode Trust (#166), TV-DSL Co-processor (#167), PonderNet (#168)
  - **Sprint 28 (medium):** Codebook VQ (#169), KV Cache Codebook (#170), ReAct loop (#171), MCP Server (#172)
- **3 future items** (#173-175): Codebook LLM finetune, Delta branches, Workspace isolation
- **~1,780 LOC kernel** + **~300 LOC Python** total for all 9 features
- **Disposições:** gRPC, FUSE, Firecracker VMs, Verbo language, Crom-Pet, Active Inference — descartados como inviáveis
- **#176 — Ed25519 Cryptographic Identity** for TrustCache: upgrades static `CapabilityToken(u64)` to real Ed25519 signing (Crom-meueu port). ~300 LOC, Sprint 27, depends on #166 Multi-mode Trust
- IDEA_BANK.md updated with ADR-0020 reference in section 1.23 + item #176
- SESSION_024.md created with full session narrative
- Version bump: v0.18.0 → v0.18.1

## [0.18.0] — 2026-06-24

### Planned (Sprint 24+ — Neural Cortex BitNet LLM Integration)

- **ADR-0019** — Neural Cortex Architecture: 3-layer decision pipeline (Reflex MLP → BitNet LLM 1.5B → WASM Skills)
- **31 new IDEA_BANK items** (#126-156): Transformer Engine, Cortex Daemon, Success Engine, Training Pipeline
- **Sprint 25:** Attention, causal mask, softmax, TransformerBlock, generation loop, tokenizer, micro-model (1M)
- **Sprint 26:** Cortex Daemon, 1.5B model (~375 MB), model HTTP update, hardware/memory/trust decisions via LLM
- **Sprint 27+:** Reflex threshold tuning, sampling strategies, speculative decoding, Success Engine (online learning)
- **Memory budget:** 2 GB QEMU → 375 MB model + ~100 MB runtime + ~1.5 GB free
- Version bump: v0.17.1 → v0.18.0 (architecture planning)

## [0.17.1] — 2026-06-24

### Fixed (Sprint 23 — Code Review & Critical Bugfix Sprint)

- **#1 — e1000 RCTL/TCTL enable:** Added `REG_RCTL` / `REG_TCTL` constants and 8 enable bits. NIC was previously dead.
- **#2 — e1000 MMIO BAR mask:** Replaced `if/else (bar0 & 1)` with unconditional `(bar0 & !0xF) as u64`.
- **#3 — DHCP broadcast MAC acceptance:** `parse_dhcp_offer` and `parse_dhcp_ack` now accept `FF:FF:FF:FF:FF:FF` as destination.
- **#4 — DHCP false positive ACK:** Changed `return true` to `return false` when no ACK received.
- **#5 — Slab allocator off-by-one:** `addr + block_size <= zone_end` → `addr + block_size < zone_end` prevents buffer overflow.
- **#6 — Inline asm UB:** Removed `options(nostack)` from `pushfq; pop` instruction.
- **#7 — PCI bridge secondary bus:** Added `read_config_byte()`, reads secondary bus number at offset 0x19 instead of hardcoded `bus+1`.
- **#8 — ACPI XSDT stride:** Detects XSDT vs RSDT; uses 8-byte entry stride for XSDT (was 4 bytes, truncating 64-bit pointers).
- **#9 — MHI alloc_by_tier:** Uses `allocate_contiguous()` first; frees previously allocated frames on failure.
- **#10 — Neural bias per batch row:** Bias now applied to all batch rows (nested loop `batch_size × out_features`).
- **DHCP protocol fixes:** xid kept same for REQUEST (not `+1`); hostname option length 12→11 (`b"neural-aios"` is 11 bytes).
- **mhi.rs:** Added `FrameDeallocator` import for deallocation cleanup.
- ADR-0017: Critical Bugfix Sprint documentation.
- SESSION_023.md: Detailed session log with difficulties and decisions.
- Version bump: v0.17.0 → v0.17.1

## [0.17.0] — 2026-06-24

### Added (Sprint 22 — Block 5: Skills + Trust Cache)

- **`trust.rs`** — `TrustCache` with:
  - `is_trusted(token, skill_name, now_ticks)` — checks cache and denylist
  - `trust_allow(token, skill_name, now_ticks)` — permanent trust until explicit deny
  - `trust_deny(token, skill_name)` — revoke trust + add to denylist
  - `check_or_cache(token, skill_name, now_ticks, ttl_ticks)` — auto-cache on valid token (360 ticks ≈ 20s TTL)
- **`HardwareInfoSkill`** — new skill exposing `SystemArchitecture` (ring mode, heap size, etc.) and MHI tier info. Invoked via `/hw`, `/hardware`, or `/info` commands.
- **`SystemStatusSkill` upgraded** — now reads MHI tiers + `GLOBAL_ALLOCATOR` occupancy to report per-tier free/total RAM in MB.
- **`SkillRegistry` additions** (`registry.rs`):
  - `has_skill(name) -> bool` — check if skill exists
  - `validate_token(name, token) -> bool` — check token authorization without executing
  - `execute_skill_unchecked(name, payload)` — skip token validation (caller must validate)
- **Trust-aware Hermes commands**:
  - `/trust allow <token> <skill>` — permanently authorize a token for a skill
  - `/trust deny <token> <skill>` — revoke authorization
  - `/hw` — display hardware info and system architecture
  - All skill executions (`/status`, `/echo`, MLP-triggered) now use `execute_skill_with_trust()` helper
- **Help text updated** — lists all available commands
- Version bump: v0.16.0 → v0.17.0

## [0.16.0] — 2026-06-23

### Fixed (Sprint 21 — IOAPIC mask bug)

- **apic.rs `redirect_irq()`** — removed `(1u32 << 16)` from redirection entry low dword. Bit 16 is the MASK bit in IOAPIC redirection entries. The original code set it, masking all interrupts (timer, keyboard, etc.). Without timer interrupts, the executor's `hlt()` never woke up, stalling the system after the first poll cycle. Debug output confirmed: `IOAPIC redirection[0]: low=0x00010000` (bit 16 = masked). After fix: timer IRQ0 (vector 32) delivers at ~18.2 Hz, executor cycles normally.

### Added (Sprint 21 — Block 4: MLP + MHI + Auto-detecção)

- `mhi.rs` — Memory Hierarchy Index with:
  - `AllocTier` enum: Dram, Vram, Nvme, Hdd
  - `MemoryTier` struct: kind, capacity_bytes, bandwidth_mbs, latency_ns, name
  - `MemoryHierarchy::new()` — auto-creates Dram tier from bitmap frame allocator
  - `alloc_by_tier(Dram)` — allocates contiguous physical frames, returns PhysAddr
  - Other tiers return `None` (drivers not yet implemented)
- `inventory.rs` — Hardware Inventory & System Architecture with:
  - `HardwareInventory::collect(pci_devices, acpi_info)` — CPU count, RAM, PCI device detection (VirtIO-net/GPU, NVMe, XHCI)
  - `SystemArchitecture::infer(inv)` — rule-based heuristics: GPU detect → ring1, RAM size → heap, CPU count → power mode
  - Both pure data structures for future MLP weight training (item #51)
- `memory.rs` — `BitmapFrameAllocator::usable_memory_bytes()` public accessor
- **Adaptive boot flow** — `main.rs` now runs: PCI scan → HardwareInventory::collect() → SystemArchitecture::infer() → log to VGA+serial → MHI init → NeuralExecutor. Example output: `[ARCH] ring0=0 ring1=0 heap=2048MB` / `[MHI] 1 tier(s), X MB usable.`
- **Workspace crate versions** — `neural-kernel` bumped to v0.16.0

## [0.15.0] — 2026-06-23

### Added (Sprint 20 — Block 3: Hermes Chat)

- `hermes.rs` — Hermes Chat console module with:
  - `IntentMlp` — real MLP intent classifier: bag-of-words (16-word vocab) → Linear(16→8) → SiLU → Linear(8→3) → argmax (3 intents: chat, status, echo)
  - Hand-crafted weights for keyword-based classification (status/memory/ram/cpu/system → status intent; echo/reverse → echo intent; hello/hi/help → chat intent)
  - `parse_command()` — multi-word command parser: `/status`, `/echo <text>`, `/help`, `/stats`, `/mem`
  - `Command` enum: Status, Echo(String), Help, Chat(String)
- **scancode_to_ascii()** — expanded with digits 0-9 (0x02-0x0B) and punctuation (`- = [ ] ; ' ` \ , . /`) for full command-line input
- **intent_router_daemon** — upgraded from mock string-contains to:
  - `parse_command()` dispatches `/status` and `/echo` to SkillRegistry
  - Unrecognized text → `INTENT_MLP.classify()` → routes to SystemStatusSkill (intent 1), EchoSkill (intent 2), or default chat response (intent 0)
  - Publishes responses on `HERMES_RESPONSE` EventBus topic
- **hermes_console_daemon** — subscribes `HERMES_RESPONSE`, prints `[Hermes] <response>` to both VGA and serial
- Both new daemons spawn in the NeuralExecutor (6 tasks total)

### Changed

- `main.rs` — added `mod hermes;`, `INTENT_MLP` lazy_static, expanded scancode table, upgraded intent_router + new console daemon

## [0.14.1] — 2026-06-23

### Fixed (Sprint 19 — SMP Multi-Core Boot)

- **Root cause isolated:** bootloader identity-maps pages 0-7 only (PD[0] = 0x4023 → PT base = 0x4000). PT[64] for VA 0x40000 was `0x0000000000000000` → AP #PF on first instruction at 0x400A4 → triple fault
- **Identity-map PTE fix:** single `write_volatile` at `phys_offset + 0x4200` writes PTE `0x40000 | 0x003` (Present|Write) — AP can fetch from VA 0x40000 after enabling paging
- **CPU_COUNT race condition:** `spin::Mutex` protects `fetch_add` because QEMU TCG lacks cross-vCPU atomicity; all APs previously read same counter value
- **50ms busy-wait** after second SIPI for accurate AP count (all 3 APs finish trampoline within <20ms)
- **Slab Allocator memory corrupt fix:** `SLAB_CHUNK_SIZE` = bucket_size (not aligned to 8); free list pointer stored before chunk, retrieved via `ptr.read::<*mut u8>()`
- **asm! memcpy:** Replaced `core::intrinsics::copy_nonoverlapping` with `asm!("rep movsb")` to avoid `native_memcpy` dependency in `no_std`

### Changed

- `smp/mod.rs` — identity-map PTE written directly via raw pointer (not OffsetPageTable mapper); `AP_BOOT_LOCK: spin::Mutex<()>` around CPU_COUNT increment; 50ms busy-wait after SIPI
- `smp/trampoline.rs` — replaced `copy_nonoverlapping` with `asm!` block for zero-dependency memcpy
- `slab.rs` — `SLAB_CHUNK_SIZE` = bucket_size (not `align_up(bucket_size, 8)`); corrected `put()` free list logic

### Result

- `-smp 2`: ✅ AP 1 boots — `[SMP] AP 1 entrou em modo 64-bit Rust!` → `APs acordados: 1`
- `-smp 4`: ✅ AP 1, 2, 3 boot — `APs acordados: 3`
- `qemu_trace.log`: zero `check_exception` lines — no #UD, #PF, #GP
- Sprint 19 (Block 2) now fully operational

## [0.14.0] — 2026-06-23

### Added (Sprint 19 — Block 2: SMP + Slab + Heap 4 MB)

- `allocate_below_1mb()` — BitmapFrameAllocator aloca frame < 1 MiB para trampoline real-mode (`src/memory.rs`)
- `PHYS_MEM_OFFSET` — AtomicU64 global com offset de memória física para acesso de qualquer módulo (`src/memory.rs`)
- Slab Allocator — 8 buckets (32, 64, 128, 256, 512, 1024, 2048, 4096), free list ligada, `Mutex<SlabAllocator>` com métricas atômicas (`src/slab.rs`)
- Heap expandido de 100 KB para 4 MB — primeiros 512 KB para Slab, restante 3.5 MB para LockedHeap (`src/allocator.rs`)
- PerCpu struct (repr(C), 64 bytes) com self_ptr, cpu_id, lapic_id, bsp_flag, ring. GS.base via wrmsr(0xC0000101) (`src/smp/percpu.rs`)
- `this_cpu()` — lê gs:[0] para obter ponteiro PerCpu. `cpu_id()` lê gs:[8]
- Trampoline assembly (global_asm!) — 16-bit → 32-bit protected → PAE → EFER.LME → paging → 64-bit long mode → Rust entry. Header patcheable de 48 bytes com campos jmp32/jmp64/cr3/stack/percpu/entry_fn (`src/smp/trampoline.rs`)
- INIT-SIPI-SIPI via LAPIC ICR — `send_init_ipi()`, `send_sipi(vector)` com entrega via shorthand "all excluding self" (`src/apic.rs`)
- `wait_for_ipi_delivery()` — spin até ICR delivery status clear. `lapic_id()` — LAPIC ID register (offset 0x20)
- SMP orchestrator — `init_smp()` aloca trampoline, identity-maps, patcha, dispara INIT-SIPI-SIPI (`src/smp/mod.rs`)
- `ap_entry()` — entry point chamado pelos APs em modo 64-bit

### Changed

- `main.rs` — `mapper` scoped no boot flow para evitar aliasing com o mapper do SMP init
- Boot flow: adicionados `mod smp`, `mod slab`, `crate::smp::init_smp()` antes do NeuralExecutor

## [0.13.0] — 2026-06-23

### Added (Sprint 18 — Block 1)

- PCI scan — CF8/CFC config space access, 256 bus × 32 device enumeration, vendor/device/class/BARs (`crates/neural-kernel/src/pci.rs`)
- ACPI parser — RSDP discovery (EBDA + BIOS area), RSDT/XSDT walking, MADT LAPIC/IOAPIC/x2APIC parsing (`crates/neural-kernel/src/acpi.rs`)
- APIC init — LAPIC SVR + TPR + timer masked, IOAPIC IRQ0→vec32 + IRQ1→vec33, PIC disable (`crates/neural-kernel/src/apic.rs`)
- Dual EOI — `USING_APIC: AtomicBool` + `send_eoi()` com fallback APIC/PIC para handlers
- Boot flow: `init_pci()` → `init_acpi()` → `init_apic(info)` (fallback PIC se sem ACPI)

- Hardware Neural Routing — IRQ1 keyboard → EventBus → Agent pipeline (`crates/neural-kernel/src/main.rs`)
  - Top-Half: `keyboard_interrupt_handler` (IDT[33]) lê porta 0x60 → `LAST_SCANCODE` (AtomicU8, Release) → EOI raw
  - Bottom-Half: `hw_bridge_daemon` (async task) poll AtomicU8 → publica `RAW_HW_IRQ1` no EventBus
  - `input_daemon` (async task) subscreve RAW_HW_IRQ1 → buffer String → `scancode_to_ascii()` → ENTER publica `USER_INTENT`
  - `intent_router_daemon` (Cortex) subscreve USER_INTENT → mock inference → `SkillRegistry::execute_skill`
- Closed Intent Pipeline (Sprint 16)
  - `SystemStatusSkill` — lê `global_hardware_context()` via TicketLock, loga `"Memoria RAM: {:.2}%"`
  - 5 tasks spawnadas (3 persistentes), 1000+ PIT ticks estáveis, zero Double Faults
- `TicketLock` FIFO crate (`crates/ticket-lock/src/lib.rs`)
  - `TicketLock<T>` — `AtomicUsize ticket/serving`, `UnsafeCell<T>`, spin loop justo
  - Garantia FIFO, `Send` + `Sync` para T: Send
  - `TicketLockGuard` com `Deref`/`DerefMut` e incremento `serving` no Drop
- EventBus refatorado para TicketLock
  - `EventBus.subscribers`: `spin::Mutex` → `TicketLock<BTreeMap<...>>`
  - `Receiver.queue`: `Arc<TicketLock<VecDeque<Event>>>`
  - ID counter: `Arc<AtomicU64>` (was raw u64)
- `GLOBAL_ALLOCATOR: TicketLock<Option<BitmapFrameAllocator>>` — frame allocator encapsulado
- `init_global_allocator()` — migra frame allocator para TicketLock pós-boot
- `global_hardware_context()` — acesso thread-safe via TicketLock
- NeuralExecutor simplificado: campo `frame_allocator` removido, usa `global_hardware_context()`
- `sync` module (`crates/neural-kernel/src/sync/`) — re-exporta `ticket_lock::*`
- ADR-0013: Neural OS Executive Summary (SotA 2026)

### Changed

- EventBus modernizado: `spin::Mutex` substituído por `TicketLock` (Sprint 17)
- BitmapFrameAllocator agora protegido por `TicketLock` (não mais por `spin::Mutex`)
- NeuralExecutor não gerencia mais frame_allocator — acesso global via TicketLock
- `interrupts.rs` — expandido com handlers: GPF, Stack Segment, Segment Not Present, Invalid TSS, Alignment Check

## [0.12.0] — 2026-06-22

### Added

- Async Neural Executor (`crates/neural-kernel/src/task/`)
  - `pub struct AgentTask { id: u64, future: Pin<Box<dyn Future>> }` — with `AtomicU64` ID generation
  - `pub struct NeuralExecutor { task_queue: VecDeque<AgentTask> }` — cooperative polling loop
  - `DummyWaker` via `RawWakerVTable` — no-op waker for `no_std` environments
  - `pub fn run(&mut self)` — replaces `loop { hlt() }`; polls tasks, logs hardware context every 100 iterations
- Event Bus IPC (`crates/event-bus/`)
  - `CapabilityToken`, `Event`, `EventBus` with publish/subscribe via `BTreeMap + spin::Mutex`
  - `Receiver::try_receive()` for non-blocking polling
  - `yield_now().await` for explicit cooperation
- Skill Registry & MCP Layer (`crates/skill-registry/`)
  - `trait Skill: Send + Sync` with `manifest()` + `execute()`
  - `SkillRegistry` with Zero-Trust CapabilityToken validation
  - `EchoSkill` — reverses payload bytes
  - `SystemStatusSkill` — logs RAM occupancy via hardware context
- `async fn system_daemon()` — test agent that spawns, executes, and completes
- `async fn hardware_monitor_daemon()` — publishes SYSTEM_READY with Token(1)
- Boot sequence: `NeuralExecutor::run()` instead of raw `hlt` loop

## [0.11.0] — 2026-06-22

### Added

- `BitmapFrameAllocator` — 128 KB `.bss` bitmap covering 4 GB physical memory
- `init(&mut self, memory_map)` — varre UEFI MemoryMap, marca `Usable` como livre, o resto ocupado
- `FrameAllocator<Size4KiB>` + `FrameDeallocator<Size4KiB>` — alloc/dealloc reais com busca linear
- `allocate_contiguous(count)` — aloca N frames contíguos para Huge Pages (2 MiB / 1 GiB)
- `hardware_context_tensor() -> [f32; 2]` — `[taxa_ocupacao, 0.0]` via contador de alocações
- Stress test: 1000 alloc/dealloc estáveis, 0% leak, RAM Tensor confirmado em QEMU
- `PackedTernaryTensor` struct (`crates/neural-kernel/src/tensor.rs`) — 2-bit per weight, 4 weights per byte
- `pack_weights()` + `get_weight()` — pack/extract 2-bit ternary values
- `matmul_hybrid()` on `PackedTernaryTensor` — reads weights bit-by-bit from packed storage
- `quantize_to_packed(tensor, threshold)` — f32→ternary calibration
- ADR-0012: 2-bit Packing and Ternary Quantization

### Changed

- `nn::BitLinear` — `weights` field changed from `TernaryTensor` to `PackedTernaryTensor`
- `main.rs` — BitNet test now uses quantization + packed inference flow
- Monorepo workspace: `src/` movido para `crates/neural-kernel/src/`

## [0.10.0] — 2026-06-21

### Added

- `TernaryTensor` struct (`src/tensor.rs`) — weight storage as `Vec<i8>` with values in {-1, 0, 1}
- `TernaryTensor::from_row_major()` — constructor with shape validation
- `TernaryTensor::matmul_hybrid(input: &Tensor) -> Option<Tensor>` — ADD/SUB-only kernel
  - Weight `+1` → `accumulator += input[t]`
  - Weight `-1` → `accumulator -= input[t]`
  - Weight `0` → skip (no multiplication)
- `nn::BitLinear` struct (`src/nn.rs`) — ternary dense layer
  - `forward()` = `matmul_hybrid()` + optional bias
- BitNet hybrid inference test in boot flow
  - Input `[1.5, -0.5, 2.0]` × TernaryTensor(3×2) → `[-0.5, -2.0]`
  - Zero multiplication operators in the inner loop
- ADR-0011: BitLinear and Hybrid Ternary MatMul

## [0.8.0] — 2026-06-21

### Added

- `pic8259 = "0.10"` dependency — 8259A PIC driver with `ChainedPics`
- PIC remap (PIC1 → vector 32, PIC2 → vector 40) — `interrupts::init_pics()`
- PIT Timer watchdog handler (IRQ 0, vector 32) — atomic `TIMER_TICKS` counter + EOI
- Page Fault handler (vector 14) — reads `CR2`, logs fault address, halts via `hlt`
- `interrupts::enable_interrupts()` — `sti` instruction sets IF=1
- `memory.rs:FrameDeallocator` trait — `deallocate_frame()` for future frame recycling
- `EmptyFrameDeallocator` — no-op stub until bitmap allocator
- ADR-0009: PIC Watchdog and Page Fault Safety

### Changed

- `src/interrupts.rs` — IDT extended with `page_fault` and `idt[32]` (timer)
- `src/main.rs` — `init_pics()` + `enable_interrupts()` + watchdog `hlt` loop
- `src/memory.rs` — `FrameDeallocator` trait + `EmptyFrameDeallocator` added

## [0.7.0] — 2026-06-21

### Added

- `Tensor::transposed()` — row-major to column-major transposition (W^T support)
- `nn::Linear` struct with `weights: Tensor` and `bias: Option<Tensor>`
  - `forward(&self, input) -> Tensor` implements Y = X·W^T + B
- `nn::argmax(tensor) -> usize` — returns index of highest logit
- Intent Router MLP in boot flow
  - Input embedding + Linear(3→2) + SiLU + argmax = kernel decision
  - Tested: `[1.0, -0.5, 0.3]` → action 0 (Acionar Daemon Ring 2)
- ADR-0007: Intent Router MLP — Primeiro Córtex Primitivo

## [0.6.0] — 2026-06-21

### Added

- `libm = "0.2"` dependency for `no_std` math functions (`expf`, `sqrtf`)
- Neural primitives module (`src/nn.rs`)
  - `silu(x)` activation via `libm::expf` — tested: `[-1, 0, 1] → [-0.269, 0, 0.731]`
  - `rms_norm(tensor, weight, eps)` via `libm::sqrtf` — tested: RMSNorm of SiLU output
- `Tensor::add_scalar`, `Tensor::mul_scalar`, `Tensor::apply<F>` (generic closure)
- `nn::silu` used as closure arg to `Tensor::apply` in boot test
- ADR-0006: Neural Primitives and libm

## [0.5.0] — 2026-06-21

### Added

- SIMD enablement module (`src/simd.rs`)
  - `enable_simd()` — CR0: clear `EMULATE_COPROCESSOR`, set `MONITOR_COPROCESSOR` + `NUMERIC_ERROR`
  - CR4: set `OSFXSR` + `OSXMMEXCPT_ENABLE`
  - `f32`/`f64` operations now execute natively without `#NM` exceptions
- Tensor Engine module (`src/tensor.rs`)
  - `Tensor` struct with `shape: (usize, usize)` and `data: Vec<f32>`
  - `from_row_major()`, `matmul()` — dot product multiplication
  - Tested: 1×3 × 3×1 → 1×1 = `[32.0]`
- `simd::enable_simd()` call in boot flow after heap init
- ADR-0005: SIMD and FPU Enablement

### Changed

- `main.rs`: added `mod simd; mod tensor;` + tensor matmul test

## [0.4.0] — 2026-06-21

### Added

- Memory module (`src/memory.rs`)
  - `OffsetPageTable` — cria mapper via `Cr3::read()` + `physical_memory_offset`
  - `BootInfoFrameAllocator` — implementa `FrameAllocator<Size4KiB>` iterando mapa UEFI/BIOS
  - `init_memory(offset)` — retorna `OffsetPageTable<'static>` pronto
- Heap allocator module (`src/allocator.rs`)
  - `LockedHeap` como `#[global_allocator]` via `linked_list_allocator` v0.9.1
  - `init_heap(mapper, frame_allocator)` — mapeia 25 páginas (100 KB) em `0x4444_4444_0000`
- `extern crate alloc` ativado — `Box::new(41)` e `Vec::push([10, 20, 30])` testados em QEMU
- `linked_list_allocator = "0.9"` dependency
- ADR-0004: Memory Paging and Heap Allocation
- SESSION_004.md: Sprint 4 detailed log

## [0.3.0] — 2026-06-21

### Added

- IDT (Interrupt Descriptor Table) module (`src/interrupts.rs`)
  - Breakpoint handler (`#BP`, vector 3) — logs VGA + serial, returns
  - Double Fault handler (`#DF`, vector 8) — logs VGA + serial, panics
  - TSS with IST entry 0 (20KB dedicated stack) for Double Fault stack switching
  - GDT with kernel code segment + TSS descriptor
  - `init_idt()` — loads GDT, sets CS, loads TSS, loads IDT
- `x86_64` crate v0.14.11 dependency (IDT, GDT, TSS, CPU instructions)
- `#![feature(abi_x86_interrupt)]` for `extern "x86-interrupt"` calling convention
- Forced `int3()` breakpoint test in boot flow
- ADR-0003: Interrupt Descriptor Table
- SESSION_003.md: Sprint 3 detailed log
- QEMU path added to `PATH` documentation for Windows

### Fixed

- Handler signature adapted to `x86_64` v0.14.13 API (`InterruptStackFrame` by value)
- `static_mut_refs` warning — replaced `&STACK` with `core::ptr::addr_of!(STACK)`
- Deprecated `set_cs` — replaced with `CS::set_reg()` via `Segment` trait
- Macro scoping — explicit `use crate::{println, serial_println}` in interrupts module

## [0.2.0] — 2026-06-21

### Added

- VGA text mode output via `map_physical_memory` feature (`vga_buffer.rs`)
  - `Writer` with scrolling, 16-color support, `core::fmt::Write` impl
  - Macros `print!` / `println!` for kernel-wide use
  - Buffer mapped at runtime using `physical_memory_offset` from `BootInfo`
- Serial port logging via `uart_16550` crate (`serial.rs`)
  - 16550 UART initialization at port `0x3F8`
  - `lazy_static!` + `spin::Mutex` for safe global access
  - Macros `serial_print!` / `serial_println!`
- Dual-output panic handler in `main.rs`
  - `panic!()` writes to both VGA and serial simultaneously
- New crate dependencies: `spin` v0.9, `lazy_static` v1.5, `uart_16550` v0.2
- `bootloader` as regular dependency (kernel-side `BootInfo` type with `map_physical_memory`)
- ADR-0002: VGA and Serial Logging Infrastructure

### Changed

- Entry point migrated from raw `extern "C" fn _start()` to `bootloader::entry_point!(kernel_main)`
- VGA base address computed as `0xB8000 + physical_memory_offset` (runtime, not hardcoded)
- `STATE.md` updated with Sprint 2 progress

## [0.1.0] — 2026-06-21

### Added

- Initial bare-metal Rust kernel scaffold
  - `#![no_std]` + `#![no_main]` environment
  - Minimal panic handler (infinite loop)
  - Serial init and output via raw port I/O
- Bootloader integration (`bootloader` v0.9.34 build-dep)
  - `bootimage runner` for automated QEMU launch
  - `relocation-model=static` to produce `ET_EXEC` ELF (fixes bootloader compatibility)
- Toolchain configuration
  - `rust-toolchain.toml` pinned to nightly
  - `.cargo/config.toml` with target and runner
- Documentation protocol
  - ADR-0001: Initial Architecture and Toolchain
  - State tracker (`STATE.md`)
  - Session log (`SESSION_001.md`)
- MSYS2 + MinGW-w64 setup for Windows toolchain without MSVC
- `AGENTS.md` — system rules for AI-assisted IDEs
