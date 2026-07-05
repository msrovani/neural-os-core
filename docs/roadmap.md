# Roadmap — neural-os-core v0.80.1-design 🏆

**Última atualização:** 2026-07-05 — **Readequação do Roadmap com ADR-0037 (SMP+GPU)**
**Mudança:** SMP+GPU incorporado como bloco prioritário. Sprints replanejados por dependência técnica.
Itens B-01 (LAN) e JARVIS Persona movidos para pós-infraestrutura SMP.

## Blocos Completos (23 blocos, 83 sprints)

| Bloco | Sprints | v | Status |
|---|---|---|---|
| 1-15. Foundation | 1-57 | 0.1–0.57 | ✅ Kernel, PCI, Rede, Transformer, Self-Heal, Agents |
| **16. HW Real + USB** | **58** | **0.58** | **✅ Boot HW real, xHCI HID, FAT12, ATA, CAD** |
| **17. Bootloader 0.11** | **59** | **0.59** | **✅ Framebuffer UEFI 1280×720, bootloader 0.11** |
| **18. Security** | **74** | **0.74.x** | **✅ TPM TIS, Ed25519 signing, Partition mask 0x1C** |
| **19. Disk Intelligence** | **75** | **0.75.x** | **✅ DiskAgent, NVMe, SMART, ARC cache, GPT** |
| **20. Memory + Tick** | **76** | **0.76.x** | **✅ Adaptive heap, Dynamic tick, Event-driven Hermes** |
| **21. Foundation Quick Wins** | **77** | **0.77.x** | **✅ Prompt >, Pre-Flight, FanOut, TaskSchema, /learn, SkillIndex** |
| **22. Agentic Evolution** | **78** | **0.78.x** | **✅ Crew/Flow, Cache, Workflow, GGUF, WASM** |
| **23. LLM Infrastructure** | **79-80** | **0.79.x-0.80.x** | **✅ AVX2 BitNet, Trinity MoE, BPE, QEMU loader, KV Cache** |
| **21a. SMP Foundation** | **81** | **0.81.x** | **✅ SPSC ring, IPI, PerCpu** |
| **21b. Work-Stealing + Matmul** | **82** | **0.82.x** | **✅ Chase-Lev, parallel-for, AgentScheduler multicore** |
| **21e. Polimento** | **83** | **0.83.x** | **✅ burn-flex, CFS, GPU+Display co-existência** |

## Próximos Blocos (Sprints 84-90, reestruturados com ADR-0037)

### Nota de HW Real (Jul 2026)

Após pesquisa expandida (ADR-0037 v4), confirmado para **nosso i5-6400 + RTX 1050 (GP107)**:
- **CPU**: AVX2 + FMA é o teto (sem AMX, AVX-512, APX, NPU)
- **GPU**: firmware NVIDIA Pascal **disponível** em linux-firmware (FECS/GPCCS signed)
- **NPU**: irrelevante (sem HW, firmware fechado)
- **Foco**: SMP 4 cores primeiro (garantido), GPU segundo (viável, complexo)

## 🟡 Bloco 21c — GPU Foundations — Sprint 84
**RTX 1050 (GP107 Pascal) como device de compute. Firmware disponível em linux-firmware desde 2017.**

| Item | Origem | O que | LOC | Dependência |
|---|---|---|---|---|
| GPU BAR0/BAR1 mapping UC | nova-core + pascal-egpu | Mapear BARs como uncacheable para MMIO | 300 | NVMe (✅) |
| ACR secure boot | pascal-egpu + nouveau | Carregar firmware signed FECS/GPCCS (disponível) | 600 | BAR0 mapping |
| PCIe doorbell register | nova-core + pascal-egpu | Setup de doorbell para submissão de jobs | 100 | BAR0 mapping |
| GPU SPSC job ring | monadic-hypervisor + dmaplane | CPU enfileira, GPU consome | 300 | Doorbell |
| VRAM buddy allocator | coconutOS + nova-core | Gerenciar 4GB GDDR5 | 400 | BAR1 mapping |
| | **Total** | | **~1700** | |

