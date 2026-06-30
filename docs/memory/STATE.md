# ════════════════════════════════════════════════════════
#   PLANO DIRETOR — neural-os-core v0.62.2 🏆
#   VFS + MHI BRIDGE + STORAGE AGENTS (ATA, DEV, PROC)
#   8 VFS mounts, ARC-style tiering, 96 arquivos Rust
# ════════════════════════════════════════════════════════

## 🏆 Marcos Acumulados
- **v0.56.0** — Medusa 3-head speculative decoding + Pipeline + Memory Tree + KG + DAG
- **v0.57.0** — Bloco 15+16+17: Memory Systems + Ecosystem + LLM v2
- **v0.57.1** — Consolidation: Plugin Hub, x2APIC, Ed25519, SMP stacks
- **v0.58.0** — 🏆 Boot em Hardware Real (SDHC USB) + xHCI + FAT12 + ATA + CAD
- **v0.59.0** — 🏆 Bootloader 0.11.15 + Framebuffer 1280×720 UEFI
- **v0.59.1** — The Agency (147 agents) + HW Agents
- **v0.59.2** — Ecosystem Batch 3: 12 repos portados
- **v0.60.0** — Double buffer + Security Pipeline + MHI + UserProfile
- **v0.60.1** — WASM+TV-DSL+AVX2+e1000+Heap
- **v0.60.2** — WHPX+USB-MSC BOT+GGUF loader
- **v0.60.3** — e1000 fix (map_page_uc + mmio_virt)
- **v0.60.4** — RTL8139 TX fix (TSD shift + iPXE sync)
- **v0.60.5** — RTL8139 early init 32KB RX
- **v0.62.0** — 🏆 **VFS Layer + MHI ARC-style tier suggestion**
- **v0.62.1** — **Storage Agents: AtaAgent, DevFsAgent, ProcFsAgent**

## Arquitetura Fundamental
**Tudo no Neural OS Hermes é um Agente ou uma Skill.**
173+ agentes: 20 nativos + 147 The Agency + ~6 HW + 3 FS.
Bootloader 0.11.15 com `bootloader_api`. Framebuffer 1280×720.
Build via `cargo build --release` + `tools/build_image.py`.

## VFS Architecture
```
VFS path → MHI tier → StorageAgent → Block device
    ↑
MHI Registry (AllocProfile + arc_suggest_tier + auto-migration)
```

| Mount | Agent | Tier | Driver |
|---|---|---|---|
| /mnt/hdd/ | AtaAgent | HDD | ATA PIO ✅ |
| /mnt/sdhc/ | UsbMscAgent | USB-MSC | BOT (stub) |
| /mnt/ram/ | RamFsAgent | DRAM | heap |
| /chat/ | HermesFsAgent | Virtual | Hermes |
| /dev/ | DevFsAgent | Virtual | PCI scan ✅ |
| /proc/ | ProcFsAgent | Virtual | AgentRegistry ✅ |
| /inference/ | InferenceFsAgent | LLM | Cortex+KG |
| /system/ | SysFsAgent | Virtual | System |

## Blocos Completos (25+ blocos, 62+ sprints)
| Bloco | v | Foco |
|---|---|---|
| 1-14 | 0.1-0.55 | OS+Neural+SelfHeal+Hermes Cognitive |
| 15-17 | 0.57 | Memory Systems + Ecosystem + LLM v2 |
| 18 | 0.56 | Ecosystem Batch (Pipeline, DAG) |
| 19 | 0.58 | HW Real (USB, FAT12, ATA) |
| 20 | 0.59 | Bootloader 0.11 + Framebuffer |
| 21 | 0.59.1 | The Agency (147 agents) |
| 22 | 0.59.2 | Ecosystem Batch 3 |
| 23 | 0.60 | Correção Estrutural (Buffer, Security, MHI, Profile) |
| 24 | 0.61 | WASM+TV-DSL+AVX2+GGUF+Heap |
| 25 | 0.62 | **VFS + MHI + Storage Agents** |

## Aprendizados Chave (Sprint 60-62)
1. **RTL8139 TSD_SIZE_SHIFT** = 0 (SIZE em bits 0-12, não 16-27)
2. **iPXE preenche RX buffer** — CAPR avança antes do kernel
3. **map_page_uc()** — criar page table entries para PCI MMIO (set_page_uc só modifica)
4. **e1000 TX non-blocking** — QEMU TCG não processa TX while guest spinning
5. **Frame allocator fragmenta** — alocar buffers grandes cedo no boot
6. **VFS + MHI fundidos** — cada VfsMount tem AllocTier, ARC-style tier suggestion

## Pendente Técnico
- **Rede RX**: QEMU SLiRP não roteia sem DHCP
- **InferenceFsAgent**: /inference/ com LLM
- **HermesFsAgent**: /chat/ como filesystem
- **RamFsAgent + Auto Tier Migration**
- **USB-MSC BOT**: bulk endpoints xHCI
- **e1000/r8169**: HW real NIC
