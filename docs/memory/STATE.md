# ════════════════════════════════════════════════════════
#   STATE — neural-os-core v0.95.0-cog+v0.96.0-heal 🧠🛡️
#   SPRINTS 95-96 — Cognitive Engine + Self-Healing Avançado
#   165+ arquivos Rust, ~19.000 LOC, 0 erros
# ════════════════════════════════════════════════════════

## Marcos Acumulados
- **v0.56.0-v0.67.0** — 22 sprints de OS neural, GPU, desktop, agentes, ecossistema
- **v0.68.0-v0.70.0** — USB Mass Storage, xHCI bulk, BootLogAgent, FAT32 writer
- **v0.71.0** — Boot Bughunt: Agent-First + DiagnosticSkill + FAT12 log + Xuvisco
- **v0.73.0-0.73.1** — Consciousness (10 métricas), Self-Improvement Loop, Shutdown tracking
- **v0.74.0-0.74.2** — TPM TIS driver, Ed25519 kernel signing, Partition mask 0x1C
- **v0.75.0-0.75.6** — FAT32-only, DiskIntelligenceAgent (680 LOC, 6 controllers, 10+ FS probes)
- **v0.76.0-0.76.1** — NVMe driver, S.M.A.R.T., Adaptive heap, Dynamic tick, Event-driven Hermes
- **v0.80.0-0.80.1** — AVX2 Debug, WHPX Detection, KV Cache (200x+ speedup)
- **v0.84.0-0.84.1** — GPU Foundations (BAR UC, SPSC job ring, VRAM alloc, secure boot)
- **v0.85.0** — GPU Decode (BitNet offload, CPU↔GPU KV cache DMA)
- **v0.86.0** — JARVIS Persona (SoulProfile, EmotionAnalysis, EgoLayer, Heartbeat)
- **v0.87.0** — Security + AHCI (TPM extend, Audit Trail, SATA 6G NCQ)
- **v0.88.0** — Emotion + Cache (EmotionEngine, SleepCycle, NeuralCache)
- **v0.89.0** — JARVIS Deep Cognitive (DreamEngine, BabelIndex, AutoSkillGen)
- **v0.90.0** — Desktop UI (JarvisDesktop compositor, Hermes Chat, Settings, Power)
- **v0.91.0** — LAN + Dependencies (DHCP, ARP cache, smoltcp upgrade)
- **v0.92.0** — WASM Runtime + IDE (MemoryPool, HybridRegistry, BitNet IDE F4)
- **v0.93.0-wasm** — WASM Skill Runtime refinado (+WASI mappings)
- **v0.94.0-0.94.1** — Vision + Display + TTF (TrueType font engine, tensor heatmap viz)
- **2026-07-06** — **v0.95.0-cog+v0.96.0-heal:** Sprint 95 (Cognitive Engine) + Sprint 96 (Self-Healing Avançado). cognitive.rs reescrito com 25+ itens (510+ LOC): IntentPlanner, SuccessEngine, NeuralCache, MatMulFreeLM, FeedbackLoop, TernaryUpdate, ReplayBuffer, WorkflowPredictor, AutoSkillGen, DynamicScaler, SelfOptScheduler, CodebookVQ (com KV Codebook e Finetune), ReActLoop, McpServer, DeltaBranches, WorkspaceIsolation, EpisodicMemory, BitNetTrainer, CandleSidecar, TaskSpawner, SleepCycleGuard. Sprint 96 completo com M1-M29 (350 LOC): ZeroCopySfs, SkillModule, FailureTaxonomy, ExceptionSelfHeal, CorrectivePrompting, Verifier, EventLog, BudgetedRecovery, SilentDetection, MultiLevelFailure, FailurePrediction, NotificationGate. +860 LOC totais. 0 erros.

## Arquitetura Fundamental
**Tudo no Neural OS Hermes é um Agente ou uma Skill.**
247+ agentes: 20 nativos + 147 The Agency + ~80 importados + ~6 HW + ~6 FS.
Bootloader 0.11.15 com `bootloader_api`. Boot sequence agent-centric.

### Activation on Demand
Agentes só congestionam o tick-tock quando necessário.
- Apenas Hermes, Display, HwBridge usam `Continuous`
- Todo agente importado declara `on_demand: true` no manifesto
- AgentScheduler não polla sem evento pendente
- Penalidade: Continuous não-essencial >5% ticks → rebaixado para EventDriven

### DiskIntelligenceAgent (v0.75.x)
StorageController trait com 6 implementações (ATA, USB-MSC, NVMe, stubs AHCI/SCSI/VirtIO).
FilesystemProbe registry com 10+ probes (FAT32, NTFS, EXT4, XFS, ISO9660, exFAT, Btrfs, HFS+, EROFS, ReFS).
VolumeManagerProbe (LVM2, LUKS). GPT partition table. SED/OPAL detection.
S.M.A.R.T. monitoring (ATA READ DATA 0xB0+0xD0, health alerts).
ARC cache 1MB DRAM + tier migration MHI. I/O scheduler (batched writes). Read-ahead (32KB).