**Risco:** ACR secure boot requer WPR setup + signature patching. Seguir nouveau + pascal-egpu. Reclocking NÃO é necessário para compute funcional (roda em clock padrão).

**Status:** 🟡 Agendado

## 🟡 Bloco 21d — GPU Decode (BitNet offload) — Sprint 85
**Decode do BitNet roda na GPU. Prefill fica na CPU.**

| Item | Origem | O que | LOC | Dependência |
|---|---|---|---|---|
| Agent.xpu prefill/decode split | Agent.xpu (arXiv 2506.24045) | CPU faz prefill, GPU faz decode | 400 | GPU job ring |
| GPU matmul kernel (ternary) | nova-core patterns | Matmul BitNet na GPU via shader | 300 | GPU ring |
| CPU→GPU KV cache DMA | dmaplane | Transferir KV cache por DMA | 200 | GPU DMA |
| XQueue (preemptível) | XSched (OSDI) | Fila de comandos GPU com preempção | 600 | GPU ring |
| | **Total** | | **~1500** | |

**Status:** 🟡 Agendado

### 🟡 Bloco 30 — JARVIS Persona + Cognitive — Sprint 86
**Reordenado para depois da infraestrutura SMP. JARVIS ganha com paralelismo.**

| Item | O que | LOC | Depende de |
|---|---|---|---|
| SOUL.md Personality Engine | ~300 | SMP base |
| IPW Monitor (RAPL MSR 0x610) | ~150 | PerCpu |
| Session Compression | ~200 | Nenhuma |
| Notification Gate | ~200 | Nenhuma |
| Sessionless Thread | ~100 | Nenhuma |
| Emotion Analysis (BitNet 7 emoções) | ~250 | BitNet (✅) |
| Capability Contracts + Consent Gates | ~200 | Nenhuma |
| Skill Discovery (DSPy/ACE) | ~300 | SkillIndex (✅) |
| ADE Pipeline | ~200 | Nenhuma |
| Semantic Cache (5-tier) | ~150 | Nenhuma |
| Dreaming/Consolidation | ~200 | CronAgent (✅) |
| Ego Layer | ~250 | BitNet (✅) |
| Proactive Heartbeats | ~100 | Nenhuma |
| Tool-State Save Game | ~100 | Nenhuma |
| Auto-Skill Generation | ~150 | Nenhuma |
| Babel-Index | ~100 | Nenhuma |
| SleepCycle Agent | ~780 | CronAgent (✅) |
| | **Total** | **~3280** | |

**Status:** 🟡 Agendado

### 🟡 Bloco 31 — JARVIS Security + AHCI — Sprint 87

| Item | O que | LOC |
|---|---|---|
| Fail-Closed Safety Invariant | ~200 |
| Merkle Audit Trail | ~200 |
| Fluid Persona | ~100 |
| AHCI driver (SATA 6G NCQ) | ~700 |
| | **Total** | **~1200 LOC** |

**Status:** 🟡 Agendado

### 🔴 Bloco 32+ — AIOS Evolution — Sprint 88+
**Tudo que depende de rede (LAN) — B-01 é o gatekeeper**

| Item | LOC | Bloqueador |
|---|---|---|
| B-01 RX fix (RTL8139 DHCP/RX) | ~500 | 🔴 QEMU SLiRP |
| WWW Agents | ~2600 | 🔴 B-01 |
| Self-Update Agent | ~800 | 🔴 B-01 |
| Plugin Hub + Marketplace | ~400 | 🔴 B-01 |
| Voice Pipeline | ~1600 | 🔴 B-01 |
| Multi-device sync | ~300 | 🔴 B-01 |
| SKYNET Mesh | ~300 | 🔴 B-01 |
| WiFi | ~1000 | 🔴 B-01 |
| | **Total** | **~7500 LOC** |

**Status:** 🔴 Bloqueado (B-01)

## Funcionalidades por Camada

