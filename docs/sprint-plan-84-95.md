# Sprint Plan 84-97 — neural-os-core v0.84.x-0.97.x
# TODAS AS IDEIAS DO IDEA_BANK ASSIGNADAS A SPRINTS

**Data:** 2026-07-06 (v2 — ADR-0038: Otimização do Ecossistema)  
**Contexto:** Bloco 21a/21b/21e completos (SMP Foundation, Work-Stealing, Polimento).  
**Próximos blocos:** GPU Foundations (84) ✅ → GPU Decode (85) ✅ → JARVIS Persona+Alloc (86) → Security+AHCI (87) → JARVIS Emotion+Cache+DHCP (88) → SleepCycle+Memory (89) → JARVIS Deep Cognitive (90) → Polimento (91) → AIOS Evolution (92+).  
**Novo:** Sprint 86 incorpora #355 buddy-slab-allocator. Sprint 88 incorpora #356 edge-dhcp (B-01).  
**Premissa:** HW real é o único critério. QEMU/VBox são dev/debug. Toda solução bloqueada exige busca ativa na internet.

---

## Sprint 84 — Bloco 21c: GPU Foundations (~1700 LOC) ✅
**IDEA_BANK:** #67, #326, #327, #328, #352, #353  
**ADR:** 0037 (itens 5-9), 0029  
**Foco:** BAR mapping + secure boot (NVIDIA ACR / AMD PSP / Intel GuC) + doorbell + job ring + VRAM allocator  
**Status:** ✅ **Completo.** Todos os itens implementados em `gpu/{detect,backend,vram,ring,intel,nvidia,amd,firmware}.rs`.

| IDEA | Item | LOC | Status |
|---|---|---|---|
| #67 | AllocTier::Vram — alocar no BAR da GPU | 50 | ✅ |
| #326 | GPU BAR0/BAR1 mapping UC (genérico NVIDIA/AMD/Intel) | 300 | ✅ |
| #327 | GPU doorbell + SPSC job ring | 400 | ✅ |
| #328 | VRAM buddy allocator | 400 | ✅ |
| #352 | Secure Boot GPU — ACR/PSP/GuC pipeline | 600 | ✅ |
| #353 | GPU Compute Pipeline — submissão genérica | 300 | ✅ |

---

## Sprint 85 — Bloco 21d: GPU Decode (BitNet offload) (~1500 LOC) ✅
**IDEA_BANK:** #329, #330, #331, #332  
**ADR:** 0037 (itens 6-9)  
**Foco:** Prefill CPU → decode GPU, matmul ternário na GPU, KV cache DMA, XQueue  
**Status:** ✅ **Completo.** Todos os itens implementados em `gpu/{xpu,backend,kv_dma,xqueue}.rs`.

| IDEA | Item | LOC | Status |
|---|---|---|---|
| #329 | Agent.xpu prefill/decode split | 400 | ✅ |
| #330 | GPU matmul kernel ternário (NVIDIA PTX / AMD AQL / Intel GEN) | 300 | ✅ |
| #331 | CPU→GPU KV cache DMA | 200 | ✅ |
| #332 | XQueue preemptível (XSched-style, 3 níveis) | 600 | ✅ |

---

## Sprint 86 — Bloco 30: JARVIS Persona + Alocador Otimizado (~1300 LOC)
**IDEA_BANK:** #315.1, #315.2, #315.3, #315.4, #315.5, #355  
**ADR:** 0036 (JARVIS Unified Layer), 0038 (Ecosystem Optimization)  
**Foco:** SOUL.md, IPW Monitor, Session Compression, Notification Gate, Sessionless Thread, **buddy-slab-allocator integration**

| IDEA | Item | LOC | Status |
|---|---|---|---|---|
| #315.1 | SOUL.md Personality Engine | 300 | 🟡 |
| #315.2 | IPW Monitor (RAPL MSR 0x610) | 150 | 🟡 |
| #315.3 | Session Compression (4 strategies) | 200 | 🟡 |
| #315.4 | Notification Gate (4 urgency levels) | 200 | 🟡 |
| #315.5 | Sessionless Thread | 100 | 🟡 |
| #355 | buddy-slab-allocator integration (substitui slab.rs + vram.rs backend) | 300 | 🟡 |