### MemoryAgent (v0.76.1)
Adaptive heap: `resize_heap_to_mb()` dinâmico via frame allocator + map_page_uc.
Orçamento calculado do modelo AI: `heap = clamp(128, params/10MB, 2048)`, `kv = params/40`.
CPU measurement via rdtsc. Dynamic tick calibration via LAPIC init_count.

### Security Stack
TPM 2.0 TIS driver (SHA256 embedded, PCR[8] extend, fallback silencioso).
Ed25519 kernel signing + auto-verification. Partition mask 0x1C (Hidden FAT32 LBA).

### Tick System (v0.76.1)
LAPIC timer com init_count dinâmico: 12-192 ticks/s baseado em agentes ativos.
Hermes event-driven: ReAct cycle só avança com entrada real (silêncio sem trabalho).
EventDriven scheduler fix: `has_event=true` + `has_pending()` early-return pattern.

### Agent Tier Classification
| Tier | Schedule | Exemplos |
|---|---|---|
| Permanent | Continuous | Hermes, Display, HwBridge |
| SystemDemand | EventDriven | DiskAgent, Cortex, Net |
| UserDemand | EventDriven | Skills, Apps, Plugins |
| Periodic | PollEvery(N) | Cron, Observer, Optimizer |
| Learning | PollEvery(2000) | Novos agentes → analisados 5000 ticks → promovidos |

## Roadmap 84-95 (Plano completo em docs/sprint-plan-84-95.md)

| Sprint | v | Bloco | Foco | LOC | Status |
|---|---|---|---|---|---|
| **77** | 0.77.x | **21** | Foundation Quick Wins | ~760 | ✅ |
| **78** | 0.78.x | **22** | Agentic Evolution | ~2720 | ✅ |
| **79** | 0.79.x | **23** | LLM Infrastructure | ~1450 | ✅ |
| **80** | 0.80.x | **24** | AVX2 Debug + KV Cache | ~550 | ✅ |
| **81** | 0.81.x | **21a** | SMP Foundation | ~860 | ✅ |
| **82** | 0.82.x | **21b** | Work-Stealing + Matmul | ~1200 | ✅ |
| **83** | 0.83.x | **21e** | Polimento | ~1680 | ✅ |
| **84** | 0.84.x | **21c** | GPU Foundations | ~1700 | 🟡 |
| **85** | 0.85.x | **21d** | GPU Decode | ~1500 | 🟡 |
| **86** | 0.86.x | **30** | JARVIS Persona | ~950 | 🟡 |
| **87** | 0.87.x | **31** | JARVIS Security + AHCI | ~1200 | 🟡 |
| **88** | 0.88.x | **32** | JARVIS Emotion + Cache | ~1200 | 🟡 |
| **89** | 0.89.x | **33** | SleepCycle + Advanced Memory | ~2500 | 🟡 |
| **90** | 0.90.x | **34** | JARVIS Deep Cognitive | ~1200 | 🟡 |
| **91** | 0.91.x | **35** | Polimento + Ecosystem | ~2500 | 🟡 |
| **92+** | 0.92.x+ | **36+** | AIOS Evolution | ~15000 | 🔴 |
| **95** | 0.95.x | **40** | Cognitive Engine | ~510 | ✅ |
| **96** | 0.96.x | **41** | Self-Healing | ~350 | ✅ |

**Total restante:** ~28.250 LOC (sprints 84-91) + ~15.000 LOC (sprint 92+, bloqueado B-01). Sprints 95-96 concluídos: ~860 LOC.

**Ver também:** `docs/sprint-plan-84-95.md` para detalhes de cada sprint com items do IDEA_BANK.

