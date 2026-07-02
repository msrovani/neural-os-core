# ════════════════════════════════════════════════════════
#   PLANO DIRETOR — neural-os-core v0.71.0 🏆
#   BOOT BUGHUNT: Agent-First Boot + DiagnosticSkill + FAT12 Log + Xuvisco Fix
#   126 arquivos Rust, ~13.800 LOC, 0 erros
# ════════════════════════════════════════════════════════

## 🏆 Marcos Acumulados
- **v0.56.0-v0.67.0** — 22 sprints de OS neural, GPU, desktop, agentes, ecossistema
- **v0.68.0-v0.70.0** — USB Mass Storage, xHCI bulk, BootLogAgent, FAT32 writer
- **v0.71.0** — 🏆 **Boot Bughunt: Agent-First refactoring + DiagnosticSkill + FAT12 log + Xuvisco fix**

## Arquitetura Fundamental
**Tudo no Neural OS Hermes é um Agente ou uma Skill.**
247+ agentes: 20 nativos + 147 The Agency + ~80 importados + ~6 HW + ~6 FS.
Bootloader 0.11.15 com `bootloader_api`. Boot sequence agent-centric com `BOOT_PHASE` events.
GPU compute bare-metal via PCI BAR MMIO (único no mundo Rust).

## Boot Architecture (v0.71.0 — Agent-First)
O boot agora segue 8 fases, cada uma publicando `BOOT_PHASE` no EventBus:

| Fase | Agente | O que acontece |
|---|---|---|
| 0. SafeHarbor | kernel_main | Serial + FB/VGA + IDT (minimo para sobreviver) |
| 1. MemoryCore | SystemAgent | Frame allocator + Page tables + Heap + SIMD |
| 2. SystemBringup | SystemAgent | CortexAgent acorda (pre-HW!) |
| 3. Diagnostics | SystemAgent → DiagnosticSkill | Testes de alocador, tensor, MLP, BitNet |
| 4. HardwareDiscovery | PCIAgent + PlatformAgent | PCI scan + ACPI + SMP + GPU detect + HwRegistry |
| 5. DriverInit | NetDriverAgent + AtaAgent + UsbDriverAgent | RTL8139, e1000, ATA, xHCI (como agentes) |
| 6. AgentFleet | AgentRegistry | Todos os agentes registrados e iniciados |
| 7. Runtime | AgentScheduler::run() | HermesAgent lidera, Cortex pensa, agentes agem |

## Xuvisco Bug (Corrigido)
**Causa raiz:** VGA text mode inicializava ANTES do framebuffer probe. `println!` escrevia nos registros VGA CRTC (portas 0x3D4/0x3D5) via `set_cursor()`. Em Intel 6xx com UEFI GOP, isso corrompia o scanout.

**Fix:** Framebuffer sondado imediatamente no início de `kernel_main`. Se disponível, toda saída usa `fb_print()` sem tocar nos registros VGA.

## FAT12 Log Bug (Corrigido)
**Causa raiz:** `boot_logger.rs` só aceitava FAT32 (type_code 0x0B/0x0C). `patch_image.py` cria partição FAT12 (type_code 0x01). `write_boot_log()` usava `Fat32Writer` que rejeitava FAT12.

**Fix:** `write_boot_log()` usa `Fat12Writer` para FAT12, `Fat32Writer` para FAT32. `BootLogAgent` lê ambos os formatos.

## Aprendizados Chave (Sprint 71)
1. **VGA CRTC + UEFI GOP = incompatível** — escrever nas portas 0x3D4/0x3D5 em modo gráfico corrompe o display Intel 6xx
2. **DiagnosticSkill substitui teste inline** — Box/Vec/Tensor/SiLU agora são skill, não código procedural no boot
3. **Cortex acorda antes do HW** — o LLM deve estar disponível para participar das decisões de hardware
4. **Boot phase events** — cada fase publica `BOOT_PHASE` no EventBus para Hermes/Cortex acompanharem
5. **FAT12 vs FAT32** — `Fat32Reader::new()` rejeita type_code 0x01; usar `Fat12Writer` para FAT12
6. **Agent-first boot** — boot procedural é anti-padrão; cada fase deve pertencer a um agente

## Pendente Técnico
- **Rede RX**: QEMU SLiRP não roteia sem DHCP
- **Intel GEN shader assembly**: matmul real via EU execution units
- **NVIDIA PFIFO PUSH_BUFFER**: ring buffer real + FALCON firmware
- **AMD PM4 ring buffer**: PKT3_WRITE_DATA, PKT3_DMA_DATA
- **BCS blitter engine**: separar blit do RCS ring
- **GTT setup**: Intel GPU precisa de GTT para batch buffers em RAM
- **VRAM free list**: substituir bump allocator por free list real
- **e1000/r8169**: HW real NIC