---

## Sprint 87 — Bloco 31: JARVIS Security + AHCI (~1200 LOC)
**IDEA_BANK:** #315.18, #315.19, #315.20, + AHCI driver  
**ADR:** 0036, 0018 (Security Pipeline)  
**Foco:** Fail-closed, Merkle trail, Fluid persona, SATA driver

| IDEA | Item | LOC | Status |
|---|---|---|---|
| #315.18 | Fail-Closed Safety Invariant (SMT-proof, 4 invariants) | 200 | 🟡 |
| #315.19 | Merkle Audit Trail (Ed25519 chain, ring 4096) | 200 | 🟡 |
| #315.20 | Fluid Persona (context-adaptive, coach/tutor/tool) | 100 | 🟡 |
| — | AHCI driver (SATA 6G NCQ) | 700 | 🟡 |

---

## Sprint 88 — Bloco 32: JARVIS Emotion + Cache + Pipeline + DHCP (~1400 LOC)
**IDEA_BANK:** #315.6, #315.7, #315.8, #315.9, #315.10, #315.11, #356  
**ADR:** 0036, 0038 (Ecosystem Optimization)  
**Foco:** Emotion analysis, Capability contracts, Skill discovery, ADE pipeline, Semantic cache, Persona pipeline, **edge-dhcp integration (B-01)**

| IDEA | Item | LOC | Status |
|---|---|---|---|
| #315.6 | Emotion Analysis — BitNet 7 emoções + intensity + sarcasm + adjust_tone | 250 | 🟡 |
| #315.7 | Capability Contract + Consent Gates (Safe/Moderate/Dangerous) | 200 | 🟡 |
| #315.8 | Skill Discovery (DSPy/ACE — observe→analyze→propose→generate) | 300 | 🟡 |
| #315.9 | ADE Pipeline (Spec→Execute→Review→Recover) | 200 | 🟡 |
| #315.10 | Semantic Cache (5-tier routing, 97.5% reduction) | 150 | 🟡 |
| #315.11 | Persona Pipeline (16 stages, OVOS-inspired) | 100 | 🟡 |
| #356 | edge-dhcp integration — DHCP no_std + no-alloc como fallback B-01 | 200 | 🟡 |

---

## Sprint 89 — Bloco 33: SleepCycle + Advanced Memory + Embedding (~2800 LOC)
**IDEA_BANK:** #314, #214, #215, #216, #219, #217, #218, #222, #223, #224, #225, #359  
**ADR:** 0023 (Memory Systems), 0038 (Ecosystem Optimization)  
**Foco:** Experiências de aprendizado onírico e memória avançada, **BGE-Small-EN-v1.5 embedding**

| IDEA | Item | LOC | Status |
|---|---|---|---|
| #314 | SleepCycle Agent (5 fases: REPLAY→DREAM→CONSOLIDATE→PRUNE→REFLECT) | 780 | 🟡 |
| #214 | SHA-256 Memory Dedup (5min sliding window) | 100 | 🟡 |
| #215 | Privacy Filter (strip secrets before memory storage) | 80 | 🟡 |
| #216 | Memory TTL/Eviction (TimeToLive, ImportanceRank, AccessFrequency) | 150 | 🟡 |
| #219 | Ebbinghaus Decay for TrustCache (strength = importance × e^(-λ·days)) | 120 | 🟡 |
| #217 | Hybrid Search (BM25 + MLP) — RRF fusion with k=60 | 200 | 🟡 |
| #218 | 4-Tier Memory Consolidation (Working→Episodic→Semantic→Procedural) | 400 | 🟡 |
| #222 | Metacognitive Guard (check past mistakes before skill execution) | 300 | 🟡 |
| #223 | Draft→Review→Merge Memory (approval workflow) | 350 | 🟡 |
| #224 | Atkinson-Shiffrin 3-tier (Sensory Register 48h→STM 7d→LTM permanent) | 800 | 🟡 |
| #225 | Bi-temporal Knowledge Graph (append-only, validity windows) | 600 | 🟡 |
| #359 | BGE-Small-EN-v1.5 embedding — converter ONNX→.bitnet, skill semantic_search | 300 | 🟡 |

