# 📋 TODO MASTER — neural-os-core v0.84.x → v0.97.x
# ~360 IDEIAS DO IDEA_BANK + TODOS OS SPRINTS — CHECKLIST COMPLETO

**Data:** 2026-07-06  
**Propósito:** Checklist mestre de TODOS os itens do projeto. Cada item do IDEA_BANK está assignado a um sprint.  
**Legenda:** ✅ feito | 🟡 em andamento | ⏳ previsto | 🔴 bloqueado | 💰 sponsor | ❌ descartado

---

## SPRINT 84 — Bloco 21c: GPU Foundations (~1700 LOC) ✅
**Objetivo:** GPUs NVIDIA/AMD/Intel como devices de compute. BAR mapping, secure boot, doorbell, job ring, VRAM allocator.

### Itens
- [x] `#326` GPU BAR0/BAR1 mapping UC (~300 LOC)
  - `gpu/backend.rs:map_bars_uc()` — mapeia BAR0/1/2 como UC (PWT|PCD) para todos vendors
  - `gpu/detect.rs` — detecta BAR0/BAR1/2 por vendor (NVIDIA/AMD/Intel)
  - `gpu/backend.rs:validate_bar0()` — lê VERSION register via MMIO para cada vendor

- [x] `#352` Secure Boot GPU — ACR/PSP/GuC pipeline (~600 LOC)
  - `gpu/firmware.rs:secure_boot_gpu()` — pipeline genérico com vendor dispatch
  - NVIDIA: ACR stub com FECS/WPR/LS ucode placeholders
  - AMD: PSP firmware loading via PM4 (MIT license)
  - Intel: GuC/HuC firmware loading

- [x] `#327` GPU doorbell + SPSC job ring (~400 LOC)
  - `gpu/ring.rs:GpuJobRing` — SPSC lockless, push/ring_doorbell/poll_head
  - Doorbell por vendor: Intel (0x120038), NVIDIA (0x002000), AMD (0x1B0)
  - RING_SIZE_DWORDS=4096, alignas(64) via DMA pages UC

- [x] `#328` VRAM buddy allocator (~400 LOC)
  - `gpu/vram.rs:VramBuddy` — free list com coalescing, alocação contígua
  - `gpu/vram.rs:init_vram_tier()` — integração MHI AllocTier::Vram

- [x] `#353` GPU Compute Pipeline — submissão genérica (~300 LOC)
  - `gpu/backend.rs:init_backend()` — pipeline: BAR mapping → validate → ring init → secure boot → vendor init

- [x] `#67` AllocTier::Vram — integração MHI (~50 LOC)
  - `mhi.rs:alloc_by_tier(AllocTier::Vram)` — mapeia BAR2 da GPU
  - Goal: `alloc_by_tier(AllocTier::Vram, size)` → mapeia BAR da GPU
  - Depende de: #328 VRAM allocator

---

## SPRINT 85 — Bloco 21d: GPU Decode (BitNet offload) (~1500 LOC) ✅
**Objetivo:** Decode do BitNet roda na GPU. Prefill fica na CPU.

