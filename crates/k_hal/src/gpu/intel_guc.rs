//! Degrau Intel GuC + DMC presence (ADR-0050 P2).
//!
//! Gen9: GuC opcional → `SkippedGen9`; DMC presence logada para display.
//! Gen12+/Arc: blob GuC pinado GGTT acima WOPCM; status MMIO honesto.
//! Blob presente ≠ CTB/doorbell ≠ has_compute.

use crate::gpu::detect::{GpuArch, GpuInfo};
use crate::gpu::firmware;
use crate::gpu::intel_gtt::GgttPin;
use alloc::vec::Vec;
use k_nano::dma::dma_alloc_coalesced;
use spin::Mutex;

const GUC_STATUS: u64 = 0xC000;
const GUC_WOPCM_SIZE: u64 = 0xC050;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GucStage {
    SkippedGen9,
    BlobsMissing,
    Uploaded,
    BootTimeout,
    /// Status MMIO avançou — ainda ≠ compute Ready.
    Booted,
    Failed,
}

#[derive(Debug, Clone, Copy)]
pub struct GucReport {
    pub stage: GucStage,
    pub blob_bytes: u32,
    pub needs_guc: bool,
    pub dmc_present: bool,
    pub gtt_off: u64,
    pub wopcm_bytes: u64,
}

static LAST_GUC: Mutex<Option<GucReport>> = Mutex::new(None);

pub fn last_guc_report() -> Option<GucReport> {
    *LAST_GUC.lock()
}

fn guc_blob_for(gpu: &GpuInfo) -> Option<Vec<u8>> {
    let names: &[&str] = match gpu.arch {
        GpuArch::IntelGen9 => &["skl_guc_70.1.1.bin", "skl_guc_69.0.3.bin", "kbl_guc_70.1.1.bin"],
        GpuArch::IntelGen12 => &["kbl_guc_70.1.1.bin", "skl_guc_70.1.1.bin"],
        GpuArch::IntelXe if !gpu.is_integrated => {
            &["dg2_guc_70.4.1.bin", "dg2_guc_70.bin"]
        }
        GpuArch::IntelXe | GpuArch::IntelXe2 => {
            &["bmg_guc_70.bin", "lnl_guc_70.bin", "dg2_guc_70.bin"]
        }
        _ => &["skl_guc_70.1.1.bin"],
    };
    for n in names {
        if let Some(b) = firmware::load_firmware_file(n) {
            if !b.is_empty() {
                return Some(b);
            }
        }
    }
    None
}

fn dmc_present_for(gpu: &GpuInfo) -> bool {
    let names: &[&str] = match gpu.arch {
        GpuArch::IntelGen9 => &[
            "skl_dmc_ver1_27.bin",
            "kbl_dmc_ver1_04.bin",
        ],
        GpuArch::IntelXe if !gpu.is_integrated => &["dg2_dmc_ver2_08.bin"],
        _ => &["skl_dmc_ver1_27.bin", "kbl_dmc_ver1_04.bin", "dg2_dmc_ver2_08.bin"],
    };
    for n in names {
        if firmware::has_named_blob(n) || firmware::load_firmware_file(n).is_some() {
            return true;
        }
    }
    false
}

/// CCS fused (honesto): Gen9=false; Arc dGPU=true; Gen12/Xe-LP log-only assume false sem fuse MMIO.
pub fn probe_ccs_fused(gpu: &GpuInfo) -> bool {
    match gpu.arch {
        GpuArch::IntelGen9 => false,
        GpuArch::IntelXe if !gpu.is_integrated => true,
        GpuArch::IntelXe2 => true,
        GpuArch::IntelGen12 | GpuArch::IntelXe => {
            k_nano::slog_hal!("INTEL", "CCS", "{}: iGPU/Xe-LP — CCS fused probe residual (assume false)", gpu.name);
            false
        }
        _ => false,
    }
}

