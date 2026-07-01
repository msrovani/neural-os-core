//! GPU Module — deteccao, VRAM tier, ring buffer, firmware.
//! Toda GPU detectada via PCI tem sua VRAM mapeada como AllocTier::Vram.
//! Display via iGPU (Intel/AMD) ou engine propria da GPU unica.

pub mod detect;
pub mod vram;
pub mod intel;
pub mod nvidia;
pub mod amd;
