# crates/k_hal/src/gpu/ — GPU Backends (R1)

**Responsibility**: MMIO GPU stack — PCI detection (`detect::detect_all` → `GpuInfo`
with vendor/arch/ISA tag/backend kind), display/compute coexistence planning
(`display_coex::plan_assignment`), BAR UC mapping, firmware secure-boot (NVIDIA ACR /
AMD PSP / Intel GuC), vendor backend probes (`IntelRing`+`BcsRing`, `NvidiaGpu`,
`AmdGpu`), and the vector_add canary that gates `has_compute`/`BackendState::Ready`
(`canary::`, `compute_abi::`).

**Key symbols**: `backend::{init_backend, init_backend_with_plan, gpu_matmul,
adr0047_compute_gate, compute_state}`; `detect::{GpuVendor, GpuArch, GpuInfo,
best_compute_gpu, best_display_gpu}`; `compute_abi::{ComputeCaps, ComputeJob,
BackendState, TensorOp, IsaTag, GoldenId}`; `firmware::secure_boot_gpu`;
`ring::GpuJobRing`; `blit::{blit_2d, run_blit_canary}`; `vram::{vram_alloc, vram_free}`;
`kernel_pack` (NKP1 signed blob envelope); `intel::{IntelRing, BcsRing}`,
`intel_guc`, `intel_gtt::GgttPin`, `nvidia::NvidiaGpu`, `nvidia_pascal`/`_acr`/`_qmd`/`_sw`,
`nvidia_pascal_ce` (CE → `mhi_tier0_copy`, hook só canário golden); `amd::{AmdGpu}`,
`amd_kiq`, `amd_mes`, `amd_psp`, `amd_discovery::AmdIpId`;
`work_queue`, `xqueue`, `kv_dma`, `msched`, `sasos`, `xpu`, `pipeline_g5`.

**Honesty (SESSION_274):** `gpu_matmul` / `nvidia_matmul` return `None` until
KernelPack W2A8 (Layer S). Canary Ready = BAR/VRAM bring-up, not device math.
`work_queue::drain` counts GPU only when the vendor path actually returns Some.

**Integration**: driven from bin via `k_hal::gpu::backend` init after platform sync;
compute consumers route through `cortex::tensor::Tensor` + `work_queue` gated by
`adr0047_compute_gate`; jarbas consumes `gpu::*` (`pub use k_hal::gpu::*`) for VRAM
gauges and VGA plane control; `compute_port::sync_from_backend` reflects backend state
to the HalOffer port. Display scanout FE (compositor) stays in jarbas — this module
only owns MMIO.
