//! Degrau PSP AMD — presença SOS/TA/ASD + upload estrutural (ADR-0049 P2).
//! Blob presente ≠ autenticado ≠ engine pronto ≠ has_compute.

use crate::gpu::detect::{GpuArch, GpuInfo};
use crate::gpu::firmware;
use alloc::vec::Vec;
use k_nano::dma::dma_alloc_coalesced;
use spin::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PspStage {
    BlobsMissing,
    Uploaded,
    AuthTimeout,
    /// Status MMIO mudou — ainda ≠ Ready.
    Authed,
    Failed,
}

#[derive(Debug, Clone, Copy)]
pub struct PspReport {
    pub stage: PspStage,
    pub asd_bytes: u32,
    pub ta_bytes: u32,
    pub family: &'static str,
}

static LAST_PSP: Mutex<Option<PspReport>> = Mutex::new(None);

pub fn last_psp_report() -> Option<PspReport> {
    *LAST_PSP.lock()
}

fn family_blobs(gpu: &GpuInfo) -> (&'static str, &'static [&'static str], &'static [&'static str]) {
    match gpu.arch {
        GpuArch::AmdGcn | GpuArch::AmdRdna1 => (
            "green_sardine",
            &["green_sardine_asd.bin", "psp_13_0_5_asd.bin"],
            &["green_sardine_ta.bin", "psp_13_0_5_ta.bin"],
        ),
        GpuArch::AmdRdna2 => (
            "gc_10_3 / psp_13",
            &["psp_13_0_5_asd.bin", "green_sardine_asd.bin"],
            &["psp_13_0_5_ta.bin", "green_sardine_ta.bin"],
        ),
        GpuArch::AmdRdna3 | GpuArch::AmdRdna4 => (
            "gc_11 / psp",
            &["psp_13_0_5_asd.bin"],
            &["psp_13_0_5_ta.bin"],
        ),
        _ => (
            "generic",
            &["psp_13_0_5_asd.bin", "green_sardine_asd.bin"],
            &["psp_13_0_5_ta.bin", "green_sardine_ta.bin"],
        ),
    }
}

fn load_first(names: &[&str]) -> Option<Vec<u8>> {
    for n in names {
        if let Some(b) = firmware::load_firmware_file(n) {
            if !b.is_empty() {
                return Some(b);
            }
        }
    }
    None
}

/// SDMA / MEC presence (log only) — GMC ring residual.
pub fn log_runtime_fw_presence(gpu: &GpuInfo) {
    let sdma = [
        "sdma_5_2_6.bin",
        "green_sardine_sdma.bin",
    ];
    let mec = match gpu.arch {
        GpuArch::AmdRdna3 | GpuArch::AmdRdna4 => {
            &["gc_11_5_0_mec.bin", "gc_11_5_0_mes_2.bin", "gc_11_5_0_mes1.bin"][..]
        }
        GpuArch::AmdRdna2 => &["gc_10_3_6_mec.bin", "gc_10_3_6_mec2.bin"][..],
        _ => &["green_sardine_mec.bin", "green_sardine_mec2.bin"][..],
    };
    let mut sdma_ok = false;
    for n in &sdma {
        if firmware::has_named_blob(n) || firmware::load_firmware_file(n).is_some() {
            sdma_ok = true;
            break;
        }
    }
    let mut mec_ok = false;
    for n in mec {
        if firmware::has_named_blob(n) || firmware::load_firmware_file(n).is_some() {
            mec_ok = true;
            break;
        }
    }
    k_nano::slog_hal!("AMD", "FW", "{}: SDMA={} MEC/MES={} (presente ≠ engine pronto)", gpu.name, sdma_ok, mec_ok);
}

/// Bring-up PSP Degrau: carrega ASD+TA para DMA; não finge SOS mailbox OK.
pub unsafe fn bring_up_psp(gpu: &GpuInfo, mmio: u64) -> PspReport {
    let (family, asd_names, ta_names) = family_blobs(gpu);
    log_runtime_fw_presence(gpu);

    let asd = load_first(asd_names);
    let ta = load_first(ta_names);
    let (Some(asd), Some(ta)) = (asd, ta) else {
        k_nano::slog_hal!("AMD", "PSP", "{}: BlobsMissing family={} (amdgpu/*_asd/_ta no FAT)", gpu.name, family);
        let r = PspReport {
            stage: PspStage::BlobsMissing,
            asd_bytes: 0,
            ta_bytes: 0,
            family,
        };
        *LAST_PSP.lock() = Some(r);
        return r;
    };

    let asd_len = asd.len() as u32;
    let ta_len = ta.len() as u32;
    let pages = (((asd.len() + ta.len()) + 4095) / 4096).max(1);
    let Some(dma) = dma_alloc_coalesced(pages * 4096) else {
        let r = PspReport {
            stage: PspStage::Failed,
            asd_bytes: asd_len,
            ta_bytes: ta_len,
            family,
        };
        *LAST_PSP.lock() = Some(r);
        return r;
    };
    core::ptr::copy_nonoverlapping(asd.as_ptr(), dma.virt as *mut u8, asd.len());
    core::ptr::copy_nonoverlapping(ta.as_ptr(), (dma.virt as *mut u8).add(asd.len()), ta.len());

    // Poll conservador em MMIO baixo — só Authed se valor mudar de 0/ffffffff.
    let probe_off = 0x0u64; // não usar offset cross-gen como “PSP OK”
    let before = core::ptr::read_volatile((mmio + probe_off) as *const u32);
    k_nano::slog_hal!("AMD", "PSP", "{}: Uploaded ASD={}B TA={}B dma={:#x} family={} (estrutural)", gpu.name, asd_len, ta_len, dma.phys, family);

    let mut authed = false;
    let _ = (before, &mut authed);
    let _keep = dma;

    // Sem mailbox PSP real → AuthTimeout honesto.
    let stage = PspStage::AuthTimeout;
    k_nano::slog_hal!("AMD", "PSP", "{}: AuthTimeout (mailbox/TMR residual) — ≠ has_compute", gpu.name);
    let r = PspReport {
        stage,
        asd_bytes: asd_len,
        ta_bytes: ta_len,
        family,
    };
    *LAST_PSP.lock() = Some(r);
    r
}