---

## Sprint 90 — Bloco 34: JARVIS Deep Cognitive ✅
**IDEA_BANK:** #315.12, #315.13, #315.14, #315.15, #315.16, #315.17  
**ADR:** 0036  
**Foco:** Dreaming/consolidation, Ego layer, heartbeats, auto-skills, Babel-Index  
**Status:** ✅ Completo (v0.90.0-cognitive)

| IDEA | Item | LOC | Status |
|---|---|---|---|
| #315.12 | Dreaming/Consolidation | 200 | ✅ |
| #315.13 | Ego Layer (self-model, confidence) | 250 | ✅ |
| #315.14 | Proactive Heartbeats | 100 | ✅ |
| #315.15 | Tool-State Save Game | 100 | ✅ |
| #315.16 | Auto-Skill Generation | 150 | ✅ |
| #315.17 | Babel-Index (entropy monitor) | 100 | ✅ |

---

## Sprint 91 — Bloco 35: Polimento + Ecosystem ✅
**IDEA_BANK:** #333, #334, #335, #336, #280l, #279a, #279b, #279c, #283a, #283b  
**ADR:** 0037, 0020 (Crom), 0021 (Life OS)  
**Foco:** Shell 40+ comandos, temas, VFS perms, CFS, MSched, Cube  
**Status:** ✅ Completo (v0.91.1-full) — stubs para burn-flex, MSched, CFS aguardando uso

| IDEA | Item | LOC | Status |
|---|---|---|---|
| #333 | burn-flex backend port | 800 | 🟡 stub |
| #334 | MSched evicção VRAM (Belady/OPT) | 500 | 🟡 stub |
| #335 | CFS scheduler (vruntime-based) | 500 | ✅ impl |
| #336 | GPU + Display co-existência | 300 | 🟡 doc |
| #280l | SkillManifest derive macro | 100 | 🟡 ref |
| #279a | Shell 40+ comandos | 300 | ✅ |
| #279b | Sistema de temas (5+ cores) | 200 | ✅ |
| #279c | VFS permissões | 400 | ✅ |
| #283a | Workspace Cube 3D | 200 | ✅ |
| #283b | Crossfade workspaces | 100 | ✅ |

---

## Sprint 92+ — Bloco 36+: AIOS Evolution (~15000 LOC)
**🔴 BLOQUEADO POR B-01** — Primeira ação: buscar na internet soluções de DHCP+RTL8139  
**IDEA_BANK:** #117-124, #250-255, #306-310, #315.21-315.28, #277a-c, #282e-h, #307, #308a-c, #309a-c  
**ADR:** 0016 (Network), 0031 (AIOS), 0032 (WASM), 0036 (JARVIS)