## Aprendizados Chave
1. **Roadmap readequado 2026-07-04:** Reorganização completa por dependências. Itens independentes primeiro (Foundation → Agentic → LLM → JARVIS → GPU). B-01 e dependentes no final.
2. **Activation on Demand:** Só Hermes/Display/HwBridge usam Continuous. O resto dorme até ter trabalho.
3. **VGA CRTC + UEFI GOP = incompatível** (Sprint 71)
4. **Cortex acorda antes do HW** — LLM deve participar das decisões de hardware
5. **FAT12 removido** — FAT32-only, 102 LOC eliminados
6. **Partition mask 0x1C** — mbr_nostd aceita Hidden FAT32, bootloader OK, SO não monta
7. **TPM fallback** — silencioso se ausente (0xFFFF FFFF), Ed25519 como enforcement primário
8. **RX=0 persistente** — QEMU slirp + VirtualBox bridge, pre-existente (B-01)
9. **Hermes event-driven** — 84 linhas/seg → 0 quando ocioso
10. **Tick dinâmico** — calibrado por workload (12-192 t/s)
11. **Sprint 77** — 7 Foundation Quick Wins: Prompt `>`, Pre-Flight, FanOut, TaskSchema, SkillIndex, CompletionContracts, DynamicSkill. ~380 LOC, 0 erros.
12. **Sprint 78** — 8 Agentic Evolution items: IntentCache wiring, OutputCache wiring, WorkflowEngine wiring, SelfCritique, GgufBackedModel, AgentTier+migrate_to_tier, FsBridgeAgent, WasmExecutor+WasmSkill. ~400 LOC, 0 erros.
12. **VirtualBox SMP fix** — AP_COUNT static from MADT lapic_count. 2 vCPUs now boot reliably on VB.
13. **Sprint 79** — LLM Infrastructure: BitNet-b1.58 850M downloaded + .bitnet v2 conversion (1.5GB). AVX2 ternary matmul kernel. BPE tokenizer. Trinity MoE stub. QEMU loader boot pipeline at phys 4GB. Ramdisk via bootloader impossível (FAT limit). Forward pass blocked by GQA + BitFFN grouped projections.
14. **BitNet b1.58 real arch** — Microsoft's model is 850M params (not 2B). GQA (20 heads Q, 5 KV heads). BitFFN with grouped down_proj (640→6912). `tie_word_embeddings=true`. vocab_size=128256 (requires u32).
15. **QEMU loader strategy** — `-device loader,file=.bitnet,addr=0x100000000` com `-m 6G` + WHPX. Model in high memory avoids frame allocator conflicts. ~30s boot overhead acceptable for dev.
16. **Build_image.py UEFI issue** — bootloader 0.11.15 default features include UEFI. `default-features=false, features=["bios"]` avoids serde compile panic.
17. **VGA buffer clear fix (v0.79.1):** `[BOOT] FB ativo — VGA text mode desligado` agora é verdade. 0xB8000 limpo via `write_bytes` sem CRTC I/O. Framebuffer limpo para preto imediatamente no probe.
18. **VGA sequencer fix (v0.79.2):** `clear_physical_buffer()` write a 0xB8000 causa page fault pre-IDT. UEFI/OVMF não mapeia legacy VGA hole. Solução: VGA sequencer I/O (0x3C4/0x3C5) Screen Off bit — zero acesso a memória desmapeada.
19. **WHPX emula AVX2/VEX lentamente (v0.80.0):** CPUID mostra AVX2=disponível, mas cada instrução VEX causa VM exit (~10k+ ciclos). Scalar GP instructions rodam nativos. `has_avx2()` deve detectar WHPX via CPUID 0x40000000 e retornar false. AVX2 sob WHPX = 4443 ticks/layer vs scalar = 2218 ticks/layer (~2.2s/layer, ~60s/forward pass).
20. **`unpack_all()` não é o gargalo (v0.80.0):** Substituir alocação de 17.7 MB por row buffer de 6.9 KB não acelerou o forward pass — o gargalo real é a emulação VEX + WHPX memory virtualization. Operações aritméticas dominam, não alocação.
21. **Forward pass BitNet b1.58 sob WHPX:** ~60s para 64 tokens × 30 layers. Generate_speculative de 8 tokens levaria ~6h. Inviável sem KV cache ou bare metal.

## Pendente Técnico (atualizado v0.84 — ver sprint-plan-84-95.md para detalhes)

### 🔴 Bloqueado (exige busca na internet)
- **B-01 DHCP/DNS/HTTP**: RX fix RTL8139 — smoltcp DHCP nunca completa. 🔴 **Buscar na internet** soluções, patches, HW real

### 🟡 Sprint 84 — GPU Foundations
- `#326` GPU BAR0/BAR1 mapping UC (NVIDIA/AMD/Intel)
- `#352` Secure Boot GPU (ACR/PSP/GuC)
- `#327` GPU doorbell + SPSC job ring
- `#328` VRAM buddy allocator
- `#353` GPU Compute Pipeline

### 🟡 Sprint 85 — GPU Decode
- `#329` Agent.xpu prefill/decode split
- `#330` GPU matmul kernel ternário
- `#331` CPU→GPU KV cache DMA
- `#332` XQueue preemptível

### 🟡 Sprint 86 — JARVIS Persona
- `#315.1-5` SOUL.md, IPW, Session Compression, Notification Gate, Sessionless Thread

### 🟡 Sprint 87 — JARVIS Security + AHCI
- `#315.18-20` Fail-Closed, Merkle, Fluid Persona
- AHCI driver (SATA 6G NCQ)

