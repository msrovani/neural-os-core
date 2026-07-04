# Roadmap — neural-os-core v0.77.0-design 🏆

**Última atualização:** 2026-07-04 — **Readequação do Roadmap**
**Mudança:** Reorganização evolutiva por dependências. Sprint 77 = Foundation Quick Wins.
Itens B-01 (LAN) empurrados para Sprint 85+.

## Blocos Completos (20 blocos, 76 sprints)

| Bloco | Sprints | v | Status |
|---|---|---|---|
| 1-15. Foundation | 1-57 | 0.1–0.57 | ✅ Kernel, PCI, Rede, Transformer, Self-Heal, Agents |
| **16. HW Real + USB** | **58** | **0.58** | **✅ Boot HW real, xHCI HID, FAT12, ATA, CAD** |
| **17. Bootloader 0.11** | **59** | **0.59** | **✅ Framebuffer UEFI 1280×720, bootloader 0.11** |
| **18. Security** | **74** | **0.74.x** | **✅ TPM TIS, Ed25519 signing, Partition mask 0x1C** |
| **19. Disk Intelligence** | **75** | **0.75.x** | **✅ DiskAgent, NVMe, SMART, ARC cache, GPT** |
| **20. Memory + Tick** | **76** | **0.76.x** | **✅ Adaptive heap, Dynamic tick, Event-driven Hermes** |

## Próximos Blocos (Sprints 77-86, reestruturados)

### 🟡 Bloco 21 — Foundation Quick Wins (Sprint 77)
**Itens independentes dos sprints 60/67/72 — sem dependências entre si**

| Item | Origem | O que | LOC |
|---|---|---|---|
| 60.1b | Sprint 60 | Prompt `>` interativo (Hermes aguarda input) | ~30 |
| 67.0.3 | Sprint 67 | Pre-Flight Principle (Skill::verify pré-execução) | ~80 |
| 67.2.3 | Sprint 67 | Background Fan-out (delegação automática) | ~80 |
| 72.2 | Sprint 72 | TaskSchema + JobPreconditions (schema de tarefas) | ~200 |
| 72.6 | Sprint 72 | SkillIndex + MCP Catalog (índice + catálogo) | ~150 |
| 67.2.2 | Sprint 67 | Completion Contracts (verificação pós-skill) | ~100 |
| 67.2.1 | Sprint 67 | `/learn` command (SKILL.md generator) | ~120 |
| | | **Total bloco** | **~760 LOC** |

### 🟡 Bloco 22 — Agentic Evolution + Memory Systems (Sprint 78)
**Completa Agentic Evolution (72) + Memory bridge + Loaders**

| Item | Origem | O que | LOC |
|---|---|---|---|
| 72.1 | Sprint 72 | Crew + FlowTrigger (orquestração multi-agente) | ~300 |
| 72.3 | Sprint 72 | IntentCache + OutputCache (cache de intenção/saída) | ~200 |
| 72.4 | Sprint 72 | WorkflowEngine + SelfCritique (engine de workflow) | ~250 |
| 72.5 | Sprint 72 | StateGraph Scheduler (agendamento por grafo) | ~200 |
| 60.8c | Sprint 60 | migrate_to_tier() (page table manipulation MHI) | ~170 |
| 62.2 | Sprint 62 | MHI+FS Bridge (suggest_tier_for_path) | ~300 |
| — | Sprint 77 old | GGUF Loader (modelos 1B+ params) | ~500 |
| — | Sprint 77 old | WASM Runtime (wasmi + WASI→Skill bridge) | ~800 |
| | | **Total bloco** | **~2720 LOC** |

### 🟡 Bloco 23 — LLM Infrastructure + MoE (Sprint 79)
**Model loading, router MoE, training infrastructure**

| Item | O que | LOC |
|---|---|---|
| AVX2 BitNet Kernel (intrinsics SIMD) | ~150 |
| Trinity Router (MoE — classifica intenção, roteia expert) | ~500 |
| Candle sidecar (training em Rust puro com GPU) | ~300 |
| TrainingAgent (fine-tune/transfer/full on-device) | ~500 |
| | **Total bloco** | **~1450 LOC** |

### 🟡 Bloco 24 — JARVIS Core Persona (Sprint 80)
**SOUL.md + IPW + Compression + Notifications + Sessionless**

