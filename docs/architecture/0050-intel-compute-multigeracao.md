# ADR-0050: Intel Compute Multigeração — GuC, Walkers e Kernel Pack

**Data:** 2026-07-16  
**Status:** Proposed  
**Lifecycle:** `por_fazer`  
**Ideia:** #456  
**Complementa:** ADR-0029, ADR-0037, ADR-0047-GPU, ADR-0048 e ADR-0049  
**Hardware de validação inicial:** Intel HD 620 / Gen9 (iGPU do lab) para walker+MAD; Arc/Battlemage opcional para CCS+DPAS  

## 1. Contexto

O neural-os-core já trata Intel como caminho de display/ring em `intel.rs` e
prioriza iGPU no `display_coex`. Porém:

- `intel_guc_load` é stub `NoFirmware`;
- detecção em `detect.rs` usa DID grosso e agrupa Arc + Iris Xe como um único
  `IntelXe`;
- o comentário de “GEN ISA sob NDA” está **incorreto** — PRMs Gen/Xe são
  públicos;
- Level Zero / OpenCL no alvo já foi rejeitado para Gen9 (IDEA #344) e continua
  incompatível com `no_std` em qualquer geração.

“Intel GPU” não é i915 vs xe como binários Linux. O modelo correto é **famílias
por graphics IP** (e display IP separado), com GuC submission no caminho
moderno e ring/execlists só até Gen9–11 / Xe-LP early.

Política preservada: **iGPU = vídeo/display**, **dGPU = AI** (NVIDIA via
ADR-0048 primeiro no lab; Arc/BMG quando for a dGPU compute). Sem inventar
PRIME, dma-buf, P2P ou IOMMU.

## 2. Decisão

Adotar arquitetura Intel multigeração espelhando ADR-0048/0049:

1. contrato de compute comum;
2. backends por família de IP / GMD_ID;
3. seleção runtime após probe;
4. `INTEL_KERNEL_PACK` offline (ocloc/IGC → zebin flatten);
5. falha não fatal e fallback CPU obrigatório.

```text
PCI VID 0x8086 + class display/compute
 → map BARs / GGTT
 → DID table (pré-GMD) OU GMD_ID MMIO 0xd8c (MTL+)
 → select backend
 ├── IntelGen9Ring     IP 9.x (/11.x)     — ring/ELSP + GPGPU_WALKER
 ├── IntelXeLpGuC      IP 12.00–12.10     — GuC + GPGPU @ RCS
 ├── IntelXeHpgCcs     IP 12.55 Arc       — GuC + COMPUTE_WALKER @ CCS
 ├── IntelXeLpg        IP 12.70–12.74     — GuC + dual-queue WA
 ├── IntelXe2          IP 20.x BMG/LNL    — GuC xe-native + CCS
 └── IntelXe3Track     IP 30.x PTL        — experimental (Xe2++ até PRM)
 → INTEL_KERNEL_PACK → batch/walker → seqno fence
 Falha / FW ausente / QEMU → CPU
```

## 3. Contrato comum

```rust
pub trait IntelComputeBackend {
    fn capabilities(&self) -> &IntelCapabilities;
    fn initialize(&mut self) -> Result<(), IntelError>;
    fn upload_program(&mut self, image: &IntelKernelImage) -> Result<ProgramId, IntelError>;
    fn upload_buffer(&mut self, bytes: &[u8]) -> Result<GpuBuffer, IntelError>;
    fn dispatch(&mut self, job: &ComputeJob) -> Result<Fence, IntelError>;
    fn wait(&mut self, fence: Fence, timeout_ticks: u64) -> Result<(), IntelError>;
    fn quarantine(&mut self, reason: IntelError);
}
```

Cortex não conhece GuC CTB, LRC, `GPGPU_WALKER` nem zebin.

### 3.1 Capacidades

- graphics IP (`ver = arch*100 + release` via GMD_ID quando presente);
- display IP separado (não misturar com compute);
- submission (`Ring` | `Execlists` | `GuC`);
- walker family (`GpgpuWalker` | `ComputeWalker`);
- engines fused: RCS / CCS / BCS;
- memória: UMA carveout vs LMEM (Arc) + ReBAR detect (log only até política M1);
- features: `mad_int8` | `dp4a` | `dpas` | `xmx_int2`.

Seleção: **GMD_ID se presente**, senão DID. Nunca misturar offsets Gen9 com Xe2.

## 4. Bring-up e submission

### 4.1 Memória

```text
CPU batch → ringbuffer (GGTT) → [Ring | ELSP | GuC CTB+doorbell]
 → LRC / context
 → RCS / CCS / BCS
 → PPGTT (48-bit) para workloads
```

- **GGTT:** GuC, ring, LRC, firmware DMA; pin bias acima de **WOPCM**;
- **PPGTT:** buffers de pesos/ativações;
- doorbell GuC **≠** `RENDER_RING_TAIL` legado do `intel.rs` atual.

### 4.2 Firmware

| Blob | Papel | Quando |
|------|-------|--------|
| DMC | display / planes / power | iGPU vídeo |
| GuC | scheduling + submission | **obrigatório** Gen12+ |
| HuC | mídia | não bloqueia matmul |
| GSC | security / HuC (DG2+) | stack xe-like moderno |

Paths linux-firmware: `i915/*` legado vs `xe/*` (LNL/BMG/PTL). Blobs
inalterados; falha = quarantine + CPU.

### 4.3 Walkers e engines

| Engine | Uso |
|--------|-----|
| RCS | display-related + compute legado Gen9 |
| CCS | IA dedicada (Xe-HPG+) — preferido |
| BCS | blits UMA↔buffers / display path |

| Comando | IP |
|---------|-----|
| `MEDIA_OBJECT` / `GPGPU_WALKER` | Gen9–Xe-LP |
| `COMPUTE_WALKER` + `CFE_STATE` | Xe-HPG+ CCS |
| `PIPE_CONTROL` / `MI_*` | sync / fences |

WA pós-DG2: RCS+CCS de address spaces diferentes não rodam em paralelo livre —
GuC dual-queue + yield. Por isso **não** rodar IA pesada na iGPU se houver dGPU.

## 5. INTEL Kernel Pack

Host: ocloc/IGC; alvo: **zero** Level Zero / OpenCL / NEO.

```text
OpenCL C / SYCL / SPIR-V
 → ocloc AOT → IGC → zebin ELF (EM_INTELGT)
 → extrator (.text + .ze_info + notes de compat)
 → INTEL_KERNEL_PACK (assinado)
 → FAT32/NeuralFS
 → SURFACE_STATE + CURBE + WALKER + PIPE_CONTROL
```

### 5.1 Toolchains (ranking)

1. **ocloc + IGC → zebin → INTEL_KERNEL_PACK** (primário);
2. Mesa NIR/genxml — caminho aberto de diagnóstico/emit;
3. oneDNN / XeTLA / OpenVINO — **referência** de kernels, não runtime;
4. **Rejeitar:** patchtokens (EOL), fat L0/OpenCL no OS, ISPC como único path.

Metadados mínimos: `gfx_core` / product / stepping, SLM, args/BTI,
`walker_family`, `features`, golden_vector_id, hash+Ed25519.

Não embutir zebin completo com debug/SPIR-V no FAT de produção.

### 5.2 Perfis BitNet

| Família | Perfil |
|---------|--------|
| Gen9 HD 620 | `mad-int8` / unpack sob demanda — **sem** DPAS |
| Xe-LP | `dp4a` quando golden passar |
| Arc / Xe2 | `dpas-int8` / `xmx-int2` (ref. arXiv 2508.06753) |

UMA Gen9: centenas de MB úteis — AirLLM streaming (ADR-0046) obrigatório; não
descompactar W2→INT8 inteiro.

## 6. Coexistência display / compute

Mesmas invariantes da ADR-0049 §6:

1. DisplayOwner = iGPU Intel (DMC + planes + BCS) ou GOP;
2. ComputeOwner = dGPU (NVIDIA preferida no lab; senão Arc/BMG);
3. falha AI nunca reseta a iGPU de display;
4. solo-GPU: time-share XQueue; frame display tem prioridade;
5. transferências AI→UI via host bounce;
6. ReBAR/IOMMU/PASID/P2P = escada futura (detect/log primeiro); SASOS “VRAM no
   mesmo AS” permanece aspiracional até isolamento DMA real.

## 7. Integração com Cortex

`TensorOp` → work queue → variante Intel do pack → fence seqno → amostragem CPU.
O n-gram speculative decoding continua CPU/policy; não confundir com DPAS.

## 8. Segurança e tolerância a falhas

- assinar pack; rejeitar stepping/ISA mismatch (notes zebin);
- relocs/`R_SEND`/BTI só com offsets validados;
- hang / MMU fault → quarantine;
- canário `vector_add` → MAD/INT8 → W2A8;
- QEMU/VirtIO sem Gen → `CPU_FALLBACK`;
- erro Intel não é fatal ao boot.

## 9. Metas e critérios de aceite

### P0 — Detecção

- [ ] GMD_ID + famílias no `detect` / capabilities;
- [ ] separar display IP de graphics IP;
- [ ] publicar engines CCS fused honestamente.

### P1 — Kernel pack host

- [ ] pack Gen9 (`GpgpuWalker` + `mad-int8`) gerado e assinado;
- [ ] rejeitar patchtokens e L0 no alvo.

### P2 — iGPU display + GuC mínimo

- [ ] DMC (display path) estável;
- [ ] GuC load real onde obrigatório (Gen12+); Gen9 pode permanecer ring;
- [ ] GGTT pin bias WOPCM.

### P3 — Primeiro compute Gen9

- [ ] `GPGPU_WALKER` / MEDIA path produz golden vector no HD 620;
- [ ] fault → CPU; FB intacto.

### P4 — BitNet Gen9

- [ ] MAD/INT8 bit-exato vs CPU;
- [ ] benchmark honesto sob UMA (pode perder para AVX2 — registrar).

### P5 — Arc/CCS (opcional lab)

- [ ] `COMPUTE_WALKER` @ CCS + GuC;
- [ ] perfil DPAS quando HW presente;
- [ ] nenhuma estrutura Gen9 no path Xe-HPG.

### P6 — Xe2/Xe3Track

- [ ] GMD_ID runtime para IP 20+;
- [ ] Xe3Track só experimental até PRM/bspec aberto.

## 10. Consequências

### Positivas

- Intel deixa de ser só blitter stub e ganha rota por IP;
- Gen9 do lab valida walker sem exigir Arc;
- Kernel Pack alinha NVIDIA/AMD/Intel;
- política iGPU vídeo + dGPU IA reforçada (dual-queue WA documentada).

### Negativas

- várias famílias de submission/walker;
- GuC+GSC aumentam TCB de firmware;
- Gen9 sem XMX limita aceleração BitNet.

### Riscos aceitos

- encoding exato de SURFACE_STATE/heaps exige engenharia de driver;
- Xe3 sem PRM público completo;
- UMA contend com FB — IA pesada na iGPU é último recurso.

## 11. Alternativas rejeitadas

1. **Level Zero / OpenCL no bare-metal** — std/ELF; IDEA #344 e requisito `no_std`.
2. **Um único backend “IntelXe”** — offsets e walkers divergem.
3. **patchtokens** — removidos do compute-runtime.
4. **IA na iGPU com dGPU presente** — conflita com display e dual-queue WA.
5. **Declarar GuC OK por presença de blob** — load ≠ submission pronta.
6. **Copiar DRM i915/xe** — copiar contratos (CTB/LRC/GMD_ID), não o driver.

## 12. Gaps vs código atual

| Item | Hoje | Esta ADR |
|------|------|----------|
| Detect | DID grosso | GMD_ID + famílias |
| GuC | stub | load + CTB |
| Walker | MEDIA stub | GPGPU / COMPUTE |
| CCS | ❌ | Xe-HPG+ |
| ISA “NDA” comment | incorreto | PRM público |
| Política display/compute | esqueleto ✅ | invariantes formais |

## 13. Fontes

- IGC / zebin spec: <https://github.com/intel/intel-graphics-compiler>
- compute-runtime / ocloc: <https://github.com/intel/compute-runtime>
- Mesa Intel genxml: <https://gitlab.freedesktop.org/mesa/mesa/-/tree/main/src/intel/genxml>
- Linux i915 + xe: <https://docs.kernel.org/gpu/>
- linux-firmware i915/xe: <https://gitlab.com/kernel-firmware/linux-firmware>
- Xe-HPG / XMX docs: <https://www.intel.com/content/www/us/en/developer/articles/technical/introduction-to-the-xe-hpg-architecture.html>
- AI-PC Xe2 int2×int8: <https://arxiv.org/abs/2508.06753>
- BitNet: <https://arxiv.org/abs/2402.17764>
- XeTLA (referência, archived): <https://github.com/intel/xetla>
- oneDNN: <https://github.com/uxlfoundation/oneDNN>