| Item | O que | LOC | Bloqueador |
|---|---|---|---|
| B-01 | RX fix (RTL8139 DHCP/RX) — **buscar na internet** | 500 | 🔴 |
| #117 | NIC driver genérico (detecta por PCI vendor/device, busca driver online) | 400 | 🔴 B-01 |
| #118-120 | smoltcp + DNS + HTTP stack | 500 | 🔴 B-01 |
| #250 | /ping command | 50 | 🔴 B-01 |
| #251-252 | DHCP/ARP com timeout + fallback | 200 | 🔴 B-01 |
| #307 | WWW Agents (Browser, Email, Search, RSS, Download, WS) | 2600 | 🔴 B-01 |
| #308a-c | Self-Update Agent (A/B slots + rollback) | 800 | 🔴 B-01 |
| #309a-c | WASM Skill Runtime + IDE Agent + Hybrid Agents | 2900 | 🔴 B-01 |
| #315.21-25 | Voice Pipeline (Kokoro-82M TTS + Vosk STT + Wake Word + Wyoming) | 1600 | 🔴 B-01 |
| #360 | Kokoro-82M TTS — converter ONNX→.bitnet (ferramenta pronta, aguarda modelo) | 300 | 🟡 ferramenta pronta |
| #315.26 | Multi-device sync (CRDT, Automerge-style) | 300 | 🔴 B-01 |
| #315.27 | SKYNET Mesh Node (speculative decoding distribuído) | 300 | 🔴 B-01 |
| #277a-c | The Agency — HwRegistry, Agency struct, LLM-aware activation | 800 | 🔴 B-01 |
| #282e-h | InferenceFsAgent, HermesFsAgent, RamFsAgent, MhiScheduler | 600 | 🔴 B-01 |
| #279d | Compositor multi-window (dock, menus, drag) | 600 | 🟡 |
| #279e | v86 browser demo (WebAssembly x86 emulator) | 500 | 🟡 |
| #306a-d | Cross-OS compat (PE/ELF/Mach-O/APK loaders) | 2000 | 🔴 B-01 |
| B-29 | WiFi (Intel/Atheros/Realtek 802.11) | 1000 | 🔴 B-01 |
| #186-189 | AppForge, Multi-User, Workflow Builder, Federated Cluster | 3000 | 🔴 B-01 |

---

## Itens Pós-MVP / Sponsor (sem sprint definido, dependem de HW ou maturidade)

| IDEA | Item | Destino | Depende de |
|---|---|---|---|
| #1-15 | USB stack completo (xHCI, device identity, WASM dispatch) | ⏳ Pós-MVP | PCI, IOAPIC |
| #43-52 | NPU AMD XDNA driver | 💰 Sponsor | HW AMD APU |
| #68-69 | AllocTier::Nvme / Hdd (SFS-based) | ⏳ Pós-MVP | NVMe driver, SFS |
| #79-80 | UEFI framebuffer + font rendering | 🟡 Sprint 92+ | Framebuffer (✅) |
| #92-93 | Huge Pages 2MiB / 1GiB | 🟡 Sprint 92+ | Page table mapper |
| #103-104 | WASM embedder + linear memory pool | 🟡 Sprint 92+ | Scheduler |
| #105-108 | Success Engine, Neural Cache, MatMul-free LM | ⏳ Pós-MVP | Cognitive runtime |
| #149-152 | Feedback loop, ternary weight update, replay buffer, consolidation | ⏳ Pós-MVP | Success Engine |
| #158-159 | Workflow Predictor, Auto-Skill (DSPy) | 🟡 Sprint 92+ | Usage Analyzer |
| #162 | Workflow Profile exportável | 🟡 Sprint 92+ | Workflow Predictor |
| #169-175 | Codebook VQ, KV cache codebook, ReAct loop, MCP Server, Delta branches | 🟡 Sprint 92+ | Transformer Engine |
| #186-189 | AppForge, Multi-User, Workflow Builder, Federated Cluster | 🔴 Pós B-01 | B-01 |
| #210-213 | Actor Registry, Crash-Recovery, ComputeBackend, Plugin System | 🔴 Pós B-01 | Agent Framework |
| #226-227 | Team Memory, Memory Git Snapshots | 🔴 Pós B-01 | Memory Systems |
| #241-247 | Observability, AI Security Scan, Hub Discovery, HITL, Remote Exec, Marketplace | 🔴 Pós B-01 | B-01 |
| #265-267 | FS Vector Search, Vector API, OverlayFS | ⏳ Pós-MVP | SFS |
| #278a-b | GGUF loader + .bitnet v3 | 🟡 Sprint 92+ | Heap >5GB |
| #306a-d | Cross-OS compat (PE/ELF/Mach-O/APK) | 🔴 Pós B-01 | B-01 + Update Agent |

---

## Resumo de Esforço (Sprints 84-97)

