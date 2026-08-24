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
pub mod intel_display;
pub mod nvidia;
pub mod nvidia_pascal;
pub mod nvidia_pascal_acr;
pub mod nvidia_pascal_ce;
pub mod nvidia_pascal_qmd;
pub mod nvidia_pascal_sw;
pub mod amd;
pub mod amd_discovery;
pub mod amd_psp;
pub mod amd_kiq;
pub mod amd_mes;
pub mod amd_mad;
pub mod backend;
pub mod blit;
pub mod compute_dispatch;
// cube FE (DoubleBuffer) permanece em jarbas — não MMIO
#[allow(dead_code)] pub mod ring;
pub mod firmware;
#[allow(dead_code)] pub mod xqueue;
#[allow(dead_code)] pub mod kv_dma;
#[allow(dead_code)] pub mod direct_storage;
#[allow(dead_code)] pub mod xpu;
#[allow(dead_code)] pub mod msched;
#[allow(dead_code)] pub mod display_coex;
#[allow(dead_code)] pub mod bench;
pub mod work_queue;
#[allow(dead_code)] pub mod sasos;
#[allow(dead_code)] pub mod pipeline_g5;
pub mod pcie_bypass;

// Seam ADR-0087 Fase 4b — Copy Engine (MHI tier1→tier0). mhi.rs (Fase 5) chama.
pub use nvidia_pascal_ce::{ce_ready, mhi_tier0_copy};

