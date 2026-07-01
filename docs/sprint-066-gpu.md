# Sprint 66 — GPU Architecture: VRAM Tier + Ring Buffer + Firmware

**v0.66.0** — GPU compute e display para Neural OS Hermes.
Toda GPU vira acelerador de IA. VRAM como tier MHI. Display via iGPU ou GPU única.

---

## Análise de Hardware

### Matriz de suporte

| GPU | Display | Compute | LLM ganho | Firmware | LOC |
|---|---|---|---|---|---|
| **Intel HD 620 / Arc** | ✅ Engine própria | ✅ Ring buffer | 3-35× | Open source | ~700 |
| **NVIDIA GTX 1050 – RTX 5090** | ✅ VBIOS | ✅ PFIFO + BAR2 | 10-120× | Extrair do driver | ~1500 |
| **AMD RX 6600 – RX 9070 XT** | ✅ DCN | ✅ PM4 + BAR2 | 15-50× | MIT incluso | ~2000 |
| **VirtIO-GPU (QEMU)** | ✅ Framebuffer | ✅ virgl | Host GPU | Host GPU | ~400 |

### Firmware Policy

| GPU | Como obtém | Legal |
|---|---|---|
| **Intel** | Incluso no Neural OS (open source) | ✅ MIT |
| **AMD** | Incluso no Neural OS (linux-firmware) | ✅ MIT |
| **NVIDIA** | Extrai do driver instalado pelo usuário | ✅ Usuário possui o hardware |
| **NVIDIA (sem driver)** | Opera em P8 mode (405MHz, ~500 GFLOPS) | ✅ |

---

## Sub-Sprints

### 66.0 — VRAM Tier (~300 LOC)
**Objetivo:** `AllocTier::Vram` funcional via BAR2 mapping

- `mhi.rs`: `alloc_by_tier(Vram)` mapeia BAR2 de qualquer GPU detectada
- `gpu/detect.rs`: PCI scan → class 0x03 → BAR0/BAR2 → `GpuInfo`
- `gpu/vram.rs`: `gpu_map_vram(size)` → mapeia páginas da VRAM no espaço de endereço
- Teste: `alloc_by_tier(Vram, 16MB)` → retorna endereço físico na VRAM

### 66.1 — Intel iGPU Ring Buffer (~700 LOC)
**Objetivo:** Display + compute via Intel Gen9+

- `gpu/intel.rs`: `IntelGpu::probe()` detecta HD 620 / Arc / Battlemage
- `gpu/intel.rs`: Ring buffer init — `MI_BATCH_BUFFER_START`, `MI_FLUSH`
- `gpu/intel.rs`: `gpu_matmul()`, `gpu_blit()`, `gpu_fill()`
- `gpu/intel.rs`: Blitter engine para Desktop Cube (cópia GPU→GPU)
- Display: Intel display pipe → framebuffer scanout

### 66.2 — AMD PM4 Compute (~2000 LOC)
**Objetivo:** Compute via PM4 packets para RDNA

- `gpu/amd.rs`: `AmdGpu::probe()` detecta RX 6000/7000/9000
- `gpu/amd.rs`: Ring buffer init — `PKT3_WRITE_DATA`, `PKT3_DMA_DATA`
- `gpu/amd.rs`: Firmware loader — PSP init (firmware incluso no OS)
- `gpu/amd.rs`: Display via DCN engine (quando GPU única)
- Suporte de fábrica: RX 6600 → RX 9070 XT

### 66.3 — NVIDIA PFIFO + FALCON (~1500 LOC)
**Objetivo:** Compute via PFIFO, VRAM tier, firmware extraction

- `gpu/nvidia.rs`: `NvidiaGpu::probe()` detecta Pascal+ (GTX 1050 → RTX 5090)
- `gpu/nvidia.rs`: PFIFO ring buffer — `PUSH_BUFFER` + `METHOD_COUNT`
- `gpu/nvidia.rs`: VRAM BAR2 mapping — `gpu_vram_alloc(size)` → PhysAddr
- `gpu/nvidia.rs`: Firmware loader — FALCON boot (extração do driver)
- `gpu/nvidia.rs`: Display engine via VBIOS (quando GPU única)
- Fallback: P8 mode (405MHz, sem firmware) — sempre funcional

### 66.4 — Cortex GPU Backend (~400 LOC)
**Objetivo:** LLM inference via GPU

- `cortex.rs`: `GpuBackend` — seleciona automaticamente
- `gpu/backend.rs`: `gpu_forward(model, tokens)` → matmul na GPU
- `gpu/backend.rs`: `gpu_attention(q, k, v)` — atenção acelerada
- Fallback: CPU BitNet `matmul_hybrid()` (sempre ativo)

### 66.5 — Desktop Cube + Blitter (~200 LOC)
**Objetivo:** Transição visual entre workspaces

- `display/workspace.rs`: Crossfade entre workspaces
- Intel Blitter: cópia GPU→GPU para transição suave
- Fallback: CPU software blend

---

## Dependências

```
66.0 (VRAM Tier)
  └→ 66.1 (Intel Ring) ─→ 66.4 (Backend) ─→ 66.5 (Cube)
  └→ 66.2 (AMD PM4) ──→ 66.4
  └→ 66.3 (NVIDIA) ───→ 66.4
```

66.0 é o pré-requisito para tudo — VRAM funcional já entrega ganho. 66.1, 66.2 e 66.3 são independentes entre si. 66.4 unifica todos.

---

## Summary

| Sub-Sprint | Feature | LOC | Prioridade | Dependências |
|---|---|---|---|---|
| 66.0 | VRAM Tier | ~300 | 🔴 Crítica | PCI scan |
| 66.1 | Intel Ring Buffer | ~700 | 🟡 Alta | 66.0 |
| 66.2 | AMD PM4 Compute | ~2000 | 🟡 Média | 66.0 |
| 66.3 | NVIDIA PFIFO + FALCON | ~1500 | 🟡 Alta | 66.0 |
| 66.4 | Cortex GPU Backend | ~400 | 🟡 Alta | 66.1/66.2/66.3 |
| 66.5 | Desktop Cube | ~200 | 🟢 Baixa | 66.4 |
| **Total** | **6 features** | **~5100 LOC** | | |