### ✅ Kernel Base
- `no_std` Rust, `x86_64-unknown-none`, nightly
- Framebuffer UEFI 1280×720
- IDT 0-31, PIC/APIC dual EOI
- Bitmap Frame Allocator (dynamic sizing)
- **Adaptive Heap** (16 MB → resize para modelo AI, via frame allocator)
- FPU/SSE, Tensor f32, matmul
- BitNet 1.58-bit (ADD/SUB kernel)
- Transformer 4 layers, Attention, 272K params

### ✅ Storage
- **DiskIntelligenceAgent** (6 controladoras, 10+ FS probes)
- NVMe driver (Admin queue + Identify + I/O Read)
- USB-MSC bulk fix (xHCI IOC+ring+ERDP + BOT protocol)
- S.M.A.R.T. monitoring (ATA 0xB0+0xD0, health alerts)
- GPT partition table, SED/OPAL detection
- ARC cache 1MB DRAM + tier migration MHI
- FAT32-only (Fat12Writer removido, 102 LOC eliminated)

### ✅ Security
- TPM 2.0 TIS driver (SHA256 embedded, PCR[8] extend, fallback)
- Ed25519 kernel signing + auto-verification
- Partition mask 0x1C (Hidden FAT32 LBA, bootloader-compatible)
- Shutdown tracking (4 causas, FAT32 persistence)

### ✅ Agent Runtime
- **Dynamic tick** (12-192 ticks/s, calibrado por workload)
- **Hermes event-driven** (silêncio sem trabalho real)
- **AgentTier classification** (Permanent/SystemDemand/UserDemand/Periodic/Learning)
- **Activation on Demand** — só agentes essenciais usam Continuous
- EventDriven scheduler fix (has_event=true, has_pending early-return)
- MemoryAgent com clock calibration via rdtsc

### ✅ Foundation Quick Wins (Sprint 77) ✅
- Prompt `>` interativo — Hermes aguarda input do usuário ✅
- Pre-Flight Principle — Skill::verify() valida antes de executar ✅
- Background Fan-out — delegação automática para sub-agentes ✅
- TaskSchema + JobPreconditions — schema de tarefas estruturadas ✅
- SkillIndex + MCP Catalog — índice pesquisável de skills ✅
- Completion Contracts — verificação pós-execução de skills ✅
- `/learn` command — geração automática de SKILL.md ✅

### ✅ Agentic Evolution (Sprint 78) ✅
- Crew + FlowTrigger — orquestração multi-agente ✅
- IntentCache + OutputCache — cache inteligente ✅
- WorkflowEngine + SelfCritique — engine de workflow ✅
- StateGraph Scheduler — agendamento por grafo de estado ✅
- migrate_to_tier() — page table manipulation MHI ✅
- MHI+FS Bridge — suggest_tier_for_path integrado ao VFS ✅
- GGUF Loader — modelos 1B+ params ✅
- WASM Runtime — wasmi + WASI→Skill bridge ✅

### ✅ LLM Infrastructure (Sprint 79-80) ✅
- AVX2 BitNet Kernel — SIMD intrinsics ✅
- BPE Tokenizer (HuggingFace JSON parser) ✅
- KV Cache (v0.80.1) — geração autoregressiva ✅
- Trinity Router (MoE) — stub funcional ✅
- QEMU loader pipeline BitNet-b1.58 850M ✅

### 🟢 SMP Foundation — SPSC + IPI + PerCpu (Bloco 25)
- SPSC ring lockless (bbqueue) para comunicação cross-core, IRQ→task, GPU→CPU
- IPI vetorizado para acordar APs sob demanda + TLB shootdown
- PerCpu dinâmico — alocar struct por AP + GS.base individual
- `#[repr(align(64))]` padronizado para false sharing prevention

### 🟢 Parallel — Work-Stealing + Matmul (Bloco 26)
- Work-stealing Chase-Lev entre 4 cores (deques lock-free)
- Parallel-for no matmul AVX2 (chunk hidden dim)
- AgentScheduler multicore (4 run queues + steal)
- Per-CPU slab allocator (alocação local sem lock)

### 🟢 GPU Compute Foundations (Bloco 27)
- GPU BAR0/BAR1 mapping UC (MMIO para RTX 1050)
- PCIe doorbell register setup
- GPU SPSC job ring (CPU enfileira, GPU consome)
- VRAM buddy allocator (4GB)

