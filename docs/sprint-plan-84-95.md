# Sprint Plan 84-95 — neural-os-core v0.84.x-0.95.x
# TODAS AS 358 IDEIAS DO IDEA_BANK ASSIGNADAS A SPRINTS

**Data:** 2026-07-06 (v2 — ADR-0038: Otimização do Ecossistema)  
**Contexto:** Bloco 21a/21b/21e completos (SMP Foundation, Work-Stealing, Polimento).  
**Próximos blocos:** GPU Foundations (84) ✅ → GPU Decode (85) ✅ → JARVIS Persona+Alloc (86) → Security+AHCI (87) → JARVIS Emotion+Cache+DHCP (88) → SleepCycle+Memory (89) → JARVIS Deep Cognitive (90) → Polimento (91) → AIOS Evolution (92+).  
**Novo:** Sprint 86 incorpora #355 buddy-slab-allocator. Sprint 88 incorpora #356 edge-dhcp (B-01).  
**Premissa:** HW real é o único critério. QEMU/VBox são dev/debug. Toda solução bloqueada exige busca ativa na internet.

---

## Sprint 84 — Bloco 21c: GPU Foundations (~1700 LOC)
**IDEA_BANK:** #67, #326, #327, #328, #352, #353  
**ADR:** 0037 (itens 5-9), 0029  
**Foco:** BAR mapping + secure boot (NVIDIA ACR / AMD PSP / Intel GuC) + doorbell + job ring + VRAM allocator

| IDEA | Item | LOC | Status |
|---|---|---|---|
| #67 | AllocTier::Vram — alocar no BAR da GPU | 50 | 🟡 |
| #326 | GPU BAR0/BAR1 mapping UC (genérico NVIDIA/AMD/Intel) | 300 | 🟡 |
| #327 | GPU doorbell + SPSC job ring | 400 | 🟡 |
| #328 | VRAM buddy allocator | 400 | 🟡 |
| #352 | Secure Boot GPU — ACR/PSP/GuC pipeline | 600 | 🟡 |
| #353 | GPU Compute Pipeline — submissão genérica | 300 | 🟡 |

**Bloqueios:** Nenhum (NVMe ✅, PCI ✅, BAR mapping disponível)

---

## Sprint 85 — Bloco 21d: GPU Decode (BitNet offload) (~1500 LOC)
**IDEA_BANK:** #329, #330, #331, #332  
**ADR:** 0037 (itens 6-9)  
**Foco:** Prefill CPU → decode GPU, matmul ternário na GPU, KV cache DMA, XQueue

| IDEA | Item | LOC | Status |
|---|---|---|---|
| #329 | Agent.xpu prefill/decode split | 400 | 🟡 |
| #330 | GPU matmul kernel ternário (NVIDIA PTX / AMD AQL / Intel GEN) | 300 | 🟡 |
| #331 | CPU→GPU KV cache DMA | 200 | 🟡 |
| #332 | XQueue preemptível (XSched-style, 3 níveis) | 600 | 🟡 |

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

## Sprint 89 — Bloco 33: SleepCycle + Advanced Memory (~2500 LOC)
**IDEA_BANK:** #314, #214, #215, #216, #219, #217, #218, #222, #223, #224, #225  
**ADR:** 0023 (Memory Systems)  
**Foco:** Experiências de aprendizado onírico e memória avançada

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

---

## Sprint 90 — Bloco 34: JARVIS Deep Cognitive (~1200 LOC)
**IDEA_BANK:** #315.12, #315.13, #315.14, #315.15, #315.16, #315.17  
**ADR:** 0036  
**Foco:** Dreaming/consolidation, Ego layer, heartbeats, auto-skills, Babel-Index

| IDEA | Item | LOC | Status |
|---|---|---|---|
| #315.12 | Dreaming/Consolidation (CronAgent noturno, insights sintéticos) | 200 | 🟡 |
| #315.13 | Ego Layer (self-model, confidence tracking, can_answer per domain) | 250 | 🟡 |
| #315.14 | Proactive Heartbeats (JARVIS inicia conversa por eventos) | 100 | 🟡 |
| #315.15 | Tool-State Save Game (snapshot + rollback de skills) | 100 | 🟡 |
| #315.16 | Auto-Skill Generation (watch→pattern→propose→generate→register) | 150 | 🟡 |
| #315.17 | Babel-Index (entropy + contradiction + staleness monitoring) | 100 | 🟡 |

---

## Sprint 91 — Bloco 35: Polimento + Ecosystem (~2500 LOC)
**IDEA_BANK:** #333, #334, #335, #336, #280l, #279a, #279b, #279c, #283a, #283b  
**ADR:** 0037, 0020 (Crom), 0021 (Life OS)  
**Foco:** burn-flex, MSched VRAM, CFS scheduler, GPU+Display, SmileyOS patterns, Desktop Cube

| IDEA | Item | LOC | Status |
|---|---|---|---|
| #333 | burn-flex backend port (elimina bitnet_avx2 manual, 2-95× speedup) | 800 | 🟡 |
| #334 | MSched evicção VRAM (Belady/OPT prediction) | 500 | 🟡 |
| #335 | CFS scheduler (vruntime-based fairness) | 500 | 🟡 |
| #336 | GPU + Display co-existência (iGPU display, dGPU compute) | 300 | 🟡 |
| #280l | SkillManifest derive macro (proc-macro para manifests) | 100 | 🟡 |
| #279a | Shell com 40+ comandos (ls, cat, ps, uptime, theme) | 300 | 🟡 |
| #279b | Sistema de temas (5+ cores, hot-swap) | 200 | 🟡 |
| #279c | Filesystem próprio com permissões (VFS upgrade) | 400 | 🟡 |
| #283a | Workspace Cube 3D com rotação via GPU | 200 | 🟡 |
| #283b | Transição crossfade entre workspaces | 100 | 🟡 |

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
| #315.21-25 | Voice Pipeline (Piper TTS + Vosk STT + Wake Word + Wyoming) | 1600 | 🔴 B-01 |
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

## Resumo de Esforço (Sprints 84-95)

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
| | | **Total sprints** | **~28.250 LOC** | |

**Nota:** Itens 💰 Sponsor ou ⏳ Pós-MVP não contam no total — serão ativados quando HW ou dependências estiverem disponíveis.
