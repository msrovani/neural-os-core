//! GPU Module — detecção, VRAM tier, ring, firmware, KernelPack, canário, backend.

pub mod detect;
pub mod compute_abi;
pub mod kernel_pack;
pub mod canary;
pub mod vram;
pub mod intel;
pub mod intel_gtt;
pub mod intel_gen9;
pub mod intel_guc;
pub mod intel_arc;
pub mod intel_mad;
pub mod nvidia;
pub mod nvidia_pascal;
pub mod nvidia_pascal_acr;
pub mod nvidia_pascal_qmd;
pub mod nvidia_pascal_sw;
pub mod amd;
pub mod amd_discovery;
pub mod amd_psp;
pub mod amd_kiq;
pub mod amd_mes;
pub mod amd_mad;
pub mod backend;
pub mod compute_dispatch;
// cube FE (DoubleBuffer) permanece em jarbas — não MMIO
pub mod ring;
pub mod firmware;
pub mod xqueue;
pub mod kv_dma;
pub mod direct_storage;
pub mod xpu;
pub mod msched;
pub mod display_coex;
pub mod bench;
pub mod work_queue;
pub mod sasos;
pub mod pipeline_g5;

