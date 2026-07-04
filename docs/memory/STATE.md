# ════════════════════════════════════════════════════════
#   STATE — neural-os-core v0.78.0-design 🏆
#   SPRINT 78 COMPLETO — Agentic Evolution
#   132 arquivos Rust, ~15.900 LOC, 0 erros
# ════════════════════════════════════════════════════════

## Marcos Acumulados
- **v0.56.0-v0.67.0** — 22 sprints de OS neural, GPU, desktop, agentes, ecossistema
- **v0.68.0-v0.70.0** — USB Mass Storage, xHCI bulk, BootLogAgent, FAT32 writer
- **v0.71.0** — Boot Bughunt: Agent-First + DiagnosticSkill + FAT12 log + Xuvisco
- **v0.73.0-0.73.1** — Consciousness (10 métricas), Self-Improvement Loop, Shutdown tracking
- **v0.74.0-0.74.2** — TPM TIS driver, Ed25519 kernel signing, Partition mask 0x1C
- **v0.75.0-0.75.6** — FAT32-only, DiskIntelligenceAgent (680 LOC, 6 controllers, 10+ FS probes)
- **v0.76.0-0.76.1** — NVMe driver, S.M.A.R.T., Adaptive heap, Dynamic tick, Event-driven Hermes
- **2026-07-04** — **Roadmap readequado:** 28 sprints replanejados por dependência. Itens B-01 empurrados para Sprint 85+. Premissa Activation on Demand adicionada.
- **2026-07-04** — **Sprint 77 completo:** 7 Foundation Quick Wins (~380 LOC). QEMU + VirtualBox (2 vCPUs) 0 erros. VirtualBox SMP fix: AP_COUNT static previne INIT-SIPI-SIPI sem APs.
- **2026-07-04** — **Sprint 78 completo:** 8 itens de Agentic Evolution. 0 erros. ~400 LOC novos.

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

## Roadmap Readequado (Sprints 77-84)

| Sprint | v | Bloco | Foco | LOC |
|---|---|---|---|---|
| **77** | 0.77.x | **21** | **Foundation Quick Wins** — Prompt >, Pre-Flight, TaskSchema, /learn, Fan-out | ~760 |
| **78** | 0.78.x | **22** | **Agentic Evolution** — Crew/Flow, Cache, Workflow, GGUF, WASM | ~2720 |
| **79** | 0.79.x | **23** | **LLM Infrastructure** — AVX2, Trinity MoE, Candle, TrainingAgent | ~1450 |
| **80** | 0.80.x | **24** | **JARVIS Persona** — SOUL.md, IPW, Compression, Notification Gate | ~950 |
| **81** | 0.81.x | **25** | **JARVIS Emotion** — Emotion, Contracts, Discovery, Cache, Pipeline | ~1200 |
| **82** | 0.82.x | **26** | **JARVIS Cognitive** — Dreaming, Ego, Heartbeats, Auto-Skills, SleepCycle | ~1680 |
| **83** | 0.83.x | **27** | **JARVIS Security + AHCI** — Fail-Closed, Merkle, Fluid, AHCI | ~1200 |
| **84** | 0.84.x | **28** | **GPU Compute** — Intel GEN shader | ~800 |
| **85+** | 0.85.x+ | **29+** | **AIOS Evolution** — WWW, Voice, SKYNET (pós B-01) | ~7500 |

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

## Pendente Técnico
- **JARVIS agents**: ~5650 LOC, Sprints 80-83
- **Intel GEN shader**: ~800 LOC, Sprint 84
- **AHCI driver**: ~700 LOC, Sprint 83
- **B-01 RX fix**: ~500 LOC, Sprint 85+ (bloqueador)

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
