# GPU Architecture — Neural OS Hermes AIOS

**v1.0** — Como o Neural OS gerencia GPUs para display + compute + LLM.

---

## Princípios

1. **iGPU display, dGPU compute** — quando ambos presentes, o iGPU (Intel/AMD) cuida da tela, o dGPU (NVIDIA/AMD) faz computação da LLM
2. **VRAM como tier MHI** — toda GPU com BAR2 tem sua memória mapeada como `AllocTier::Vram`
3. **Display sem GPU** — framebuffer UEFI ou VirtIO-GPU funcionam sem aceleração 3D
4. **GPU única** — quando não há iGPU, a própria dGPU faz display + compute (via VBIOS)
5. **Firmware extraction** — detecta e extrai firmware do driver NVIDIA instalado pelo usuário

---

## Componentes

### gpu/mod.rs — Detecção e Inicialização

```rust
pub enum GpuBackend {
    /// Intel iGPU: ring buffer protocol (Gen6+)
    IntelRing { device_id: u16, has_render: bool, has_blitter: bool },
    /// NVIDIA dGPU: PFIFO + VBIOS display (Pascal+)
    NvidiaFull { clock: ClockMode },
    /// NVIDIA dGPU sem firmware (P8 = 400MHz base)
    NvidiaP8,
    /// AMD RDNA: PM4 packet submission
    AmdPm4 { family: AmdFamily },
    /// VirtIO-GPU com virgl (QEMU)
    VirtIoVirgl,
    /// Sem GPU acelarada — fallback CPU
    CpuOnly,
}

pub enum ClockMode { P8, P0, Max }

pub fn detect_gpu() -> (GpuBackend, Option<DisplayGpu>) {
    let devices = scan_pci();
    let mut compute = GpuBackend::CpuOnly;
    let mut display = None;

    for dev in &devices {
        if dev.class != 0x03 { continue; } // Display controller
        match (dev.vendor_id, has_igpu(dev), can_compute(dev)) {
            // Intel iGPU → sempre display, compute opcional
            (0x8086, true, _) => { display = Some(DisplayGpu::Intel(dev)); compute = detect_intel_compute(dev); }
            // NVIDIA → compute sem display (a menos que seja o único GPU)
            (0x10DE, _, true) if display.is_some() => { compute = detect_nvidia(dev); }
            // NVIDIA → display + compute (GPU única, sem iGPU)
            (0x10DE, _, true) if display.is_none() => { display = Some(DisplayGpu::Nvidia(dev)); compute = detect_nvidia(dev); }
            // AMD → ambos display e compute
            (0x1002, _, true) => { display = Some(DisplayGpu::Amd(dev)); compute = detect_amd(dev); }
            _ => {}
        }
    }
    (compute, display)
}
```

### gpu/intel.rs — Intel iGPU Ring Buffer (~700 LOC)

Protocolo: `MI_BATCH_BUFFER_START`, `MI_LOAD_REGISTER_IMM`, `MI_FLUSH`

```
Controla via MMIO BAR0 (Gen9.5):
├── 0x120000 → RENDER_RING_BASE
├── 0x120034 → RENDER_RING_HEAD
├── 0x120038 → RENDER_RING_TAIL
└── 0x12003C → RENDER_RING_CTL

Ring buffer em DRAM (alloc_pages):
┌──────┬──────┬──────┬──────┐
│ cmd1 │ cmd2 │ cmd3 │ cmd4 │  → comandos GEN executam no EU (Execution Units)
└───↑──┴──────┴──────┴──────┘
    │
  DRAM (acessivel pela GPU via IOMMU)
```

Operações implementadas:
- `gpu_matmul(a, b)` → batch buffer com shader assembly gen9
- `gpu_blit(src, dst)` → via blitter engine dedicada
- `gpu_fill(color)` → preencher framebuffer rapidamente

### gpu/nvidia.rs — NVIDIA PFIFO + VRAM (~1500 LOC)

Protocolo: `PUSH_BUFFER` + `METHOD_COUNT` via PFIFO

```
BAR0 (16MB MMIO) → regiões:
├── 0x000000–0x001000: VERSION, GPU info
├── 0x001000–0x002000: DISPLAY (heads, PLL, ramdac)
├── 0x002000–0x008000: PFIFO (PUSH_BUFFER submission)
│   └── 0x002000: PUSH_BUFFER base address
│   └── 0x002004: PUSH_BUFFER size (dwords)
│   └── 0x002008: PUSH_BUFFER tail (CPU escreve após cada push)
├── 0x008000–0x00C000: RAMIN (memory management, pages)
└── 0x00C000–0x010000: PBDMA (DMA engine, copies)

PUSH_BUFFER channel:
┌──────────────────────────────┐
│ METHOD_COUNT 0x90, 0x1234    │  → escreve 0x1234 no reg 0x90
│ METHOD_COUNT 0x94, 0x5678    │  → escreve 0x5678 no reg 0x94
│ REGISTER_READ 0x100           │  → executa load de VRAM
│ REGISTER_WRITE 0x104          │  → executa store em VRAM
│ DMA_COPY src, dst, len       │  → copia entre VRAM e RAM
│ INTERRUPT                    │  → notifica completação
└──────────────────────────────┘
```

### gpu/amd.rs — AMD RDNA PM4 (~2000 LOC)

Protocolo: `PKT3_*` via ring buffer

