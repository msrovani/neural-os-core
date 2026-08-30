//! GPU Secure Boot — firmware loading para NVIDIA ACR, AMD PSP, Intel GuC.
//!
//! NVIDIA Pascal: Degrau ACR em `nvidia_pascal_acr` (WPR/LSB/HS). Stub linear
//! antigo removido — não alegar VRAM desbloqueada sem HsBooted.
//!
//! # Fonte dos firmwares
//! GitLab mirror: https://gitlab.com/kernel-firmware/linux-firmware.git
//!
//! # Como incluir no boot
//!   `python tools/download_firmware.py` → target/firmware/
//!   Ou: firmware/*.bin via mkfat32.py (FECS_*.BIN / ACR_BL.BIN / …).

use crate::gpu::detect::{GpuInfo, GpuVendor};
use crate::gpu::nvidia_pascal_acr::{self, AcrReport, AcrStage};
use crate::gpu::nvidia_pascal_sw::{self, SwReport};
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

static FW_PRELOAD: Mutex<BTreeMap<String, Vec<u8>>> = Mutex::new(BTreeMap::new());
static LAST_ACR: Mutex<Option<AcrReport>> = Mutex::new(None);
static LAST_SW: Mutex<Option<SwReport>> = Mutex::new(None);

/// Injeta blob (ex.: lido via USB-MSC no bin) antes do ACR — chave = nome lógico (`fecs_bl.bin`).
pub fn preload_blob(logical_name: &str, data: Vec<u8>) {
    if data.is_empty() {
        return;
    }
    k_nano::slog_hal!("FW", "preload", "{} ({}B)", logical_name, data.len());
    FW_PRELOAD.lock().insert(String::from(logical_name), data);
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SecureBootResult {
    /// HS concluiu (WPR status avançou) — **não** = compute Ready.
    Ok,
    NoFirmware,
    SignatureFail,
    InitFail,
}

/// Último report ACR (Pascal) — consumido por probe/sw.
pub fn last_acr_report() -> Option<AcrReport> {
    *LAST_ACR.lock()
}

pub fn last_sw_report() -> Option<SwReport> {
    *LAST_SW.lock()
}

/// Carrega blob de firmware (preload → FAT 8.3 → VFS).
pub fn load_firmware_file(name: &str) -> Option<Vec<u8>> {
    if let Some(data) = FW_PRELOAD.lock().get(name) {
        if !data.is_empty() {
            return Some(data.clone());
        }
    }
    // Aliases 8.3 gravados no FAT pela mkfat32 (encode_83).
    let short: &[&str] = match name {
        "fecs_bl.bin" => &["FECS_BL.BIN", "FW_FECS_BL_BIN"],
        "fecs_data.bin" => &["FECS_DAT.BIN", "FW_FECS_DATA_BIN"],
        "fecs_inst.bin" => &["FECS_INS.BIN", "FW_FECS_INST_BIN"],
        "fecs_sig.bin" => &["FECS_SIG.BIN", "FW_FECS_SIG_BIN"],
        "gpccs_bl.bin" => &["GPCCS_BL.BIN", "FW_GPCCS_BL_BIN"],
        "gpccs_data.bin" => &["GPCCS_DA.BIN", "FW_GPCCS_DATA_BIN"],
        "gpccs_inst.bin" => &["GPCCS_IN.BIN", "FW_GPCCS_INST_BIN"],
        "gpccs_sig.bin" => &["GPCCS_SI.BIN", "FW_GPCCS_SIG_BIN"],
        "sw_ctx.bin" => &["SW_CTX.BIN", "FW_SW_CTX_BIN", "FW_GP108_SW_CTX_BIN"],
        "sw_bundle_init.bin" => &["SW_BNDL.BIN", "FW_SW_BUNDLE_INIT_BIN"],
        "sw_method_init.bin" => &["SW_MTHD.BIN", "FW_SW_METHOD_INIT_BIN"],
        "sw_nonctx.bin" => &["SW_NONC.BIN", "FW_SW_NONCTX_BIN"],
        "bl.bin" => &["ACR_BL.BIN", "FW_GP108_BL_BIN"],
        "ucode_load.bin" => &["ACRLOAD.BIN", "FW_GP108_UCODE_LOAD_BIN"],
        "ucode_unload.bin" => &["ACRUNLD.BIN", "FW_GP108_UCODE_UNLOAD_BIN"],
        "unload_bl.bin" => &["ACR_UBL.BIN", "FW_GP108_UNLOAD_BL_BIN"],
        // Intel GuC (FAT 8.3 / FW_* aliases)
        "skl_guc_70.1.1.bin" => &["SKLGUC70.BIN", "FW_SKL_GUC_70_1_1_BIN"],
        "skl_guc_69.0.3.bin" => &["SKLGUC69.BIN", "FW_SKL_GUC_69_0_3_BIN"],
        "kbl_guc_70.1.1.bin" => &["KBLGUC70.BIN", "FW_KBL_GUC_70_1_1_BIN"],
        "dg2_guc_70.bin" => &["DG2GUC70.BIN", "FW_DG2_GUC_70_BIN"],
        "dg2_guc_70.4.1.bin" => &["DG2GUC704.BIN", "FW_DG2_GUC_70_4_1_BIN"],
        "bmg_guc_70.bin" => &["BMGGUC70.BIN", "FW_BMG_GUC_70_BIN"],
        "lnl_guc_70.bin" => &["LNLGUC70.BIN", "FW_LNL_GUC_70_BIN"],
        // Intel DMC (display)
        "skl_dmc_ver1_27.bin" => &["SKLDMC27.BIN", "FW_SKL_DMC_VER1_27_BIN"],
        "kbl_dmc_ver1_04.bin" => &["KBLDMC04.BIN", "FW_KBL_DMC_VER1_04_BIN"],
        "dg2_dmc_ver2_08.bin" => &["DG2DMC08.BIN", "FW_DG2_DMC_VER2_08_BIN"],
        "skl_huc_2.0.0.bin" => &["SKLHUC20.BIN", "FW_SKL_HUC_2_0_0_BIN"],
        "kbl_huc_4.0.0.bin" => &["KBLHUC40.BIN", "FW_KBL_HUC_4_0_0_BIN"],
        "dg2_huc_gsc.bin" => &["DG2HUGSC.BIN", "FW_DG2_HUC_GSC_BIN"],
        "ptl_guc_70.bin" => &["PTLGUC70.BIN", "FW_PTL_GUC_70_BIN"],
        "bmg_huc.bin" => &["BMGHUC70.BIN", "FW_BMG_HUC_BIN"],
        "lnl_huc.bin" => &["LNLHUC70.BIN", "FW_LNL_HUC_BIN"],
        // AMD PSP / SDMA / MEC (FAT 8.3 / FW_*)
        "green_sardine_asd.bin" => &["GS_ASD.BIN", "FW_GREEN_SARDINE_ASD_BIN"],
        "green_sardine_ta.bin" => &["GS_TA.BIN", "FW_GREEN_SARDINE_TA_BIN"],
        "green_sardine_sdma.bin" => &["GS_SDMA.BIN", "FW_GREEN_SARDINE_SDMA_BIN"],
        "green_sardine_mec.bin" => &["GS_MEC.BIN", "FW_GREEN_SARDINE_MEC_BIN"],
        "green_sardine_mec2.bin" => &["GS_MEC2.BIN", "FW_GREEN_SARDINE_MEC2_BIN"],
        "green_sardine_ce.bin" => &["GS_CE.BIN", "FW_GREEN_SARDINE_CE_BIN"],
        "green_sardine_dmcub.bin" => &["GS_DMCB.BIN", "FW_GREEN_SARDINE_DMCUB_BIN"],
        "green_sardine_me.bin" => &["GS_ME.BIN", "FW_GREEN_SARDINE_ME_BIN"],
        "green_sardine_pfp.bin" => &["GS_PFP.BIN", "FW_GREEN_SARDINE_PFP_BIN"],
        "green_sardine_rlc.bin" => &["GS_RLC.BIN", "FW_GREEN_SARDINE_RLC_BIN"],
        "green_sardine_vcn.bin" => &["GS_VCN.BIN", "FW_GREEN_SARDINE_VCN_BIN"],
        "psp_13_0_5_asd.bin" => &["PSPASD.BIN", "FW_PSP_13_0_5_ASD_BIN"],
        "psp_13_0_5_ta.bin" => &["PSPTA.BIN", "FW_PSP_13_0_5_TA_BIN"],
        "psp_13_0_5_toc.bin" => &["PSPTOC.BIN", "FW_PSP_13_0_5_TOC_BIN"],
        "sdma_5_2_6.bin" => &["SDMA526.BIN", "FW_SDMA_5_2_6_BIN"],
        "gc_10_3_6_mec.bin" => &["GC103MEC.BIN", "FW_GC_10_3_6_MEC_BIN"],
        "gc_10_3_6_mec2.bin" => &["GC103ME2.BIN", "FW_GC_10_3_6_MEC2_BIN"],
        "gc_10_3_6_ce.bin" => &["GC103CE.BIN", "FW_GC_10_3_6_CE_BIN"],
        "gc_10_3_6_me.bin" => &["GC103ME.BIN", "FW_GC_10_3_6_ME_BIN"],
        "gc_10_3_6_pfp.bin" => &["GC103PFP.BIN", "FW_GC_10_3_6_PFP_BIN"],
        "gc_10_3_6_rlc.bin" => &["GC103RLC.BIN", "FW_GC_10_3_6_RLC_BIN"],
        "gc_11_5_0_mec.bin" => &["GC115MEC.BIN", "FW_GC_11_5_0_MEC_BIN"],
        "gc_11_5_0_mes_2.bin" => &["GC115MS2.BIN", "FW_GC_11_5_0_MES_2_BIN"],
        "gc_11_5_0_mes1.bin" => &["GC115MS1.BIN", "FW_GC_11_5_0_MES1_BIN"],
        "gc_11_5_0_imu.bin" => &["GC115IMU.BIN", "FW_GC_11_5_0_IMU_BIN"],
        "gc_11_5_0_me.bin" => &["GC115ME.BIN", "FW_GC_11_5_0_ME_BIN"],
        "gc_11_5_0_pfp.bin" => &["GC115PFP.BIN", "FW_GC_11_5_0_PFP_BIN"],
        "gc_11_5_0_rlc.bin" => &["GC115RLC.BIN", "FW_GC_11_5_0_RLC_BIN"],
        _ => &[],
    };

    for alias in short {
        if let Some(data) = read_fat32_root(alias) {
            if !data.is_empty() {
                return Some(data);
            }
        }
    }

    // Legado: FW_<NAME> / FW_GP108_<NAME> (FAT only — k-hal não depende de hermes VFS)
    let fname = alloc::format!("FW_{}", name.to_uppercase().replace(".", "_"));
    if let Some(data) = read_fat32_root(&fname) {
        if !data.is_empty() {
            return Some(data);
        }
    }
    let fname_gr = alloc::format!("FW_GP108_{}", name.to_uppercase().replace(".", "_"));
    if let Some(data) = read_fat32_root(&fname_gr) {
        if !data.is_empty() {
            return Some(data);
        }
    }
    None
}

/// Leitura direta da raiz FAT32 (ATA) — caminho HW real (USB/AHCI com MBR 0x0C).
fn read_fat32_root(name: &str) -> Option<Vec<u8>> {
    unsafe {
        let ata = k_nano::ATA_DRIVER.lock();
        let ata = ata.as_ref()?;
        let parts = k_nano::fat32::read_mbr(ata);
        for p in &parts {
            if !matches!(p.type_code, 0x0B | 0x0C | 0x1C | 0x73) {
                continue;
            }
            if let Some(fs) = k_nano::fat32::Fat32Reader::new(ata, p) {
                if let Some(data) = fs.read_file(name) {
                    return Some(data);
                }
            }
        }
    }
    None
}

/// Probe de presença (FAT/VFS) sem carregar o blob inteiro na heap se já cacheado.
pub fn has_named_blob(name: &str) -> bool {
    load_firmware_file(name).is_some()
}

/// Carrega ACR Pascal via `nvidia_pascal_acr` + aplica `sw_*` quando WPR ok.
///
/// `SecureBootResult::Ok` = `HsBooted` apenas (não compute Ready).
pub unsafe fn nvidia_acr_load(gpu: &GpuInfo, pmoff: u64) -> SecureBootResult {
    if !crate::gpu::detect::is_nvidia_legacy_acr(gpu.arch) {
        k_nano::slog_hal!(
            "ACR",
            "info",
            "NVIDIA {}: skip LegacyAcr family={} — use GSP path",
            gpu.name,
            crate::gpu::detect::nvidia_family_str(gpu.arch)
        );
        return SecureBootResult::NoFirmware;
    }

    let mmio = gpu.bar0 + pmoff;
    let bar2 = gpu.bar2 + pmoff;
    let report = nvidia_pascal_acr::bring_up_acr(gpu, mmio, bar2);
    *LAST_ACR.lock() = Some(report);

    // sw_* após qualquer estágio ≥ WprBuilt (inclui HsTimeout).
    if report.wpr_ok() {
        let sw = nvidia_pascal_sw::apply_sw(mmio, &report);
        *LAST_SW.lock() = Some(sw);
    }

    match report.stage {
        AcrStage::HsBooted => SecureBootResult::Ok,
        AcrStage::BlobsMissing => SecureBootResult::NoFirmware,
        AcrStage::Failed => SecureBootResult::InitFail,
        AcrStage::WprBuilt | AcrStage::HsSubmitted | AcrStage::HsTimeout => {
            k_nano::slog_hal!("ACR", "info", "{}: stage={:?} — sem HsBooted (não alegar VRAM unlocked)", gpu.name, report.stage);
            SecureBootResult::InitFail
        }
    }
}

pub unsafe fn amd_psp_load(gpu: &GpuInfo, pmoff: u64) -> SecureBootResult {
    let mmio = gpu.bar0 + pmoff;
    let report = crate::gpu::amd_psp::bring_up_psp(gpu, mmio);
    match report.stage {
        crate::gpu::amd_psp::PspStage::Authed => SecureBootResult::Ok,
        crate::gpu::amd_psp::PspStage::BlobsMissing => SecureBootResult::NoFirmware,
        crate::gpu::amd_psp::PspStage::Uploaded
        | crate::gpu::amd_psp::PspStage::AuthTimeout
        | crate::gpu::amd_psp::PspStage::Failed => {
            k_nano::slog_hal!("AMD", "PSP", "{}: stage={:?} — blob≠engine (InitFail honesto)", gpu.name, report.stage);
            SecureBootResult::InitFail
        }
    }
}

pub unsafe fn intel_guc_load(gpu: &GpuInfo, pmoff: u64) -> SecureBootResult {
    let mmio = gpu.bar0 + pmoff;
    let report = crate::gpu::intel_guc::bring_up_guc(gpu, mmio);
    match report.stage {
        crate::gpu::intel_guc::GucStage::Booted | crate::gpu::intel_guc::GucStage::SkippedGen9 => {
            SecureBootResult::Ok
        }
        crate::gpu::intel_guc::GucStage::BlobsMissing => SecureBootResult::NoFirmware,
        crate::gpu::intel_guc::GucStage::BootTimeout
        | crate::gpu::intel_guc::GucStage::Uploaded
        | crate::gpu::intel_guc::GucStage::Failed => SecureBootResult::InitFail,
    }
}

/// Verifica se firmware NVIDIA ACR esta disponivel (sem carregar).
pub fn nvidia_acr_load_available() -> bool {
    load_firmware_file("fecs_bl.bin").is_some()
        && load_firmware_file("bl.bin").is_some()
        && load_firmware_file("ucode_load.bin").is_some()
}

/// Carrega firmware para qualquer dispositivo baseado em VID/DID/classe.
/// Usado pelo SelfHealAgent para hot-load apos download.
pub fn hot_load_firmware(vid: u16, _did: u16, class: u8) -> SecureBootResult {
    match (vid, class) {
        (0x10DE, 0x03) => {
            // Tenta NVIDIA ACR loading via GPU detectado
            unsafe {
                let pmoff = k_nano::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
                let gpus = crate::gpu::detect::detect_all();
                for gpu in &gpus {
                    if gpu.vendor == crate::gpu::detect::GpuVendor::Nvidia {
                        return nvidia_acr_load(gpu, pmoff);
                    }
                }
            }
            SecureBootResult::NoFirmware
        }
        _ => SecureBootResult::NoFirmware,
    }
}

/// Teste de firmware — carrega e valida todos os blobs NVIDIA sem GPU.
/// Chamado no boot para debug, mesmo sem GPU NVIDIA presente.
pub fn test_load_firmware() -> bool {
        let names = [
        ("bl.bin", 0usize),
        ("ucode_load.bin", 0),
        ("fecs_bl.bin",    576usize),
        ("fecs_data.bin",  2248),
        ("fecs_inst.bin",  21161),
        ("fecs_sig.bin",   192),
        ("gpccs_bl.bin",   576),
        ("gpccs_data.bin", 2092),
        ("gpccs_inst.bin", 13095),
        ("gpccs_sig.bin",  192),
        ("sw_nonctx.bin", 0),
    ];
    k_nano::slog_hal!("FW", "TEST", "NVIDIA GP108 firmware check (ACR HS + LSF + sw):");
    let mut ok = true;
    for (name, exp) in &names {
        match load_firmware_file(name) {
            Some(data) if *exp == 0 || data.len() == *exp => {
                k_nano::slog_hal!("FW", "ok", "{} ({}B)", name, data.len());
            }
            Some(data) => {
                k_nano::slog_hal!("SZ", "info", "{}: {}B (esperado {}B)", name, data.len(), exp);
                ok = false;
            }
            None => {
                k_nano::slog_hal!("", "-", "{} NAO ENCONTRADO", name);
                ok = false;
            }
        }
    }
    if ok {
        k_nano::slog_hal!("FW", "TEST", "blobs ACR/LSF/sw presentes");
    } else {
        k_nano::slog_hal!("FW", "TEST", "firmware NVIDIA incompleto (ACR Degrau precisa HS+LSF)");
    }
    ok
}

pub unsafe fn secure_boot_gpu(gpu: &GpuInfo, pmoff: u64) -> SecureBootResult {
    k_nano::slog_hal!("SECURE", "BOOT", "{}: iniciando...", gpu.name);
    let result = match gpu.vendor {
        GpuVendor::Nvidia => {
            if crate::gpu::detect::is_nvidia_legacy_acr(gpu.arch) {
                nvidia_acr_load(gpu, pmoff)
            } else if crate::gpu::detect::is_nvidia_gsp_family(gpu.arch) {
                k_nano::slog_hal!(
                    "SECURE",
                    "BOOT",
                    "NVIDIA GSP family={} — scaffold (GSP-RM residual)",
                    crate::gpu::detect::nvidia_family_str(gpu.arch)
                );
                SecureBootResult::NoFirmware
            } else {
                SecureBootResult::NoFirmware
            }
        }
        GpuVendor::Amd => amd_psp_load(gpu, pmoff),
        GpuVendor::Intel => intel_guc_load(gpu, pmoff),
        _ => SecureBootResult::NoFirmware,
    };
    match result {
        SecureBootResult::Ok => k_nano::slog_hal!("GPU", "secureboot", "{}: OK", gpu.name),
        SecureBootResult::NoFirmware => k_nano::slog_hal!("GPU", "secureboot", "{}: sem firmware (ver docs/dead-ends.md)", gpu.name),
        SecureBootResult::SignatureFail => k_nano::slog_hal!("GPU", "secureboot", "{}: ASSINATURA INVALIDA!", gpu.name),
        SecureBootResult::InitFail => k_nano::slog_hal!("GPU", "secureboot", "{}: FALHA NA INICIALIZACAO", gpu.name),
    }
    result
}
