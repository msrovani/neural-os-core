# Sprint Plan 92-99 — neural-os-core v0.92.x-0.99.x
# TODAS AS 406+ IDEIAS DO IDEA_BANK ASSIGNADAS A SPRINTS

**Data:** 2026-07-06  
**Contexto:** Sprints 1-91 completos (~35.000 LOC). 46 novas ideias (#361-#406) extraídas de ADRs.  
**Premissa:** Toda ideia do IDEA_BANK tem sprint definido. Nada de "futuro" sem destino.

---

## Sprint 92 — Bloco LAN: Rede Funcional + Dependências (~3000 LOC)
**IDEA_BANK:** B-01, #117-124, #250-255, #186-189, #241-247, #306a-d  
**Depende de:** Nada (é o gatekeeper)  
**Foco:** Destravar a rede para todo o ecossistema AIOS

| IDEA | Item | LOC | Depende |
|---|---|---|---|
| **B-01** | RX fix (RTL8139 DHCP/RX) — buscar na internet | 500 | — |
| #117 | NIC driver genérico (detecta por PCI, busca driver online) | 400 | B-01 |
| #118-120 | smoltcp + DNS + HTTP stack | 500 | B-01 |
| #250 | /ping command | 50 | B-01 |
| #251-252 | DHCP/ARP com timeout + fallback | 200 | B-01 |
| #186 | AppForge / App Store (catalog, install one-click) | 1500 | B-01 |
| #187 | Multi-User / Multi-Persona (memória isolada) | 600 | B-01 |
| #188 | Visual Workflow Builder (DAG pipeline) | 500 | B-01, GPU |
| #189 | Federated Cluster / P2P Workers | 800 | B-01 |
| #241 | OpenTelemetry-Like Observability | 500 | B-01 |
| #242 | AI-Driven Security Scan for Skills | 350 | B-01 |
| #243 | Hub Discovery / Multi-Instance Board | 400 | B-01 |
| #244 | Human-in-the-Loop Approval | 250 | B-01 |
| #245 | Remote Agent Execution | 800 | B-01 |
| #246 | Skill Marketplace | 600 | B-01 |
| #247 | Automatic Context Compaction Agent | 300 | B-01 |
| #306a | Windows PE32+ loader | 600 | B-01 |
| #306b | Linux ELF loader | 500 | B-01 |
| #306c | macOS Mach-O loader | 400 | B-01 |
| #306d | Android APK compat | 500 | B-01 |
| **M4** | Zero-Trust Syscall Categories (4-class) | 100 | B-01 |
| **M5** | Neural Cache decisions per token | 150 | B-01 |

---

## Sprint 93 — Bloco WASM: Runtime + Skills (~4000 LOC)
**IDEA_BANK:** #103-104, #309a-c, M31-M36, M42-M43, M45  
**Depende de:** B-01 (para download de WASM modules)  
**Foco:** WASM embedder, skill runtime, IDE, marketplace

| IDEA | Item | LOC |
|---|---|---|
| #103 | WASM embedder (wasmi no_std) | 500 |
| #104 | Linear memory pool (256 KB/skill) | 100 |
| #309a | WASM Skill Runtime (wasmi v0.42+, fuel metering) | 800 |
| #309b | IDE Agent (BitNet IDE) | 2000 |
| #309c | Hybrid Agents (kernel + WASM) | 100 |
| **M31** | AgentManifest JSON format spec | — |
| **M32** | Developer contract for WASM agents | 80 |
| **M33** | 15 WASI→skill mappings | 350 |
| **M34** | BitNet IDE with HOWTO feature | 2000 |
| **M35** | Marketplace/App Store agent | 400 |
| **M36** | DiskMonitor example agent | 70 |
| **M42** | WASM linear memory pool (256KB) | 100 |
| **M43** | Skill ABI design | 100 |
| **M45** | Capability token cryptographically signed | 100 |

---

## Sprint 94 — Bloco Voice + Vision: TTS/STT/Camera (~3000 LOC)
**IDEA_BANK:** #315.21-25, #360, #79-82  
**Depende de:** B-01 (download de modelos), B-01 (STT cloud)  
**Foco:** Kokoro TTS, Vosk STT, wake word, Wyoming pipeline, camera

| IDEA | Item | LOC |
|---|---|---|
| #315.21 | Kokoro-82M TTS Integration (ONNX→.bitnet já pronto) | 100 |
| #315.22 | Vosk/Whisper STT | 400 |
| #315.23 | Wake Word (Rustpotter) | 100 |
| #315.24 | Wyoming Protocol IPC | 300 |
| #315.25 | Voice Pipeline (8-domain) | 200 |
| #360 | Kokoro-82M download + conversão | 300 |
| #79 | UEFI framebuffer renderização de fontes | 200 |
| #80 | Font rendering para alta resolução | 200 |
| #81 | VirtIO-GPU 2D/3D acelerado | 400 |
| #82 | Tensor visualization no framebuffer | 300 |

---

## Sprint 95 — Bloco Cognitive: Aprendizado + Memória Avançada (~3000 LOC)
**IDEA_BANK:** #105-108, #149-152, #158-162, #169-175, M37-M41, M2  
**Depende de:** Nada (CPU-only)  
**Foco:** Success Engine, feedback loop, EWC, codebook VQ, on-device learning

| IDEA | Item | LOC |
|---|---|---|
| #105 | Intent Planner (sequência de SkillCommands) | 300 |
| #106 | Success Engine (feedback loop online) | 400 |
| #107 | Neural Cache (lookup table 50ns) | 300 |
| #108 | MatMul-free LM (RWKV/Mamba) | 500 |
| #149 | Feedback loop — usuário avalia resposta | 150 |
| #150 | Ternary weight update | 200 |
| #151 | Experience replay buffer | 200 |
| #152 | Weight consolidation (export modelo) | 150 |
| #158 | Workflow Predictor (pré-carrega recursos) | 200 |
| #159 | Auto-Skill Generator (cria skill WASM) | 300 |
| #160 | Dynamic Resource Scaling (MHI auto-ajuste) | 200 |
| #161 | Self-Optimizing Scheduler | 300 |
| #162 | Workflow Profile exportável | 200 |
| #169 | Codebook Compression (VQ) | 300 |
| #170 | KV Cache Codebook | 200 |
| #171 | ReAct loop com auto-correção | 300 |
| #172 | MCP Server support | 400 |
| #173 | Codebook LLM finetune | 300 |
| #174 | Delta branches (speculative decoding) | 300 |
| #175 | Workspace isolation | 200 |
| **M2** | Episodic memory via battery-backed NVMe | 200 |
| **M37** | SleepCycle guard rails per phase | 100 |
| **M38** | BitNetTrainer implementation | 300 |
| **M39** | Candle Trainer sidecar | 500 |
| **M40** | Task Spawner (ELF loader) | 500 |
| **M41** | Three data sources for on-device training | — |

---

## Sprint 96 — Bloco Self-Heal + Security: Resiliência (~2500 LOC)
**IDEA_BANK:** #226-227, #265-267, M6-M14, M1, M3, M29  
**Depende de:** Nada  
**Foco:** Self-healing avançado, audit trail, vector FS

| IDEA | Item | LOC |
|---|---|---|
| #226 | Team/Shared Memory | 400 |
| #227 | Memory Git Snapshots | 500 |
| #265 | Filesystem como Vector Search | 300 |
| #266 | Multi-dialect Vector API | 300 |
| #267 | OverlayFS Copy-on-Write | 200 |
| **M1** | Zero-Copy SFS via zerocopy crate | 100 |
| **M3** | Skills-as-Modules capability import | 150 |
| **M6** | Failure Taxonomy Enum | 30 |
| **M7** | Exception Handlers + SelfHeal | 80 |
| **M8** | Corrective Prompting | 60 |
| **M9** | Verifier Pós-Recovery | 80 |
| **M10** | Erros no EventLog | 10 |
| **M11** | Budgeted Recovery | 30 |
| **M12** | Silent Failure Detection | 60 |
| **M13** | Multi-level Failure Architecture | 100 |
| **M14** | Failure Prediction | 100 |
| **M29** | J.A.R.V.I.S. Notification Gate (detalhado) | 150 |

---

## Sprint 97 — Bloco AIOS: Cross-OS + Update + Mesh (~3000 LOC)
**IDEA_BANK:** #306-310, #307, #308a-c, M23-M30, M46  
**Depende de:** B-01, WASM (Sprint 93)  
**Foco:** Sistema de updates, compatibilidade cross-OS, mesh

| IDEA | Item | LOC |
|---|---|---|
| #307 | Syscall-to-Skill Translation Layer | 500 |
| #308a | Update/Upgrade Agent (A/B slots) | 500 |
| #308b | Update channels (stable/nightly/security) | 200 |
| #308c | Rollback automático | 100 |
| #310a | J.A.R.V.I.S. Layer (persona completa) | 500 |
| #310b | Stack final Boot→Kernel→Cortex→Hermes→JARVIS | 200 |
| **M23** | Detalhado WASI→Skill mapping (20 syscalls) | 200 |
| **M24** | Tier 0-4 Agent Classification | 100 |
| **M25** | WASM Host Function Interface signatures | 80 |
| **M26** | Performance budget table (kernel vs WASM) | — |
| **M27** | ChromeOS A/B update reference | — |
| **M28** | J.A.R.V.I.S. Context Window Manager | 500 |
| **M30** | Update channel strategy (detalhado) | 100 |
| **M46** | VirtIO-GPU GET_DISPLAY_INFO pending fix | 50 |

---

## Sprint 98 — Bloco NPU + GPU Polish (~3000 LOC)
**IDEA_BANK:** #43-52, #333-336, M18-M22  
**Depende de:** 💰 HW AMD APU (para NPU), GPU já funcional  
**Foco:** NPU AMD XDNA driver, GPU compute polimento

| IDEA | Item | LOC |
|---|---|---|
| #43-52 | NPU AMD XDNA driver completo | 3000 |
| #333 | burn-flex backend (SIMD gemm profissional) | 800 |
| #334 | MSched evicção VRAM (Belady/OPT) | 500 |
| #335 | CFS scheduler (vruntime-based fairness) | 500 |
| #336 | GPU + Display co-existência | 300 |
| **M18** | Per-vendor GPU driver LOC (NVIDIA PFIFO, AMD PM4) | 4600 |
| **M19** | NVIDIA Pascal Push Buffer channel layout | — |
| **M20** | AMD RDNA PM4 packet types | — |
| **M21** | Model swap flow (/model <path>) | 80 |
| **M22** | iGPU display + dGPU compute architecture | — |

---

## Sprint 99+ — Bloco Meta: Extras (~2000 LOC)
**IDEA_BANK:** #212-213, #278a-b, #315.26-28, #210-211  
**Depende de:** Infraestrutura madura  
**Foco:** GGUF loader, plugin system, gamification

| IDEA | Item | LOC |
|---|---|---|
| #210 | Subagent Crash-Recovery Persistence | 600 |
| #211 | ComputeBackend Trait | 800 |
| #212 | Plugin System via Loadable Page Ranges | 400 |
| #213 | WASM + Docker Sandbox for Skills | 500 |
| #278a | GGUF loader mínimo | 500 |
| #278b | .bitnet v3 header extensível | 200 |
| #315.26 | Multi-device sync (CRDT) | 300 |
| #315.27 | SKYNET Mesh Node | 300 |
| #315.28 | Gamification | 200 |
| **M44** | NVMe submission/completion queue | 800 |

---

## ❌ Descartados
| IDEA | Item | Motivo |
|---|---|---|
| #83 | Intel HDA audio driver | Sem skill de áudio no roadmap |
| #84 | Áudio via USB (UAC) | USB + áudio = duplo pós-MVP |
| #116 | Port ARM/RISC-V | Fora do escopo x86-64 atual |
| #248 | Docker Sandbox | Incompatível com bare-metal no_std |
| #249 | Python/.NET Runtime | Barreira de linguagem |
| #357 | khal-std | Requer wgpu (std-only) |

---

## Resumo de Esforço

| Sprint | Bloco | LOC | Depende de |
|---|---|---|---|
| 92 | LAN: Rede + Dependências | ~3000 | — |
| 93 | WASM: Runtime + Skills | ~4000 | B-01 |
| 94 | Voice + Vision | ~3000 | B-01 |
| 95 | Cognitive: Aprendizado | ~3000 | — |
| 96 | Self-Heal + Security | ~2500 | — |
| 97 | AIOS: Cross-OS + Update | ~3000 | B-01 + WASM |
| 98 | NPU + GPU Polish | ~3000 | HW AMD APU |
| 99+ | Meta: Extras | ~2000 | Infra madura |
| **Total** | | **~23.500 LOC** | |
