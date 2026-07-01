# ════════════════════════════════════════════════════════
#   PLANO DIRETOR — neural-os-core v0.66.0 🏆
#   GPU DETECT + VRAM TIER + INTEL RING BUFFER + BACKEND
#   122 arquivos Rust, ~12.500 LOC, 0 erros
# ════════════════════════════════════════════════════════

## 🏆 Marcos Acumulados
- **v0.56.0** — Medusa 3-head speculative decoding + Pipeline + Memory Tree + KG + DAG
- **v0.57.0** — Bloco 15+16+17: Memory Systems + Ecosystem + LLM v2
- **v0.57.1** — Consolidation: Plugin Hub, x2APIC, Ed25519, SMP stacks
- **v0.58.0** — 🏆 Boot em Hardware Real (SDHC USB) + xHCI + FAT12 + ATA + CAD
- **v0.59.0** — 🏆 Bootloader 0.11.15 + Framebuffer 1280x720 UEFI
- **v0.59.1** — The Agency (147 agents) + HW Agents
- **v0.59.2** — Ecosystem Batch 3: 12 repos portados
- **v0.60.0** — Double buffer + Security Pipeline + MHI + UserProfile
- **v0.60.1** — WASM+TV-DSL+AVX2+e1000+Heap
- **v0.60.2** — WHPX+USB-MSC BOT+GGUF loader
- **v0.60.3** — e1000 fix (map_page_uc + mmio_virt)
- **v0.60.4** — RTL8139 TX fix (TSD shift + iPXE sync)
- **v0.60.5** — RTL8139 early init 32KB RX
- **v0.61.0** — 🏆 Desktop: MouseAgent + Theme + Compositor + Shell + 3 Apps + WASM
- **v0.62.0** — 🏆 VFS Layer + MHI ARC-style tier suggestion
- **v0.62.1** — Storage Agents: AtaAgent, DevFsAgent, ProcFsAgent
- **v0.62.2** — InferenceFS + HermesFS + RamFS + MhiScheduler
- **v0.63.0** — Cortex Evolution: Model trait + PTRM + Kanerva
- **v0.63.1** — MegaTrain + Self-skill generation
- **v0.64.0** — Voice skill + Gbrain reranker + BrowserAgent
- **v0.65.0** — COSMIC UI + AxiomOS verifier + HAL + Bench
- **v0.66.0** — 🏆 **GPU: detect + VRAM tier + Intel ring buffer + backend + cube**

## Arquitetura Fundamental
**Tudo no Neural OS Hermes é um Agente ou uma Skill.**
173+ agentes: 20 nativos + 147 The Agency + ~6 HW + 3 FS.
Bootloader 0.11.15 com `bootloader_api`. Framebuffer 1280x720.
GPU compute bare-metal via PCI BAR MMIO (único no mundo Rust).
Build via `cargo build --release` + `tools/build_image.py`.

## GPU Architecture
```
gpu/
├── mod.rs       — Re-exports públicos
├── detect.rs    — PCI scan class 0x03, 30+ GPUs, GpuInfo, best_compute/display
├── vram.rs      — BAR2 mapping, bump allocator, DEADBEEF test
├── intel.rs     — Ring buffer Gen9+, write/submit/wait_idle, blit, matmul stub
├── nvidia.rs    — PFIFO probe, VRAM mapping (256 páginas), P8 mode
├── amd.rs       — PM4 probe, VRAM mapping (256 páginas)
├── backend.rs   — GpuAccel enum, Mutex-safe, auto-select, gpu_matmul
└── cube.rs      — Crossfade sem float, AtomicBool, split animado
```

Prioridade GPU: Intel ring > AMD PM4 > NVIDIA PFIFO > CPU fallback.
VRAM mapeada como tier MHI (AllocTier::Vram) — bump allocator.

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

## Blocos Completos (26+ blocos, 66+ sprints)
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
| 25 | 0.62 | VFS + MHI + Storage Agents |
| 26 | 0.63-0.65 | Desktop + Cortex + Voice + COSMIC |
| **27** | **0.66** | **🏆 GPU: detect + VRAM + Intel ring + backend + cube** |

## Aprendizados Chave (Sprint 60-66)
1. **RTL8139 TSD_SIZE_SHIFT** = 0 (SIZE em bits 0-12, não 16-27)
2. **iPXE preenche RX buffer** — CAPR avança antes do kernel
3. **map_page_uc()** — criar page table entries para PCI MMIO
4. **e1000 TX non-blocking** — QEMU TCG não processa TX while guest spinning
5. **Frame allocator fragmenta** — alocar buffers grandes cedo no boot
6. **VFS + MHI fundidos** — cada VfsMount tem AllocTier
7. **GPU bare-metal MMIO**: escrever direto nos registros GPU via PCI BAR (sem Vulkan/DX)
8. **Intel ring buffer**: MI_BATCH_BUFFER_START + MI_BATCH_BUFFER_END via RENDER_RING registers
9. **VRAM bump allocator**: `vram_alloc()` com `next_offset` — não sobrescreve alocações anteriores
10. **static mut UB**: usar `Mutex` em vez de `&mut static` para evitar undefined behavior

## Pendente Técnico
- **Rede RX**: QEMU SLiRP não roteia sem DHCP
- **Intel GEN shader assembly**: matmul real via EU execution units
- **NVIDIA PFIFO PUSH_BUFFER**: ring buffer real + FALCON firmware
- **AMD PM4 ring buffer**: PKT3_WRITE_DATA, PKT3_DMA_DATA
- **BCS blitter engine**: separar blit do RCS ring
- **GTT setup**: Intel GPU precisa de GTT para batch buffers em RAM
- **VRAM free list**: substituir bump allocator por free list real
- **e1000/r8169**: HW real NIC
