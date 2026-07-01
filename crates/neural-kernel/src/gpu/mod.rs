//! GPU Module — detecção, VRAM tier, ring buffer, firmware, backend, cube.

pub mod detect;
pub mod vram;
pub mod intel;
pub mod nvidia;
pub mod amd;
pub mod backend;
pub mod cube;

/// Re-exporta a GPU acelerada para o kernel (usado quando conectado ao boot)
pub use backend::gpu_matmul;
pub use detect::{best_compute_gpu, best_display_gpu, detect_all, GpuInfo, GpuVendor};
pub use vram::{vram_alloc, vram_free};
