# SESSION_138 — GPU Multivendor Unlock (ADR-0048/49/50) fundação

**Data:** 2026-07-17  
**Foco:** Desbloquear compute GPU bare-metal — Fase A–D scaffold (honestidade de caps + KernelPack + bring-up stubs).

## Feito

### Fase A — Fundação
- `compute_abi.rs`: `TensorOp`, `ComputeJob`, `GpuBufferHandle`, `FenceId`, `ComputeCaps`, `IsaTag`, golden `vector_add_cpu`
- `detect.rs`: `has_compute=false` até canário; `compute_candidate` + `backend_kind`; DIDs lab (1050/2060/4070S, HD620/630, 5600G/Raphael/Phoenix); AMD APU `is_integrated`
- `display_coex` dirige `init_backend_with_plan` (main.rs)
- Hardening: ACR `bar2+pmoff`; ACR só Pascal; AMD doorbell noop; gate ADR-0047 só `BackendState::Ready`; VRAM offset relativo no DMA NVIDIA

### Fase B — KernelPack + tools
- `kernel_pack.rs` envelope NKP1 + FNV1a64 + `verify_trusted`
- `tools/gpu_kernels/` (workspace exclude) — CPU golden tests OK
- `tools/pack_{nvidia,amd,intel}_kernels.py` — stubs sem toolkit; CUBIN/HSACO/zebin quando nvcc/clang/ocloc presentes
- `download_firmware.py` — subset `amdgpu/`

### Fase C — Bring-up (scaffold; canário HW aberto)
- NVIDIA: `try_vector_add_legacy` / `try_vector_add_gsp`
- Intel: `try_vector_add_gen9` / `try_vector_add_arc` + opcodes walker
- AMD: `AmdIpId` hint + `try_vector_add`
- `canary.rs`: pack signed obrigatório; FailDispatch → Quarantine/CPU (display intacto)

### Fase D
- `work_queue::submit_tensor(TensorOp)` ligado em `gpu_matmul`
- INDEX 0048–0050 → `fazendo`; TECNOLOGIAS 3.1/3.7–3.10; STATE

## Verificação
- `cargo check -p jarbas --release` → 0 errors (`target/check-gpu-a`)
- `cargo check -p neural-kernel --release` → 0 errors (`target/check-gpu-nk`)
- `cargo test --manifest-path tools/gpu_kernels/Cargo.toml` → 2 passed

## Não alegado / próximo silicon
- QMD / PASCAL_COMPUTE_B / GSP-RM reais
- GPGPU_WALKER / COMPUTE_WALKER golden no HW
- PSP→SDMA→doorbell por gen
- Assinatura Ed25519 host nos NKP (hoje hash ok, sig zeros = unsigned deny ativo)
- BitLinearW2A8 device (só stub CPU em `gpu_kernels`)

## Anti-padrões evitados
- Sem Vulkan/wgpu/cudarc no bin
- Sem `has_compute=true` mentiroso no boot
- Sem doorbell AMD genérico 0x1B0
- Sem ACR em Turing+/Ada