| Item | O que | LOC |
|---|---|---|
| #315.1 SOUL.md Personality Engine | ~300 |
| #315.2 IPW Monitor (RAPL MSR 0x610) | ~150 |
| #315.3 Session Compression (4 strategies) | ~200 |
| #315.4 Notification Gate (4 urgency levels) | ~200 |
| #315.5 Sessionless Thread | ~100 |
| | **Total bloco** | **~950 LOC** |

### 🟡 Bloco 25 — JARVIS Emotion + Cache + Pipeline (Sprint 81)

| Item | O que | LOC |
|---|---|---|
| #315.6 Emotion Analysis (BitNet classifier 7 emoções) | ~250 |
| #315.7 Capability Contracts + Consent Gates (Safe/Moderate/Dangerous) | ~200 |
| #315.8 Skill Discovery (DSPy/ACE pipeline) | ~300 |
| #315.9 ADE Pipeline (Spec→Execute→Review→Recover) | ~200 |
| #315.10 Semantic Cache (5-tier routing, 97.5% reduction) | ~150 |
| #315.11 Persona Pipeline (16 stages OVOS-inspired) | ~100 |
| | **Total bloco** | **~1200 LOC** |

### 🟡 Bloco 26 — JARVIS Cognitive Memory (Sprint 82)

| Item | O que | LOC |
|---|---|---|
| #315.12 Dreaming/Consolidation (CronAgent noturno) | ~200 |
| #315.13 Ego Layer (self-model, confidence tracking) | ~250 |
| #315.14 Proactive Heartbeats (JARVIS inicia conversa) | ~100 |
| #315.15 Tool-State Save Game (snapshot + rollback) | ~100 |
| #315.16 Auto-Skill Generation (watch→pattern→propose→generate) | ~150 |
| #315.17 Babel-Index (entropy + contradiction + staleness) | ~100 |
| SleepCycle Agent (5 fases: REPLAY→DREAM→CONSOLIDATE→PRUNE→REFLECT) | ~780 |
| | **Total bloco** | **~1680 LOC** |

### 🟡 Bloco 27 — JARVIS Security + AHCI (Sprint 83)

| Item | O que | LOC |
|---|---|---|
| #315.18 Fail-Closed Safety Invariant (SMT-proof, 4 invariants) | ~200 |
| #315.19 Merkle Audit Trail (Ed25519 signed, ring buffer 4096) | ~200 |
| #315.20 Fluid Persona (context-adaptive, coach/tutor/tool) | ~100 |
| AHCI driver (SATA 6G NCQ) | ~700 |
| | **Total bloco** | **~1200 LOC** |

### 🟡 Bloco 28 — GPU Compute (Sprint 84)

| Item | O que | LOC |
|---|---|---|
| Intel GEN shader (matmul via EU execution units) | ~800 |
| | **Total bloco** | **~800 LOC** |

### 🔴 Bloco 29+ — AIOS Evolution (Sprint 85+, pós B-01)
**Tudo que depende de rede (LAN) — B-01 é o gatekeeper**

| Item | LOC | Bloqueador |
|---|---|---|
| B-01 RX fix (RTL8139 DHCP/RX) | ~500 | 🔴 QEMU SLiRP |
| WWW Agents (Browser, Email, RSS, Search, Download, WS) | ~2600 | 🔴 B-01 |
| Self-Update Agent (A/B slots + rollback) | ~800 | 🔴 B-01 |
| Plugin Hub + Marketplace | ~400 | 🔴 B-01 |
| Voice Pipeline (Piper TTS + Vosk STT + Wake Word + Wyoming) | ~1600 | 🔴 B-01 |
| Multi-device sync (CRDT) | ~300 | 🔴 B-01 |
| SKYNET Mesh Node | ~300 | 🔴 B-01 |
| WiFi (Intel/Atheros/Realtek 802.11) | ~1000 | 🔴 B-01 |
| | **Total** | **~7500 LOC** |

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

### 🟡 Foundation Quick Wins (Sprint 77)
- Prompt `>` interativo — Hermes aguarda input do usuário
- Pre-Flight Principle — Skill::verify() valida antes de executar
- Background Fan-out — delegação automática para sub-agentes
- TaskSchema + JobPreconditions — schema de tarefas estruturadas
- SkillIndex + MCP Catalog — índice pesquisável de skills
- Completion Contracts — verificação pós-execução de skills
- `/learn` command — geração automática de SKILL.md

