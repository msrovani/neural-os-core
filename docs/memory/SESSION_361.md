# SESSION_361 — GPU Multi-ISA Wave0–5 + Falcon3 SKUs + dual-GPU AIOS

**Sprint:** v1.9.99-s361 TEST  
**Data:** 2026-09-19  
**Objetivo:** Fechar plano GPU multi-vendor (Waves 0–5): KernelImage loaders, packers W2A8, dual iGPU/dGPU, Falcon3 1B/3B/7B/10B AIOS, docs/Remember.

## Pós-wave — C1–C3 (stub→real)

- **C1:** `w2a8_device` (layout signed sem +128); stage upload + host GEMV; packers `.cu`/`.cl` W2A8; CE bw→Mad/Scalar `replan_after_bandwidth`; `tools/GPU_KERNEL_PACK.md`
- **C2:** ACR slog AWAITING_HW; `gpu_ternary` prepara buffers e retorna None+fallback CPU até metal
- **C3:** host sem nvcc/ocloc → `target/nkp-lab/NKP_*.BIN` stub honestos; ISA real = toolkit no host

## Entregue

### Wave 0
- `ComputeCaps` + `OpProfile` + `CompilerId` RustCuda/Nvptx
- `find_active_pack` multi-ISA + `NKP_W2A8_*`
- `falcon3_w2a8` + `Falcon3Kind` shapes HF
- `display_coex` dual + testes
- `aios_adapt` Observe→Plan→Act→Verify→Remember

### Wave 1
- `kernel_image.rs`, `blob_cubin.rs`, `blob_hsaco.rs`, `blob_zebin.rs`
- `QmdLaunch::from_kernel_image` + **consumidores wired:**
  - NVIDIA `dispatch_vector_add` → KernelImage → QMD
  - Intel Gen9 walker VFE threads from `img.regs`
  - AMD KIQ `DISPATCH_DIRECT` dim_x from image; MES log image
- `from_blob` / stub detect `CPU_W2A8_STUB`

### Wave 2
- `pack_nvidia_kernels.py --op w2a8` + `--source {cu,rust}` + SMs 52/61/70/75/80/89
- `pack_amd_kernels.py` / `pack_intel_kernels.py --op w2a8` (stub até Degrau/Gen9 silício)
- `gpu_kernels::bitlinear_w2a8_ref` (golden real, não placeholder)
- mkfat32 embute W2A8 packs

### Wave 3–4
- Ready invariante: pack+golden; dual compute ISA
- `gpu_ternary`: pack+image+shape+profile (Dp4a/Mad/Dot); device AWAITING_HW → None
- `remember_caps` slog

### Wave 5
- ADR-0105 + INDEX + IDEA #550 + STATE + TECNOLOGIAS + SESSION_INDEX
- Host tests: cubin/hsaco/zebin/aios/display_coex/falcon3/qmd+from_image/gpu_kernels W2A8 (16+3 PASS)
- `cargo check --release` k-hal + neural-kernel OK (`target/agent-gpu-mv`)
- Packers AMD/Intel parity `--op w2a8`

## Honesty

- Sem CUBIN/EU real no lab → stub packs → CpuOnly (não Ready).
- W2A8 device dispatch = Layer S residual.
- AMD golden silício aberto.

## Próximo

Golden silício GTX 1050 + HD Gen9; QMD despacho com KernelImage code pages.
