//! NVIDIA GSP-RM bring-up (Turing+/Ampere) — escada D0–D5 (ADR-0048 P5).
//!
//! Referências: Nouveau `nvkm/subdev/gsp/tu102.c` + r535; OpenRM `KernelGsp`;
//! Nova-core falcon/FWSEC. Blobs FAT: `GSP.BIN` / `GSPBOOT.BIN` / `GSPLOAD.BIN`
//! (mkfat32 `nvidia_gsp_fat_name` ← linux-firmware ga102/gsp pin 535).
//!
//! Honestidade: estágio tipado; `GpuGspBooted` só em `Alive+`; `GpuCompute`
//! só via canário PASS em `backend` (nunca por blob presente).

mod hs_hdr;
mod bring_up;

pub use bring_up::{bring_up_gsp, last_gsp_report, try_gsp_vector_add, GspReport, GspStage};
pub use hs_hdr::{parse_hs_bin, HsBinInfo};

/// Nomes FAT / preload (recipe `gpu.nvidia.ga107`).
pub const FAT_GSP: &str = "GSP.BIN";
pub const FAT_BOOTLOADER: &str = "GSPBOOT.BIN";
pub const FAT_BOOTER_LOAD: &str = "GSPLOAD.BIN";
pub const FAT_BOOTER_UNLOAD: &str = "GSPUNLD.BIN";

/// Logical OpenRM names (telemetria).
pub const LOG_GSP: &str = "gsp";
pub const LOG_BOOTLOADER: &str = "bootloader";
pub const LOG_BOOTER_LOAD: &str = "booter_load";
pub const LOG_BOOTER_UNLOAD: &str = "booter_unload";