### 🟡 Agentic Evolution (Sprint 78)
- Crew + FlowTrigger — orquestração multi-agente
- IntentCache + OutputCache — cache inteligente
- WorkflowEngine + SelfCritique — engine de workflow
- StateGraph Scheduler — agendamento por grafo de estado
- migrate_to_tier() — page table manipulation MHI
- MHI+FS Bridge — suggest_tier_for_path integrado ao VFS
- GGUF Loader — modelos 1B+ params
- WASM Runtime — wasmi + WASI→Skill bridge

### 🟡 LLM Infrastructure (Sprint 79)
- AVX2 BitNet Kernel — SIMD intrinsics
- Trinity Router (MoE) — classifica intenção, roteia expert
- Candle sidecar — training em Rust puro com GPU
- TrainingAgent — fine-tuning/transfer/full on-device

### 🟡 JARVIS Persona (Sprint 80)
- SOUL.md Personality Engine — persona JARVIS
- IPW Monitor — Intelligence Per Watt via RAPL MSR
- Session Compression — 4 estratégias
- Notification Gate — 4 níveis de urgência
- Sessionless Thread — conversa contínua sem reset

### 🟡 JARVIS Emotion + Cache (Sprint 81)
- Emotion Analysis — BitNet classifier 7 emoções
- Capability Contracts + Consent Gates — 3 níveis de risco
- Skill Discovery — DSPy/ACE pipeline
- ADE Pipeline — Spec→Execute→Review→Recover
- Semantic Cache — 5-tier routing (97.5% reduction)
- Persona Pipeline — 16 stages OVOS-inspired

### 🟡 JARVIS Cognitive Memory (Sprint 82)
- Dreaming/Consolidation — memória sintética noturna
- Ego Layer — self-model, confidence tracking
- Proactive Heartbeats — JARVIS inicia conversa
- Tool-State Save Game — snapshot + rollback
- Auto-Skill Generation — watch→pattern→propose→generate
- Babel-Index — entropia + contradição + staleness
- SleepCycle Agent — 5 fases de aprendizado onírico

### 🟡 JARVIS Security + AHCI (Sprint 83)
- Fail-Closed Safety Invariant — SMT-proof, 4 invariants
- Merkle Audit Trail — Ed25519 signed chain
- Fluid Persona — context-adaptive coach/tutor/tool
- AHCI driver — SATA 6G com NCQ

### 🟡 GPU Compute (Sprint 84)
- Intel GEN shader — matmul via EU execution units

### 🔴 AIOS Evolution (Sprint 85+, pós B-01)
- B-01 RX fix — RTL8139 DHCP/RX
- WWW Infrastructure + Agents (Browser, Email, RSS, Search, Download, WS)
- Self-Update Agent (A/B slots + rollback)
- Plugin Hub + Marketplace
- Voice Pipeline (Piper TTS, Vosk STT, Wake Word, Wyoming)
- Multi-device sync (CRDT)
- SKYNET Mesh Node
- WiFi

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

## Pendências Técnicas

| Item | Esforço | Bloco |
|---|---|---|
| Prompt `>` interativo | ~30 LOC | 21 (Sprint 77) |
| WASM sandbox (`wasmi`) | ~800 LOC | 22 (Sprint 78) |
| AHCI driver (SATA 6G NCQ) | ~700 LOC | 27 (Sprint 83) |
| Intel GEN shader | ~800 LOC | 28 (Sprint 84) |
| B-01 RX fix | ~500 LOC | 29+ (Sprint 85+) |
| Modelo 1.5B params (treino) | Python | 29+ |
| Framebuffer UEFI (bootloader 0.11+) | ~500 LOC | Upgrade bootloader |
| VirtIO-GPU GET_DISPLAY_INFO | Debug | QEMU TCG |
| SMP `-smp 2` sem WHPX | Debug | TCG atomicidade |
| Driver e1000/r8169 (rede real) | ~300 LOC | Teste HW |

## Activation on Demand — Filosofia

Todo agente importado (The Agency 147, HW Agents, FS Agents) deve declarar `on_demand: true`
no manifesto, usando `EventDriven` ou `UserDemand`. O AgentScheduler só polla agentes
quando há evento pendente. Agentes `Continuous` são exclusivos dos essenciais: Hermes,
Display e HwBridge. Se um `Continuous` não-essencial consumir >5% dos ticks sem produzir
eventos por 1000 ticks, o SafetyAgent o rebaixa para `EventDriven`.