| Sprint | Bloco | Foco | LOC | Status |
|---|---|---|---|---|
| 84 | 21c | GPU Foundations | ~1700 | 🟡 |
| 85 | 21d | GPU Decode | ~1500 | 🟡 |
| 86 | 30 | JARVIS Persona + Alloc | ~1300 | 🟡 |
| 87 | 31 | JARVIS Security + AHCI | ~1200 | 🟡 |
| 88 | 32 | JARVIS Emotion + Cache + DHCP | ~1400 | 🟡 |
| 89 | 33 | SleepCycle + Memory | ~2500 | 🟡 |
| 90 | 34 | JARVIS Deep Cognitive | ~1200 | 🟡 |
| 91 | 35 | Polimento + Ecosystem | ~2500 | 🟡 |
| 92+ | 36+ | AIOS Evolution | ~15000 | 🔴 |
| **95** | **40** | **Cognitive Engine** | **~510** | **✅** |
| **96** | **41** | **Self-Healing** | **~350** | **✅** |
| **97** | **42** | **JARVIS Desktop + Memory** | **~1200** | **🟡** |
| | | **Total sprints** | **~33.060 LOC** | |

**Nota:** Itens 💰 Sponsor ou ⏳ Pós-MVP não contam no total — serão ativados quando HW ou dependências estiverem disponíveis.

---

## Sprint 95 — Bloco 40: Cognitive Engine (~510 LOC) ✅
**IDEA_BANK:** #105, #106, #107, #108, #149, #150, #151, #152, #158, #159, #160, #161, #162, #169, #170, #171, #172, #173, #174, #175, M2, M37-M41  
**ADR:** 0038 (Ecosystem Optimization)  
**Foco:** Motor cognitivo completo: planejamento, aprendizado, cache, predição, VQ, ReAct  
**Status:** ✅ Completo (v0.95.0-cog)

| IDEA | Item | LOC | Status |
|---|---|---|---|
| #105 | IntentPlanner — SkillSteps com params, goal-based plan | 30 | ✅ |
| #106 | SuccessEngine — win/loss streak, recent_rate 64-window | 25 | ✅ |
| #107 | NeuralCache — TTL + LRU evicção max 4096 | 35 | ✅ |
| #108 | MatMulFreeLM — RWKV-style WKV forward | 25 | ✅ |
| #149 | FeedbackLoop — rating 0-10 + comment | 15 | ✅ |
| #150 | TernaryUpdate — gradiente→{-1,0,+1} com threshold | 20 | ✅ |
| #151 | ReplayBuffer — ring buffer 10K | 25 | ✅ |
| #152 | WeightConsolidation — snapshot + metadata | 20 | ✅ |
| #158 | WorkflowPredictor — confidence scoring | 20 | ✅ |
| #159 | AutoSkillGen — WASM templates | 25 | ✅ |
| #160 | DynamicScaler — heap_target por pressure | 20 | ✅ |
| #161 | SelfOptScheduler — timeslice por latência | 25 | ✅ |
| #162 | WorkflowProfile — JSON export | 15 | ✅ |
| #169 | CodebookVQ — 256 codes × 64 dim | 30 | ✅ |
| #170 | KV Cache Codebook — compress/decompress | 25 | ✅ |
| #171 | ReActLoop — Thought→Action→Observation | 25 | ✅ |
| #172 | McpServer — tools/list, tools/call | 30 | ✅ |
| #173 | CodebookFinetune — centroid adjustment | 20 | ✅ |
| #174 | DeltaBranches — speculative draft/verify | 20 | ✅ |
| #175 | WorkspaceIsolation — sandbox heap per agent | 20 | ✅ |
| M2 | EpisodicMemory — ring buffer 1000 | 15 | ✅ |
| M37 | SleepCycleGuard — blocked words per phase | 15 | ✅ |
| M38 | BitNetTrainer — train_step + ternary_update | 25 | ✅ |
| M39 | CandleSidecar — stub connect/train/loss | 15 | ✅ |
| M40 | TaskSpawner — max 16 children | 15 | ✅ |
| M41 | ThreeDataSources — replay, feedback, episodic | 15 | ✅ |

