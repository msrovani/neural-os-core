//! GPU Module — detecção, VRAM tier, ring buffer, firmware, backend, cube.

pub mod detect;
pub mod vram;
pub mod intel;
pub mod nvidia;
pub mod amd;
pub mod backend;
pub mod cube;
pub mod ring;
pub mod firmware;
pub mod xqueue;
pub mod kv_dma;
pub mod xpu;