### Itens
- [ ] `#329` Agent.xpu prefill/decode split (~400 LOC)
  - Goal: CPU processa prompt (prefill), GPU gera tokens (decode)
  - Sub-itens: [ ] tokenization + embedding na CPU
             [ ] matmul na GPU via job ring
             [ ] sync point entre CPU e GPU
  - Dificuldades: coordenar 2 devices com latências diferentes
  - Depende de: GPU job ring (#327)

- [x] `#330` GPU matmul kernel ternário (~300 LOC)
  - `gpu/intel.rs:gpu_matmul()` — GEN compute shader via MEDIA_OBJECT
  - `gpu/intel.rs:gpu_blit()` — blitter engine para copy
  - `gpu/backend.rs:gpu_matmul()` — dispatca Intel ring, fallback CPU
  - Stub: GEN shader assembly pendente (NDA Intel, requer engenharia reversa i915)

- [x] `#331` CPU→GPU KV cache DMA (~200 LOC)
  - `gpu/kv_dma.rs:KvDma` — transferencia entre RAM e VRAM via DMA engine
  - Suporta Intel Blitter (BCS) e DMA copy genérico
  - Referência: dmaplane (arXiv 2603.10030)

- [x] `#332` XQueue preemptível (~600 LOC)
  - `gpu/xqueue.rs:XQueue` — 3 níveis: pending → in-flight → running
  - Política agnóstica de hardware (funciona NVIDIA/AMD/Intel)
  - Referência: XSched (OSDI 2025)

---

## SPRINT 86 — Bloco 30: JARVIS Persona (~950 LOC)
**Objetivo:** JARVIS ganha personalidade. SOUL.md, IPW, Session Compression, Notification Gate.

### Itens
- [ ] `#315.1` SOUL.md Personality Engine (~300 LOC)
  - Goal: Parser markdown de SOUL.md → nome/tom/humor_level/formality/empathy/greetings
  - Dificuldades: parser markdown mínimo em no_std

- [ ] `#315.2` IPW Monitor (RAPL MSR 0x610) (~150 LOC)
  - Goal: Mede energia via PKG_ENERGY_STATUS, calcula tokens/watt
  - Depende de: PerCpu (✅ Bloco 21a)

- [ ] `#315.3` Session Compression (~200 LOC)
  - Goal: 4 estratégias: Summarize (BitNet) / DropLowest / MergeSimilar / SegmentMeans

- [ ] `#315.4` Notification Gate (~200 LOC)
  - Goal: 4 urgency levels: Critical/High/Medium/Low. Rate limiting, dedup

- [ ] `#315.5` Sessionless Thread (~100 LOC)
  - Goal: Conversa contínua sem reset de contexto entre comandos

---

## SPRINT 87 — Bloco 31: JARVIS Security + AHCI (~1200 LOC)
**Objetivo:** Segurança avançada + driver SATA.

### Itens
- [ ] `#315.18` Fail-Closed Safety Invariant (~200 LOC)
  - Goal: 4 invariants SMT-proof: process separation, pre-action, fail-closed, signed evidence

- [ ] `#315.19` Merkle Audit Trail (~200 LOC)
  - Goal: Chain de audit Ed25519: tick, agent, action, payload_hash, prev_hash, signature

- [ ] `#315.20` Fluid Persona (~100 LOC)
  - Goal: Persona adapta por contexto: urgente→preciso, triste→empático, irritado→formal

- [ ] AHCI driver (SATA 6G NCQ) (~700 LOC)
  - Goal: Driver SATA nativo para HW real
  - Dificuldades: AHCI register interface, NCQ command queuing

---

## SPRINT 88 — Bloco 32: JARVIS Emotion + Cache + Pipeline (~1200 LOC)
**Objetivo:** Análise emocional, descoberta de skills, cache semântico, pipeline de persona.

### Itens
- [ ] `#315.6` Emotion Analysis (~250 LOC)
  - Goal: BitNet classifier 7 emoções + intensidade + sarcasmo + ajuste de tom

- [ ] `#315.7` Capability Contract + Consent Gates (~200 LOC)
  - Goal: 3 níveis (Safe/Moderate/Dangerous). SkillRegistry + SafetyAgent validam

- [ ] `#315.8` Skill Discovery (DSPy/ACE) (~300 LOC)
  - Goal: SkillObserver monitora padrões, sugere novas skills
  - Pipeline: observe → analyze → propose → generate

- [ ] `#315.9` ADE Pipeline (~200 LOC)
  - Goal: 4 fases: Specification (SDD) → Execution (AgentScheduler) → Review (contracts) → Recover (self-heal)

- [ ] `#315.10` Semantic Cache (5-tier) (~150 LOC)
  - Goal: Tier 1: SHA-256 exact | Tier 2: embedding >0.95 | Tier 3: pattern | Tier 4: fallback | Tier 5: cold
  - Referência: NabaOS (97.5% reduction)

- [ ] `#315.11` Persona Pipeline (16 stages) (~100 LOC)
  - Goal: SafetyCheck→StopHandler→Converse→SkillHigh→Persona→...→AuditLog

---

## SPRINT 89 — Bloco 33: SleepCycle + Advanced Memory (~2500 LOC)
**Objetivo:** Ciclo de sono (aprendizado inspirado no sono humano) + memória avançada (Atkinson-Shiffrin, KG, Ebbinghaus).

### Itens
- [ ] `#314` SleepCycle Agent (~780 LOC)
  - Goal: 5 fases: REPLAY → DREAM → CONSOLIDATE → PRUNE → REFLECT
  - Sub-itens: [ ] `#314a` Experience Replay (1000 eventos, amostra 64)
             [ ] `#314b` Generative Replay (BitNet variações sintéticas)
             [ ] `#314c` Elastic Weight Consolidation (protege skills existentes)
             [ ] `#314d` Synaptic Homeostasis (prune pesos < threshold, ~18% redução)
             [ ] `#314e` Metacognitive Reflection (confidence tracking, micro-lessons)
             [ ] `#314f` CronAgent scheduler (períodos idle)
  - Dificuldades: Primeiro sistema bare-metal com ciclo de sono. Pioneirismo.

- [ ] `#214` SHA-256 Memory Dedup (~100 LOC)
  - Goal: Prevenir entradas duplicadas no EventBus e TrustCache

- [ ] `#215` Privacy Filter (~80 LOC)
  - Goal: Strip API keys, secrets, \<private\> antes de armazenar

- [ ] `#216` Memory TTL/Eviction (~150 LOC)
  - Goal: Auto-evict baseado em TTL configurável, ImportanceRank, AccessFrequency

- [ ] `#219` Ebbinghaus Decay (~120 LOC)
  - Goal: strength = importance × e^(-λ_eff × days) × (1 + recall_count × 0.2)

- [ ] `#217` Hybrid Search (BM25 + MLP) (~200 LOC)
  - Goal: RRF fusion para intent routing: MLP + BM25 keyword fallback

- [ ] `#218` 4-Tier Memory Consolidation (~400 LOC)
  - Goal: Working → Episodic → Semantic → Procedural pipeline

- [ ] `#222` Metacognitive Guard (~300 LOC)
  - Goal: Antes de executar skill, verifica TrustCache por erros passados

- [ ] `#223` Draft→Review→Merge Memory (~350 LOC)
  - Goal: Mudanças de memória passam por workflow de aprovação

- [ ] `#224` Atkinson-Shiffrin 3-tier (~800 LOC)
  - Goal: Sensory Register (48h) → STM (7d) → LTM (permanent, semantic-indexed)

- [ ] `#225` Bi-temporal Knowledge Graph (~600 LOC)
  - Goal: Grafo temporal com validity windows + contradiction detection

---

## SPRINT 90 — Bloco 34: JARVIS Deep Cognitive (~1200 LOC)
**Objetivo:** Sonhos, ego, batimentos cardíacos proativos, auto-skills, monitor de entropia.

### Itens
- [ ] `#315.12` Dreaming/Consolidation (~200 LOC)
  - Goal: CronAgent noturno: agrupa memórias similares, gera insights sintéticos

- [ ] `#315.13` Ego Layer (~250 LOC)
  - Goal: Self-model: JARVIS sabe o que sabe/não sabe. Confidence tracking por domínio

- [ ] `#315.14` Proactive Heartbeats (~100 LOC)
  - Goal: JARVIS inicia conversa proativamente baseado em eventos

- [ ] `#315.15` Tool-State Save Game (~100 LOC)
  - Goal: Snapshot + rollback automático se skill falhar

- [ ] `#315.16` Auto-Skill Generation (~150 LOC)
  - Goal: Cratos-inspired: watch → pattern → propose → generate → register

- [ ] `#315.17` Babel-Index (~100 LOC)
  - Goal: Monitora entropia, contradiction rate, staleness index da memória

---

## SPRINT 91 — Bloco 35: Polimento + Ecosystem (~2500 LOC)
**Objetivo:** burn-flex backend, MSched VRAM, CFS, GPU+Display, SmileyOS UI patterns.

### Itens
- [ ] `#333` burn-flex backend port (~800 LOC)
  - Goal: Portar CPU backend do burn-flex (tracel-ai/burn). SIMD gemm + quantization
  - Impacto: Elimina bitnet_avx2 manual (~800 LOC). 2-95× speedup
  - Referência: github.com/antimora/burn-flex

- [ ] `#334` MSched evicção VRAM (~500 LOC)
  - Goal: Belady (OPT) eviction policy. Prevê working set do próximo kernel GPU

- [ ] `#335` CFS scheduler (~500 LOC)
  - Goal: Substituir round-robin por Completely Fair Scheduler (vruntime)

- [ ] `#336` GPU + Display co-existência (~300 LOC)
  - Goal: iGPU (Intel) display, dGPU (NVIDIA) compute. Time-sharing se só 1 GPU

- [ ] `#279a` Shell 40+ comandos (~300 LOC)
  - Goal: ls, cat, ps, uptime, theme, kill, echo, clear, help

- [ ] `#279b` Sistema de temas (~200 LOC)
  - Goal: 5+ cores, hot-swap via `theme <name>`

- [ ] `#279c` VFS upgrade com permissões (~400 LOC)
  - Goal: Filesystem próprio com permissões (rwx por agente)

- [ ] `#280l` SkillManifest derive macro (~100 LOC)
  - Goal: Proc-macro para gerar manifests de skills

- [ ] `#283a` Workspace Cube 3D (~200 LOC)
  - Goal: 3 workspaces (main/dev/chat) como faces de cubo giratório

- [ ] `#283b` Crossfade workspaces (~100 LOC)
  - Goal: Transição sem FPU, inteiros step 0..50

---

## SPRINT 92+ — Bloco 36+: AIOS Evolution (~15000 LOC) 🔴
**🔴 BLOQUEADO POR B-01** — Primeira ação em todo item bloqueado: **buscar na internet**

### B-01: DHCP/DNS/HTTP — Rede funcional (~500 LOC) 🔴
- **Goal:** smoltcp DHCP obtém IP, DNS resolve, HTTP faz GET/POST
- **Ação imediata:** Buscar na internet: smoltcp DHCP debug, RTL8139 RX datasheet, testes HW real
- **Bloqueia:** Toda a cadeia WWW (B-11, B-12, B-13, B-17, B-27)
- **Sub-itens:**
  - [ ] Debug smoltcp DHCP: descobrir por que dhcp_poll() nunca retorna Configured
  - [ ] Verificar RTL8139 RX (CAPR, RBSTART, interrupção IRQ11)
  - [ ] Testar com -nic tap,model=rtl8139 (alternativa ao SLiRP)
  - [ ] Implementar fallback: static IP 10.0.2.15/24
  - [ ] Testar ping 10.0.2.2 via ICMP socket

### WWW Agents + Network Stack (~3600 LOC) 🔴
- [ ] `#117` NIC driver genérico (detecta PCI vendor/device, busca driver online) (~400 LOC)
- [ ] `#118-120` smoltcp + DNS + HTTP stack (~500 LOC)
- [ ] `#250` /ping command (~50 LOC)
- [ ] `#251-252` DHCP/ARP com timeout + fallback (~200 LOC)
- [ ] `#307` WWW Agents: Browser, Email, Search, RSS, Download, WS (~2600 LOC)

### Self-Update + WASM (~3700 LOC) 🔴
- [ ] `#308a-c` Self-Update Agent (A/B slots, channels, rollback) (~800 LOC)
- [ ] `#309a-c` WASM Skill Runtime + IDE Agent + Hybrid Agents (~2900 LOC)

### Voice Pipeline (~1600 LOC) 🔴
- [ ] `#315.21` Piper TTS Integration (~100 LOC)
- [ ] `#315.22` Vosk/Whisper STT (~400 LOC)
- [ ] `#315.23` Wake Word (Rustpotter) (~100 LOC)
- [ ] `#315.24` Wyoming Protocol IPC (~300 LOC)
- [ ] `#315.25` Voice Pipeline completo (~200 LOC)

### Multi-device + SKYNET (~600 LOC) 🔴
- [ ] `#315.26` Multi-device sync (CRDT, Automerge-style) (~300 LOC)
- [ ] `#315.27` SKYNET Mesh Node (~300 LOC)

### The Agency + FS Agents (~1400 LOC) 🔴
- [ ] `#277a-c` HwRegistry, Agency struct, LLM-aware activation (~800 LOC)
- [ ] `#282e-h` InferenceFsAgent, HermesFsAgent, RamFsAgent, MhiScheduler (~600 LOC)

### Cross-OS Compatibility (~2000 LOC) 🔴
- [ ] `#306a` Windows PE32+ loader + syscall translation (~600 LOC)
- [ ] `#306b` Linux ELF loader + syscall translation (~500 LOC)
- [ ] `#306c` macOS Mach-O loader (~400 LOC)
- [ ] `#306d` Android APK compat (~500 LOC)

### WiFi (~1000 LOC) 🔴
- [ ] `B-29` Intel Wireless / Atheros / Realtek 802.11 (~1000 LOC)

### Compositor + Browser (~1100 LOC) 🟡
- [ ] `#279d` Compositor multi-window (dock, menus, drag) (~600 LOC)
- [ ] `#279e` v86 browser demo (WebAssembly x86 emulator) (~500 LOC)

---

## ITENS PÓS-MVP (sem sprint definido, dependem de maturação do sistema)

### ⏳ Pós-MVP — Dependem de infraestrutura básica
- [ ] `#1-15` USB stack completo (xHCI controller, device identity, WASM dispatch) (~3000 LOC)
- [ ] `#68-69` AllocTier::Nvme / Hdd (SFS-based) (~300 LOC)
- [ ] `#79-80` UEFI framebuffer + font rendering (~400 LOC)
- [ ] `#92-93` Huge Pages 2MiB / 1GiB (~300 LOC)
- [ ] `#103-104` WASM embedder + linear memory pool (~800 LOC)
- [ ] `#105-108` Success Engine (feedback loop, replay, consolidation, MatMul-free) (~2000 LOC)
- [ ] `#149-152` Feedback loop, ternary weight update, experience replay, consolidation (~500 LOC)
- [ ] `#158-159` Workflow Predictor, Auto-Skill Generator (~400 LOC)
- [ ] `#162` Workflow Profile exportável (~200 LOC)
- [ ] `#169-175` Codebook VQ, KV cache codebook, ReAct loop, MCP Server, Delta branches (~1500 LOC)
- [ ] `#186-189` AppForge, Multi-User, Workflow Builder, Federated Cluster (~3000 LOC)
- [ ] `#210-213` Actor Registry, Crash-Recovery, ComputeBackend, Plugin System (~2500 LOC)
- [ ] `#226-227` Team Memory, Memory Git Snapshots (~900 LOC)
- [ ] `#241-247` Observability, AI Security Scan, Hub Discovery, HITL, Remote Exec, Marketplace (~3000 LOC)
- [ ] `#265-267` FS Vector Search, Vector API, OverlayFS (~800 LOC)
- [ ] `#278a-b` GGUF loader + .bitnet v3 (~500 LOC)
- [ ] `#306a-d` Cross-OS compat (PE/ELF/Mach-O/APK) (~2000 LOC)

### 💰 Sponsor — Dependem de hardware específico
- [ ] `#43-52` NPU AMD XDNA driver completo (~3000 LOC)
- [ ] `#116` Port ARM/RISC-V (~5000 LOC)

### ❌ Descartados
- [ ] `#83` Intel HDA audio driver — sem skill de áudio no roadmap
- [ ] `#84` Áudio via USB (UAC) — USB + áudio = duplo pós-MVP
- [ ] `#248` Docker Sandbox — incompatível com bare-metal no_std
- [ ] `#249` Python/.NET Runtime — barreira de linguagem

---

## RESUMO GERAL

| Categoria | Itens | LOC | Status |
|---|---|---|---|
| ✅ Completos (Sprints 1-83) | ~200 | ~20.000 | ✅ |
| ✅ Sprint 84 (GPU Foundations) | 6 | ~1.700 | ✅ |
| 🟡 Sprint 85 (GPU Decode) | 4 | ~1.500 | 🟡 |
| 🟡 Sprint 86 (JARVIS Persona) | 5 | ~950 | 🟡 |
| 🟡 Sprint 87 (JARVIS Security+AHCI) | 4 | ~1.200 | 🟡 |
| 🟡 Sprint 88 (JARVIS Emotion+Cache) | 6 | ~1.200 | 🟡 |
| 🟡 Sprint 89 (SleepCycle+Memory) | 11 | ~2.500 | 🟡 |
| 🟡 Sprint 90 (JARVIS Deep Cognitive) | 6 | ~1.200 | 🟡 |
| 🟡 Sprint 91 (Polimento+Ecosystem) | 10 | ~2.500 | 🟡 |
| 🔴 Sprint 92+ (AIOS Evolution) | 25+ | ~15.000 | 🔴 |
| ⏳ Pós-MVP | 25+ | ~20.000 | ⏳ |
| 💰 Sponsor | 2 | ~8.000 | 💰 |
| ❌ Descartados | 4 | — | ❌ |
| **Total** | **~354** | **~75.000** | |

---

## SPRINT 95 — Bloco 40: Cognitive Engine (~510 LOC) ✅
**Objetivo:** Motor cognitivo completo — planejamento, aprendizado, cache, VQ, ReAct, MCP.
**Status:** ✅ Completo (v0.95.0-cog) — `cognitive.rs` 86→510 LOC.

### Itens
- [x] `#105` IntentPlanner — SkillSteps com params, goal-based plan generation
- [x] `#106` SuccessEngine — win/loss streak, recent_rate 64-window
- [x] `#107` NeuralCache — TTL + LRU evicção max 4096
- [x] `#108` MatMulFreeLM — RWKV-style WKV forward
- [x] `#149` FeedbackLoop — rating 0-10 + comment
- [x] `#150` TernaryUpdate — gradiente→{-1,0,+1}
- [x] `#151` ReplayBuffer — ring buffer 10K
- [x] `#152` WeightConsolidation — snapshot + metadata
- [x] `#158` WorkflowPredictor — confidence scoring
- [x] `#159` AutoSkillGen — WASM templates
- [x] `#160` DynamicScaler — heap_target por pressure
- [x] `#161` SelfOptScheduler — timeslice por latência
- [x] `#162` WorkflowProfile — JSON export
- [x] `#169` CodebookVQ — 256 codes × 64 dim nearest-neighbor
- [x] `#170` KV Cache Codebook — compress/decompress
- [x] `#171` ReActLoop — Thought→Action→Observation
- [x] `#172` McpServer — tools/list, tools/call
- [x] `#173` CodebookFinetune — centroid adjustment
- [x] `#174` DeltaBranches — speculative draft/verify
- [x] `#175` WorkspaceIsolation — sandbox heap per agent
- [x] M2 EpisodicMemory — ring buffer 1000
- [x] M37 SleepCycleGuard — blocked words per phase
- [x] M38 BitNetTrainer — train_step + ternary_update
- [x] M39 CandleSidecar — stub connect/train/loss
- [x] M40 TaskSpawner — max 16 children
- [x] M41 ThreeDataSources — replay, feedback, episodic

---

## SPRINT 96 — Bloco 41: Self-Healing Avançado (~350 LOC) ✅
**Objetivo:** Sistema de auto-recuperação, VFS vetorial, taxonomia de falhas, notification gate.
**Status:** ✅ Completo (v0.96.0-heal)

### Itens
- [x] `#226-227` TeamMemory + snapshot versioning
- [x] `#265-266` VectorFs + Vector API (384-dim dot product)
- [x] `#267` OverlayFS — multi-layer mount
- [x] M1 ZeroCopySfs — slice refs, 256-byte dir index
- [x] M3 SkillModule — fn ptr import + version
- [x] M6-M14 FailureTaxonomy (5 classes), ExceptionSelfHeal, CorrectivePrompting, Verifier, EventLog, BudgetedRecovery, SilentFailureDetector, MultiLevelFailure, FailurePrediction
- [x] M29 NotificationGate — allow list + block/deliver counters

### Runtime Fixes
- [x] RTL8139 RX debug rate-limited (1/100 chamadas)
- [x] Scheduler skipa agentes passivos (>50 consecutive Pending → 80% skip)
- [x] `has_event` depende de `ScheduleKind` real, não hardcoded

---

## SPRINT 97 — Bloco 42: JARVIS Desktop + Memory Systems (~1200 LOC) 🟡
**Objetivo:** Finalizar desktop multi-window, temas, crossfade + sistemas de memória avançados.
**Dependências:** Nenhuma (independe de B-01/hardware)

### Itens
- [ ] `#279d` Compositor multi-window (dock, menus, drag) ~300 LOC
- [ ] `#279b` Sistema de temas (5+ cores) ~200 LOC
- [ ] `#283b` Crossfade workspaces ~100 LOC
- [ ] `#214` SHA-256 Memory Dedup (5min sliding window) ~100 LOC
- [ ] `#215` Privacy Filter (strip secrets) ~80 LOC
- [ ] `#216` Memory TTL/Eviction (TTL, ImportanceRank, AccessFreq) ~150 LOC
- [ ] `#219` Ebbinghaus Decay for TrustCache ~120 LOC
- [ ] `#222` Metacognitive Guard (check past mistakes) ~300 loc
- [ ] `#224` Atkinson-Shiffrin 3-tier memory (Sensory→STM→LTM) ~300 LOC
- [ ] `#225` Bi-temporal Knowledge Graph (append-only, validity windows) ~200 LOC
- [ ] Scheduler: StateGraph init + event-based activation ~200 LOC

---

## MAPA DE DEPENDÊNCIAS (DAG Simplificado)

```
Sprint 84 (GPU Found.) ──→ Sprint 85 (GPU Decode)
                              │
Sprint 86 (JARVIS Persona) ──→ Sprint 88 (JARVIS Emotion)
                              │
Sprint 89 (SleepCycle+Memory) ──→ Sprint 90 (JARVIS Deep Cognitive)
                                    │
Sprint 91 (Polimento) ──→ Sprint 92+ (AIOS Evolution) ←── B-01 (🔴)
                                                            │
                              ⏳ Pós-MVP ←── Infraestrutura madura
                              💰 Sponsor ←── HW AMD APU

                            ═══ NOVOS (Sprints 95-97) ═══

Sprint 95 (Cognitive Engine) ✅ ──→ Sprint 96 (Self-Healing) ✅
                                                            │
                            Sprint 97 (Desktop + Memory) 🟡 ←──╯
                                                            │
                              ⏳ Pós-MVP / HW real ←── infraestrutura madura
```

## COMO USAR ESTE ARQUIVO

1. **Escolha um sprint** com status 🟡 (em andamento) ou ⏳ (previsto)
2. **Leia as fontes:** IDEA_BANK.md para o item específico, ADR correspondente
3. **Verifique dependências** no campo "Depende de"
4. **Implemente** os sub-itens na ordem listada
5. **Marque** [x] os itens concluídos
6. **Busque na internet** se encontrar 🔴 bloqueado — nunca fique parado