---

## Sprint 96 — Bloco 41: Self-Healing Avançado (~350 LOC) ✅
**IDEA_BANK:** #226, #227, #265, #266, #267, M1, M3, M6-M14, M29  
**ADR:** 0025 (Tier 3 Security), 0038  
**Foco:** Sistema de auto-recuperação, VFS vetorial, taxonomia de falhas  
**Status:** ✅ Completo (v0.96.0-heal)

| IDEA | Item | LOC | Status |
|---|---|---|---|
| #226 | TeamMemory — BTreeMap compartilhado | 25 | ✅ |
| #227 | Memory Snapshots — versioning | 20 | ✅ |
| #265 | VectorFs — dot product search 384-dim | 40 | ✅ |
| #266 | Vector API (KNN) | 20 | ✅ |
| #267 | OverlayFS — multi-layer mount | 30 | ✅ |
| M1 | ZeroCopySfs — slice refs, 256-byte dir index | 25 | ✅ |
| M3 | SkillModule — fn ptr import + version | 15 | ✅ |
| M6 | FailureTaxonomy — 5 classes | 15 | ✅ |
| M7 | ExceptionSelfHeal — auto analyze/recover | 20 | ✅ |
| M8 | CorrectivePrompting — context + escalation | 15 | ✅ |
| M9 | Verifier — fn check→bool | 10 | ✅ |
| M10 | EventLog — format + persist stub | 15 | ✅ |
| M11 | BudgetedRecovery — attempts/daemon per window | 20 | ✅ |
| M12 | SilentFailureDetector — heartbeat + threshold | 20 | ✅ |
| M13 | MultiLevelFailure — Ok/Warning/Error/Critical | 15 | ✅ |
| M14 | FailurePrediction — trend via window diff | 15 | ✅ |
| M29 | NotificationGate — allow list + counters | 20 | ✅ |

### Runtime Fixes (Sprint 95/96)
- RTL8139 RX debug rate-limited (1/100 chamadas)
- Scheduler skipa agentes passivos (>50 consecutive Pending → 80% skip)
- `has_event` depende de `ScheduleKind` real, não hardcoded

---

## Sprint 97 — Bloco 42: JARVIS Desktop + Memory Systems (~1200 LOC)
**IDEA_BANK:** #279a-e, #283a-b, #214, #215, #216, #217, #218, #219, #222, #223, #224, #225  
**ADR:** 0036 (JARVIS), 0023 (Memory Systems)  
**Foco:** Finalizar desktop (multi-window, temas, crossfade) + sistemas de memória avançados (Ebbinghaus, Atkinson-Shiffrin, dedup, KG)

| IDEA | Item | LOC | Status |
|---|---|---|---|
| #279d | Compositor multi-window (dock, menus, drag) | 300 | 🟡 |
| #279b | Sistema de temas (5+ cores) | 200 | 🟡 |
| #283b | Crossfade workspaces | 100 | 🟡 |
| #214 | SHA-256 Memory Dedup (5min sliding window) | 100 | 🟡 |
| #215 | Privacy Filter (strip secrets before memory) | 80 | 🟡 |
| #216 | Memory TTL/Eviction (TTL, ImportanceRank, AccessFreq) | 150 | 🟡 |
| #219 | Ebbinghaus Decay for TrustCache | 120 | 🟡 |
| #222 | Metacognitive Guard (check past mistakes) | 300 | 🟡 |
| #224 | Atkinson-Shiffrin 3-tier memory | 300 | 🟡 |
| #225 | Bi-temporal Knowledge Graph | 200 | 🟡 |
| — | Scheduler: StateGraph init + event-based activation | 200 | 🟡 |

**Dependências:** Nenhuma (tudo independe de B-01/hardware).  
**Foco:** Polir desktop visual + implementar sistemas de memória real para melhorar a cognição do Hermes Agent.