```
Ring buffer em DRAM (dwords):
╔══════════════════════════════════╗
║ PKT3_WRITE_DATA(VM, addr)       ║  → carrega firmware microcode
║ PKT3_WRITE_DATA(DST, data)      ║  → inicializa registers
║ PKT3_ACQUIRE_MEM                ║  → barreira de memória
║ PKT3_DMA_DATA(src, dst, len)    ║  → copia via DMA engine
║ PKT3_RELEASE_MEM                ║  → libera recurso
║ PKT3_SET_BASE(addr)             ║  → configura base de compute
╚══════════════════════════════════╝
```

### gpu/virtio.rs — VirtIO-GPU virgl (~400 LOC)

Protocolo: `VIRTIO_GPU_CONTEXT_INIT` + Gallium3D TGSI.

Usa o host GPU do QEMU como backend. Para desenvolvimento e teste (QEMU).

---

## MHI Integration: VRAM como Tier

```rust
// mhi.rs — já temos AllocTier::Vram
pub fn alloc_by_tier(tier: AllocTier, size: usize) -> Option<PhysAddr> {
    match tier {
        AllocTier::Vram => {
            // Mapeia BAR2 da GPU (VRAM física)
            // Se não mapeável, fallback para DRAM
            gpu_map_vram(size)
        }
        // ...
    }
}
```

---

## GPU Swap entre modelos (Cortex)

Quando o usuário troca o modelo da LLM via `/model <caminho.gguf>`:

```rust
pub fn model_swap(path: &str) -> bool {
    let gpu = detect_gpu();
    let model_size = file_size(path);
    let vram_avail = vram_free();
    
    match gpu {
        GpuBackend::IntelRing | GpuBackend::NvidiaP8 
            if model_size < vram_avail => {
            // Carrega modelo direto na VRAM!
            load_model_to_vram(path)
        }
        GpuBackend::CpuOnly => {
            // Fallback CPU (BitNet existente)
            load_model_to_dram(path)
        }
        _ => {
            // VRAM insuficiente, fallback
            load_model_to_dram(path)
        }
    }
}
```

---

## GPU Architecture Layers

```
┌──────────────────────────────────────────────────────────┐
│                   Neural OS Hermes                        │
├──────────────────────────────────────────────────────────┤
│                                                           │
│  ┌─────────┐  ┌──────────┐  ┌────────┐  ┌───────────┐   │
│  │  User    │  │  Hermes  │  │  LLM   │  │   VFS     │   │
│  │  Input   │  │  Agent   │  │ Cortex │  │  /dev/    │   │
│  └────┬────┘  └────┬─────┘  └───┬────┘  └─────┬─────┘   │
│       │            │            │             │         │
│  ┌────┴────────────┴────────────┴─────────────┴────┐    │
│  │              EventBus IPC                        │    │
│  └────┬──────────────────────────────────────┬─────┘    │
│       │                                      │          │
│  ┌────┴────────────────────┐  ┌──────────────┴───────┐  │
│  │  GPU Manager            │  │  MHI (Memory Tiers)  │  │
│  │  ┌──────────────────┐   │  │  ┌────────────────┐  │  │
│  │  │ Intel Ring       │   │  │  │ AllocTier::Vram│  │  │
│  │  │ NVIDIA PFIFO     │   │  │  │ (BAR2 mapped)  │  │  │
│  │  │ AMD PM4          │   │  │  │ AllocTier::Dram│  │  │
│  │  │ VirtIO virgl     │   │  │  │ (heap/DRAM)    │  │  │
│  │  └──────────────────┘   │  │  └────────────────┘  │  │
│  └─────────────────────────┘  └──────────────────────┘  │
│                                                           │
│  ┌──────────────────────────────────────────────────────┐ │
│  │           Display Output                              │ │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌────────┐  │ │
│  │  │ Framebuf │ │ Intel    │ │ NVIDIA   │ │ AMD    │  │ │
│  │  │ UEFI     │ │ Display  │ │ Display  │ │ DCN    │  │ │
│  │  │          │ │ Engine   │ │ Engine   │ │ Engine │  │ │
│  │  └──────────┘ └──────────┘ └──────────┘ └────────┘  │ │
│  └──────────────────────────────────────────────────────┘ │
│                                                           │
├──────────────────────────────────────────────────────────┤
│            Hardware Layer                                  │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌─────────────┐  │
│  │ Intel HD │ │ NVIDIA   │ │ AMD      │ │ VirtIO-GPU  │  │
│  │ 620      │ │ GTX 1050 │ │ Radeon   │ │ (QEMU)      │  │
│  └──────────┘ └──────────┘ └──────────┘ └─────────────┘  │
└──────────────────────────────────────────────────────────┘
```

---

## Resumo por Hardware

| Hardware | Display | Compute | VRAM | LLM Inference |
|---|---|---|---|---|
| Intel HD 620 + GTX 1050 | Intel ✅ | NVIDIA PFIFO | VRAM 2GB | 10× CPU |
| Intel HD 620 só | Intel ✅ | Intel Ring | DRAM | 3× CPU |
| AMD iGPU + NVIDIA | AMD ✅ | NVIDIA PFIFO | VRAM | 10× CPU |
| Só NVIDIA | NVIDIA VBIOS | NVIDIA PFIFO | VRAM | 10× CPU |
| Só AMD | AMD DCN | AMD PM4 | VRAM | 15× CPU |
| QEMU (VirtIO) | Framebuffer UEFI | VirtIO virgl | DRAM | 1× CPU |
| Sem GPU | UEFI | N/A | N/A | BitNet CPU |