### 🟡 Sprint 88 — JARVIS Emotion + Cache
- `#315.6-11` Emotion Analysis, Capability Contracts, Skill Discovery, ADE, Semantic Cache, Persona Pipeline

### 🟡 Sprint 89 — SleepCycle + Memory
- `#314` SleepCycle Agent (5 fases: REPLAY→DREAM→CONSOLIDATE→PRUNE→REFLECT)
- `#214-225` Memory Systems (SHA-256 dedup, Ebbinghaus, Atkinson-Shiffrin, KG)

### 🟡 Sprint 90 — JARVIS Deep Cognitive
- `#315.12-17` Dreaming, Ego, Heartbeats, Tool-State, Auto-Skills, Babel-Index

### 🟡 Sprint 91 — Polimento + Ecosystem
- `#333` burn-flex backend, `#334` MSched VRAM, `#335` CFS, `#336` GPU+Display
- `#279a-c` SmileyOS patterns (shell, temas, FS)
- `#283a-b` Desktop Cube

### 🔴 Sprint 92+ — AIOS Evolution
- Bloqueado por B-01. 25+ items (~15000 LOC)

### ✅ Sprint 95 — Cognitive Engine (completo)
- `#105-162, #169-175, M2, M37-M41` — IntentPlanner, SuccessEngine, NeuralCache, MatMulFreeLM, FeedbackLoop, TernaryUpdate, ReplayBuffer, WorkflowPredictor, AutoSkillGen, DynamicScaler, SelfOptScheduler, CodebookVQ, KV Codebook, ReActLoop, McpServer, CodebookFinetune, DeltaBranches, WorkspaceIsolation, EpisodicMemory, SleepCycleGuard, BitNetTrainer, CandleSidecar, TaskSpawner, ThreeDataSources. ~510 LOC.

### ✅ Sprint 96 — Self-Healing (completo)
- `#226-227, #265-267, M1-M29` — TeamMemory, VectorFs, OverlayFS, ZeroCopySfs, SkillModule, FailureTaxonomy (5 classes), ExceptionSelfHeal, CorrectivePrompting, Verifier, EventLog, BudgetedRecovery, SilentFailureDetector, FailurePrediction, NotificationGate. ~350 LOC.

### ✅ Scheduler performance fix (Sprint 95/96 runtime)
- RTL8139 RX debug rate-limited (1/100 chamadas) — serial flood eliminado
- Scheduler skipa agentes passivos (>50 consecutive Pending → 80% skip)
- `has_event` agora depende de `ScheduleKind` real, não hardcoded `true`

## Arquivos Chave
| Arquivo | Função |
|---|---|
| `disk_agent/mod.rs` | DiskIntelligenceAgent (198 LOC) |
| `disk_agent/controller.rs` | StorageController trait + AtaCtrl + UsbMscCtrl + NvmeCtrl |
| `disk_agent/fs_probe.rs` | FilesystemProbe + 10 probes (260 LOC) |
| `disk_agent/nvme.rs` | NVMe driver (239 LOC) |
| `memory_agent.rs` | Adaptive budget + CPU calibration + dynamic tick |
| `allocator.rs` | resize_heap_to_mb() + CURRENT_HEAP_MB |
| `tpm.rs` | TPM 2.0 TIS + SHA256 embedded (279 LOC) |
| `identity.rs` | Ed25519 kernel verification |
| `agents.rs` | HermesAgent event-driven + Cortex fallback |

---

## Navegação Rápida para AI DEVs

```
📁 docs/                         → Toda a documentação
├── 📄 sprint-plan-84-95.md      → PLANO: 9 sprints com todos os items do IDEA_BANK
├── 📄 TODO.md                   → CHECKLIST: sub-itens, goals, dificuldades por sprint
├── 📄 roadmap.md                → VISÃO GERAL: blocos completos e futuros
├── 📄 integration-adrs-idea-bank-sprints-todo.md  → RASTREABILIDADE ADR×IDEA×SPRINT
├── 📁 architecture/             → ADRs: decisões arquiteturais (38 documentos)
│   └── 📄 0037-smp-gpu-architecture.md  → ADR mais recente: SMP+GPU multi-vendor
├── 📁 memory/                   → Estado, ideias, sessões
│   ├── 📄 STATE.md              → ⭐ COMEÇE AQUI: estado atual do kernel
│   ├── 📄 IDEA_BANK.md          → 354 ideias catalogadas
│   ├── 📄 SESSION_INDEX.md      → Índice de 42 sessões + lições críticas
│   └── 📄 SESSION_NNN.md       → Sessões individuais com debug e descobertas
└── 📁 research/                 → Pesquisas de ecossistema
📄 AGENTS.md                     → ⭐ POLÍTICAS: regras de engenharia, premissas
📄 crates/neural-kernel/         → CÓDIGO FONTE (kernel bare-metal)
```