### 🟢 GPU Decode (Bloco 28)
- Agent.xpu prefill/decode split (CPU prefill, GPU decode)
- GPU matmul kernel ternário (BitNet na GPU)
- CPU→GPU KV cache DMA
- XQueue preemptível (múltiplos workloads GPU)

### 🟢 Polimento (Bloco 29)
- burn-flex backend port (gemm profissional, elimina bitnet_avx2 manual)
- MSched evicção ótima de VRAM (Belady)
- CFS scheduler para agents (fairness)
- GPU + Display co-existência

### 🟡 JARVIS Persona + Cognitive (Bloco 30, pós-SMP)
- SOUL.md, IPW Monitor, Session Compression, Notification Gate
- Emotion Analysis, Capability Contracts, Skill Discovery
- Semantic Cache, Dreaming/Consolidation, Ego Layer
- SleepCycle Agent (780 LOC)

### 🔴 AIOS Evolution (Bloco 32+, pós B-01)
- B-01 RX fix, WWW Agents, Self-Update, Voice Pipeline, WiFi

### ✅ Input
- PS/2 keyboard (IRQ1, scancode set 1)
- **xHCI USB HID keyboard** (Boot Protocol, 68 teclas)
- Ctrl+Alt+Del (PS/2 + USB) com shutdown+FAT12 dump

### ✅ Display
- VGA text mode buffer (0xB8000)
- **UEFI framebuffer** (preparado, aguarda bootloader 0.11+)
- VirtIO-GPU (QEMU)
- Console multi-região, fonte VGA 8×16

### ✅ Agentes (20 agentes)
| Código | Agente | Tipo | Função |
|---|---|---|---|
| A-001 | SystemAgent | System | Init, EchoSkill |
| A-002 | MonitorAgent | System | SYSTEM_READY |
| A-003 | HwBridgeAgent | Router | IRQ bridge |
| A-004 | NetAgent | Network | smoltcp poll |
| A-005 | InputAgent | Console | Keyboard (PS/2 + USB) |
| A-006 | CortexAgent | Inference | LLM transformer + Medusa |
| A-007 | HermesAgent | Router | Intent routing, ReAct, Council |
| A-008 | DisplayAgent | Console | Framebuffer + VGA |
| A-009 | NetDriverAgent | Driver | RTL8139 + VirtIO-net |
| A-010 | UsbDriverAgent | Driver | xHCI init |
| A-011 | BootSelfHealAgent | System | SelfHeal init |
| A-012 | BootTrustAgent | System | TrustCache init |
| A-013 | PlatformAgent | System | PCI+ACPI+APIC+SMP |
| A-014 | MemoryAgent | System | MHI + Arch |
| A-015 | GpuDriverAgent | Driver | VirtIO-GPU |
| A-016 | HwDetectAgent | System | HwIdentifySkill |
| A-017 | CronAgent | System | Cron Scheduler |
| A-018 | SecurityAgent | System | Security Pipeline |
| A-019 | SafetyAgent | System | Asimov 4 Laws |
| A-020 | OptimizerAgent | System | Self-Optimization |

### ✅ Trust & Security
- TrustCache (allow/deny/TTL/denylist)
- Ed25519 via `ed25519-compact`
- CapabilityToken enum (Legacy + Ed25519)
- 5 detectores (PortScan, ArpSpoof, etc)
- Path Confinement, Mask Secrets
- Graduated Enforcement (Observe→Warn→Contain→Enforce)
- Safety Interceptor (Asimov 4 Laws)

### ✅ Self-Healing
- FailureClass taxonomy (Memory/Execution/Resource/Logic/External)
- SelfHeal analyze + RecoveryAction
- Exception handlers (Page Fault, Double Fault, GPF)
- RESPAWN_QUEUE + corrective prompting
- CDC Rabin chunking + XOR Delta snapshot

