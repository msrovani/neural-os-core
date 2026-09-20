# ADR-0105 — GPU Multi-ISA KernelImage + Falcon3 W2A8 AIOS

**Status:** Accepted — backlog B0–B3 código; B2.4/B4 device golden = AWAITING_HW  
**IDEA:** #550, #536; CPU F4 = ADR-0084; dispatch = ADR-0057 WS-D (#467b)  
**SESSION:** 361 (+ residuals s328/s249b); implementação unificada s366+  
**Lifecycle:** `fazendo` (device silício aberto)

## Contexto

KernelPack NKP1 existia, mas QMD/Walker usavam magic numbers e o lab assumia sm_61-only / BitNet-2B. Aceite silício = Pascal+Gen9+lab Ampere; LLM canônico = Falcon3 1.58bit (1B/3B/7B/10B).

Este ADR é o **canônico do tema W2A8** (device + ladder CPU até Ready). Não substitui ADR-0057 (dispatch) nem ADR-0084 (fidelidade BitNet).

## Decisão (MVP + B0–B3)

1. **KernelImage** parseado de CUBIN/HSACO/zebin — `regs`/`shared`/`param_*` do blob.
2. **AIOS adapt** (`aios_adapt`): SKU Falcon3 + dual iGPU/dGPU + OpProfile; `w2a8_pack_present` = FAT (≠ verified/Ready).
3. **Ready** só com pack verified + canário golden; stub/unsigned ≠ Ready.
4. **gpu_ternary** valida pack+image+shape Falcon3; sem Ready → **CPU ladder** (honesto).
5. Packers host `--op w2a8` multi-SM; `tools/pack_nkp_lab.ps1` + `GPU_KERNEL_PACK.md`.
6. **CPU W2A8** (B3): gate `GENERATION_GAPS_RESOLVED` via `apply_tier`; WHPX/KVM/bare-metal; path escalar no soft-float + maddubs no host.

## Distinções obrigatórias (honesty)

| Path | Onde | ≠ |
|------|------|---|
| **CPU W2A8** | `cortex::bitnet_w2a8` + `note_w2a8_*` | AVX2 host / scalar bare-metal |
| **GPU W2A8** | KernelPack + `note_gpu_disp_awaiting` | CUBIN no silício (AWAITING) |
| QEMU / stub | `CpuOnly` | Nunca vender como aceleração |

---

## Backlog — estado

### B0 — FAT / lab pack

| ID | Item | Estado |
|----|------|--------|
| B0.1 | Artefatos `NKP_W2A8_SM{75,80,86,89}.BIN` em `target/nkp-lab/` | **host:** `tools/pack_nkp_lab.ps1` (CUBIN se nvcc; senão CpuStub) |
| B0.2 | `mkfat32` embute packs + aliases 8.3 | ✅ |
| B0.3 | Unsigned ≠ Ready; `promote_with_session`; slog present≠verified | ✅ |

### B1 — Ready + golden

| ID | Item | Estado |
|----|------|--------|
| B1.1 | Canário `vector_add` + fence | ✅ código (Pass só silício) |
| B1.2 | Golden BitLinearW2A8 no metal | **AWAITING_HW** |
| B1.3 | QEMU → CpuOnly + slog `ok` | ✅ |

### B2 — Device dispatch

| ID | Item | Estado |
|----|------|--------|
| B2.1 | QMD/Walker com KernelImage | ✅ prep; submit AWAITING |
| B2.2 | `gpu_ternary` quando Ready | ✅ hook; device None até fence |
| B2.3 | Telemetria `w2a8[ok]` ≠ `gpu_await` | ✅ |
| B2.4 | FW vendor mínimo | **AWAITING_HW** |

### B3 — CPU W2A8 ladder ✅

| ID | Item | Estado |
|----|------|--------|
| B3.1 | `refresh_generation_gaps` + `apply_tier` | ✅ |
| B3.2 | `w2a8_enabled` WHPX/KVM/bare (não TCG) | ✅ |
| B3.3 | `note_w2a8_ok` ≠ GPU | ✅ |

### B4 — Multi-ISA residual

| ID | Item | Estado |
|----|------|--------|
| B4.1 | Intel Gen9 ocloc real | **AWAITING** toolkit + golden |
| B4.2 | AMD gfx1030+ HSACO real | **AWAITING** |
| B4.3 | sm_61 Pascal lab | opcional `pack_nkp_lab -IncludeSm61` |

---

## Não-fazer (explícito)

- PTX JIT no bare-metal  
- Fingir GPU matmul contando CPU como GPU  
- Hardcodar shapes BitNet-2B no path Falcon3  
- NPU XDNA neste ADR  
- Prefill AirLLM (sprint Infer)

## Relação com outras ADRs

| ADR | Papel |
|-----|--------|
| **0057** | Dispatch; WS-D hook Ready |
| **0084** | Fidelidade; F4 = B3 |
| **0087** | CE golden GTX — separado |
| **0101** | Lab Falcon3 tok/s consome Ready |

## Aceite

- **Código (s366+):** B0.2/B0.3, B1.1/B1.3, B2.1–B2.3, B3 completo.  
- **Silício:** B1.2 + B2.4 + B4 com pack CUBIN/zebin/HSACO real + fence.

## Evidência / tools

- `tools/GPU_KERNEL_PACK.md`  
- `tools/pack_nkp_lab.ps1`  
- `tools/pack_{nvidia,intel,amd}_kernels.py`  
- `k_hal::gpu::{kernel_pack,falcon3_w2a8,aios_adapt,compute_dispatch}`  
- `cortex::{bitnet_w2a8,matmul_diag,difficulty_gate,refresh_generation_gaps}`
