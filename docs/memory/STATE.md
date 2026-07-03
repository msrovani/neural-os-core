# ════════════════════════════════════════════════════════
#   STATE — neural-os-core v0.76.1 🏆
#   TPM + DISK AGENT + NVMe + SMART + ADAPTIVE HEAP + DYNAMIC TICK
#   132 arquivos Rust, ~15.500 LOC, 0 erros
# ════════════════════════════════════════════════════════

## Marcos Acumulados
- **v0.56.0-v0.67.0** — 22 sprints de OS neural, GPU, desktop, agentes, ecossistema
- **v0.68.0-v0.70.0** — USB Mass Storage, xHCI bulk, BootLogAgent, FAT32 writer
- **v0.71.0** — Boot Bughunt: Agent-First + DiagnosticSkill + FAT12 log + Xuvisco
- **v0.73.0-0.73.1** — Consciousness (10 métricas), Self-Improvement Loop, Shutdown tracking
- **v0.74.0-0.74.2** — TPM TIS driver, Ed25519 kernel signing, Partition mask 0x1C
- **v0.75.0-0.75.6** — FAT32-only, DiskIntelligenceAgent (680 LOC, 6 controllers, 10+ FS probes)
- **v0.76.0-0.76.1** — NVMe driver, S.M.A.R.T., Adaptive heap, Dynamic tick, Event-driven Hermes

## Arquitetura Fundamental
**Tudo no Neural OS Hermes é um Agente ou uma Skill.**
247+ agentes: 20 nativos + 147 The Agency + ~80 importados + ~6 HW + ~6 FS.
Bootloader 0.11.15 com `bootloader_api`. Boot sequence agent-centric.

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

### Agent Tier Classification (Premissa v0.76.1)
| Tier | Schedule | Exemplos |
|---|---|---|
| Permanent | Continuous | Hermes, Display, HwBridge |
| SystemDemand | EventDriven | DiskAgent, Cortex, Net |
| UserDemand | EventDriven | Skills, Apps, Plugins |
| Periodic | PollEvery(N) | Cron, Observer, Optimizer |
| Learning | PollEvery(2000) | Novos agentes → analisados 5000 ticks → promovidos |

## Aprendizados Chave
1. **VGA CRTC + UEFI GOP = incompatível** (Sprint 71)
2. **Cortex acorda antes do HW** — LLM deve participar das decisões de hardware
3. **FAT12 removido** — FAT32-only, 102 LOC eliminados
4. **Partition mask 0x1C** — mbr_nostd aceita Hidden FAT32, bootloader OK, SO não monta
5. **TPM fallback** — silencioso se ausente (0xFFFF FFFF), Ed25519 como enforcement primário
6. **RX=0 persistente** — QEMU slirp + VirtualBox bridge, pre-existente (B-01)
7. **Hermes event-driven** — 84 linhas/seg → 0 quando ocioso
8. **Tick dinâmico** — calibrado por workload (12-192 t/s)
9. **AgentTier** — classificação obrigatória, Learning como fallback

## Pendente Técnico
- **RX fix (B-01)**: QEMU SLiRP DHCP/RX bloqueia toda cadeia WWW
- **GGUF loader (#278)**: ~500 LOC para modelos 1B+ params
- **WASM runtime (ADR-0032)**: wasmi + WASI→Skill bridge
- **Intel GEN shader**: matmul real via EU execution units
- **AHCI driver**: SATA 6G com NCQ (~700 LOC)
- **USB write path**: UsbMscCtrl::write_blocks atual é stub
- **e1000/r8169**: HW real NIC driver

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