### ✅ Hermes Cognitive
- DA Identity Layer (nome/versão/lema)
- Runtime SDD (goal/context/plan/rollback)
- ReAct 7 fases (OBSERVE→THINK→PLAN→BUILD→EXECUTE→VERIFY→LEARN)
- Council skill (3 vozes)
- Intent Transparency, Context Fencing
- Bitter Pill Engineering
- Usage Pattern Analyzer, Workflow Predictor
- Dynamic Resource Scaling, Reflex Threshold
- Self-Optimizing Scheduler

### ✅ Storage (novo em v0.58)
- **ATA PIO driver** (read/write via PCI class 0x01)
- **MBR parser** (tabela de 4 partições)
- **FAT12 filesystem** (BPB, root dir, append file)
- **patch_image.py** (cria partição FAT12 na imagem)

### ✅ Boot Hardware Real
- **primeiro boot em notebook físico** via SDHC USB
- VGA text mode funcional
- Hermes Cognitive rodando (ReAct)
- USB keyboard via xHCI
- Ctrl+Alt+Del com dump FAT12 + shutdown
- BOOT.LOG visível no Windows Explorer

## Pendências Técnicas (Atualizado ADR-0037)

| Item | Esforço | Bloco | Prioridade |
|---|---|---|---|
| SPSC ring lockless | ~100 LOC | 25 (SMP Foundation) | 🔴 Crítica — base cross-core |
| IPI vetorizado | ~150 LOC | 25 (SMP Foundation) | 🔴 Crítica — acordar APs |
| PerCpu dinâmico | ~300 LOC | 25 (SMP Foundation) | 🔴 Crítica — dados por core |
| Work-stealing scheduler | ~400 LOC | 26 (Parallel) | 🟡 Alta — distribuir carga |
| Parallel-for AVX2 matmul | ~300 LOC | 26 (Parallel) | 🟡 Alta — 2-3× speedup |
| AgentScheduler multicore | ~200 LOC | 26 (Parallel) | 🟡 Alta — 4 run queues |
| Per-CPU slab allocator | ~300 LOC | 26 (Parallel) | 🟡 Média — alocação local |
| GPU BAR mapping | ~300 LOC | 27 (GPU Found.) | 🟡 Média — RTX 1050 compute |
| GPU doorbell + job ring | ~400 LOC | 27 (GPU Found.) | 🟡 Média — submissão GPU |
| VRAM allocator (buddy) | ~400 LOC | 27 (GPU Found.) | 🟡 Média — 4GB VRAM |
| Agent.xpu prefill/decode split | ~400 LOC | 28 (GPU Decode) | 🟢 Baixa — após GPU pronta |
| GPU matmul ternário | ~300 LOC | 28 (GPU Decode) | 🟢 Baixa — BitNet offload |
| KV cache DMA CPU↔GPU | ~200 LOC | 28 (GPU Decode) | 🟢 Baixa — zero copy |
| XQueue preemptível | ~600 LOC | 28 (GPU Decode) | 🟢 Baixa — scheduling GPU |
| burn-flex backend | ~800 LOC | 29 (Polimento) | 🟢 Baixa — gemm profissional |
| MSched evicção VRAM | ~500 LOC | 29 (Polimento) | 🟢 Baixa — memória GPU |
| CFS scheduler | ~500 LOC | 29 (Polimento) | 🟢 Baixa — fairness |
| JARVIS Persona (17 itens) | ~3280 LOC | 30 (JARVIS) | 🟢 Baixa — pós-SMP |
| AHCI driver | ~700 LOC | 31 (Security) | 🟢 Baixa |
| B-01 RX fix | ~500 LOC | 32+ (AIOS) | 🔴 Bloqueado (QEMU) |

## Activation on Demand — Filosofia

Todo agente importado (The Agency 147, HW Agents, FS Agents) deve declarar `on_demand: true`
no manifesto, usando `EventDriven` ou `UserDemand`. O AgentScheduler só polla agentes
quando há evento pendente. Agentes `Continuous` são exclusivos dos essenciais: Hermes,
Display e HwBridge. Se um `Continuous` não-essencial consumir >5% dos ticks sem produzir
eventos por 1000 ticks, o SafetyAgent o rebaixa para `EventDriven`.