/// Bring-up GuC + DMC presence. Gen9 → skip GuC; Gen12+/Arc → pin GGTT + poll.
pub unsafe fn bring_up_guc(gpu: &GpuInfo, mmio: u64) -> GucReport {
    let dmc = dmc_present_for(gpu);
    k_nano::slog_hal!("INTEL", "DMC", "{}: present={} (display path; load residual se GOP OK)", gpu.name, dmc);
    let ccs = probe_ccs_fused(gpu);
    k_nano::slog_hal!("INTEL", "CCS", "{}: fused={}", gpu.name, ccs);

    let mut gtt = GgttPin::new(mmio);
    let wopcm = gtt.wopcm_bytes;

    let needs = !matches!(gpu.arch, GpuArch::IntelGen9);
    if !needs {
        let r = GucReport {
            stage: GucStage::SkippedGen9,
            blob_bytes: 0,
            needs_guc: false,
            dmc_present: dmc,
            gtt_off: 0,
            wopcm_bytes: wopcm,
        };
        k_nano::slog_hal!("INTEL", "GUC", "{}: SkippedGen9 (ring path) WOPCM={}B", gpu.name, wopcm);
        *LAST_GUC.lock() = Some(r);
        return r;
    }

    let Some(blob) = guc_blob_for(gpu) else {
        k_nano::slog_hal!("INTEL", "GUC", "{}: BlobsMissing (coloque i915/*_guc_*.bin no FAT)", gpu.name);
        let r = GucReport {
            stage: GucStage::BlobsMissing,
            blob_bytes: 0,
            needs_guc: true,
            dmc_present: dmc,
            gtt_off: 0,
            wopcm_bytes: wopcm,
        };
        *LAST_GUC.lock() = Some(r);
        return r;
    };
    let len = blob.len() as u32;

    let pages = ((blob.len() + 4095) / 4096).max(1) as u32;
    let Some(dma) = dma_alloc_coalesced((pages as usize) * 4096) else {
        let r = GucReport {
            stage: GucStage::Failed,
            blob_bytes: len,
            needs_guc: true,
            dmc_present: dmc,
            gtt_off: 0,
            wopcm_bytes: wopcm,
        };
        *LAST_GUC.lock() = Some(r);
        return r;
    };
    core::ptr::copy_nonoverlapping(blob.as_ptr(), dma.virt as *mut u8, blob.len());

    let gtt_off = match gtt.pin_sys(dma.phys, pages) {
        Some(off) => off,
        None => {
            let r = GucReport {
                stage: GucStage::Failed,
                blob_bytes: len,
                needs_guc: true,
                dmc_present: dmc,
                gtt_off: 0,
                wopcm_bytes: wopcm,
            };
            *LAST_GUC.lock() = Some(r);
            return r;
        }
    };

    let wopcm_reg = core::ptr::read_volatile((mmio + GUC_WOPCM_SIZE) as *const u32);
    k_nano::slog_hal!("INTEL", "GUC", "{}: Uploaded {}B dma={:#x} gtt_off={:#x} WOPCM_reg={:#x} (estrutural)", gpu.name, len, dma.phys, gtt_off, wopcm_reg);

    let before = core::ptr::read_volatile((mmio + GUC_STATUS) as *const u32);
    let mut booted = false;
    for _ in 0..100_000 {
        let st = core::ptr::read_volatile((mmio + GUC_STATUS) as *const u32);
        if st != 0 && st != 0xffff_ffff && st != before {
            booted = true;
            break;
        }
        core::hint::spin_loop();
    }
    let _keep = dma;

    let stage = if booted {
        k_nano::slog_hal!("INTEL", "GUC", "{}: Booted (status MMIO) — ≠ has_compute", gpu.name);
        GucStage::Booted
    } else {
        k_nano::slog_hal!("INTEL", "GUC", "{}: BootTimeout (esperado sem CTB/doorbell real)", gpu.name);
        GucStage::BootTimeout
    };
    let r = GucReport {
        stage,
        blob_bytes: len,
        needs_guc: true,
        dmc_present: dmc,
        gtt_off,
        wopcm_bytes: wopcm,
    };
    *LAST_GUC.lock() = Some(r);
    r
}
