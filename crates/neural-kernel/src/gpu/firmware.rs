//! GPU Secure Boot — firmware loading para NVIDIA ACR, AMD PSP, Intel GuC.
//! Pipeline: linux-firmware → kernel → BAR0 → GPU engine.
//! Firmware blobs disponíveis em linux-firmware desde 2017.

use crate::gpu::detect::{GpuInfo, GpuVendor};
use crate::serial_println;

/// Resultado do secure boot
pub enum SecureBootResult {
    Ok,
    NoFirmware,
    SignatureFail,
    InitFail,
}

/// Blob de firmware: endereço virtual + tamanho
pub struct FirmwareBlob {
    pub data: &'static [u8],
    pub name: &'static str,
}

// ─── NVIDIA ACR (Authenticated Code Radix) ─────────────────────────────────
// Pipeline: FECS blobs → WPR setup → LS ucode → GR falcon → verify signature
// Firmware: linux-firmware/nvidia/gp107/ (FECS+GPCCS signed, desde 2017)

/// Carrega firmware NVIDIA via ACR (se disponível no kernel)
pub unsafe fn nvidia_acr_load(gpu: &GpuInfo, _pmoff: u64) -> SecureBootResult {
    let _ = gpu;
    // Etapas (implementação futura quando firmware for embutido):
    // 1. Carregar FECS blobs (fecs_bl, fecs_data, fecs_inst, fecs_sig)
    // 2. WPR (Write Protection Region) setup no topo da VRAM
    // 3. Carregar GPCCS blobs (gpccs_bl, gpccs_data, gpccs_inst, gpccs_sig)
    // 4. LS ucode loading no GR engine falcon via SEC2/PMU
    // 5. Verificar signature
    serial_println!("[ACR] NVIDIA {}: firmware loading NAO IMPLEMENTADO (blobs necessarios)", gpu.name);
    serial_println!("[ACR]  Fonte: linux-firmware/nvidia/ (FECS+GPCCS disponiveis desde 2017)");
    SecureBootResult::NoFirmware
}

// ─── AMD PSP (Platform Security Processor) ─────────────────────────────────
// Pipeline: PSP firmware → PM4 init ring → RDNA compute
// Firmware: linux-firmware/amdgpu/ (MIT license)

/// Carrega firmware AMD via PSP (MIT license)
pub unsafe fn amd_psp_load(gpu: &GpuInfo, _pmoff: u64) -> SecureBootResult {
    let _ = gpu;
    // Etapas:
    // 1. PSP firmware loading via PCIe BAR
    // 2. PM4 init ring setup
    // 3. Authentication via PSP
    serial_println!("[PSP] AMD {}: firmware loading NAO IMPLEMENTADO (linux-firmware/amdgpu/)", gpu.name);
    SecureBootResult::NoFirmware
}

// ─── Intel GuC (Graphics micro-controller) ─────────────────────────────────
// Pipeline: GuC firmware → HuC auth → submission via ring
// Firmware: linux-firmware/i915/ (open source)

/// Carrega firmware Intel GuC/HuC
pub unsafe fn intel_guc_load(gpu: &GpuInfo, _pmoff: u64) -> SecureBootResult {
    let _ = gpu;
    // Etapas:
    // 1. GuC firmware loading via MMIO
    // 2. HuC authentication
    // 3. Submission via RCS ring (já implementado em intel.rs)
    serial_println!("[GUC] Intel {}: firmware loading NAO IMPLEMENTADO (linux-firmware/i915/)", gpu.name);
    SecureBootResult::NoFirmware
}

/// Inicia secure boot para a GPU detectada
pub unsafe fn secure_boot_gpu(gpu: &GpuInfo, pmoff: u64) -> SecureBootResult {
    serial_println!("[SECURE-BOOT] {}: iniciando...", gpu.name);
    let result = match gpu.vendor {
        GpuVendor::Nvidia => nvidia_acr_load(gpu, pmoff),
        GpuVendor::Amd => amd_psp_load(gpu, pmoff),
        GpuVendor::Intel => intel_guc_load(gpu, pmoff),
        _ => SecureBootResult::NoFirmware,
    };
    match result {
        SecureBootResult::Ok => serial_println!("[SECURE-BOOT] {}: OK", gpu.name),
        SecureBootResult::NoFirmware => serial_println!("[SECURE-BOOT] {}: firmware nao disponivel (ver linux-firmware)", gpu.name),
        SecureBootResult::SignatureFail => serial_println!("[SECURE-BOOT] {}: ASSINATURA INVALIDA!", gpu.name),
        SecureBootResult::InitFail => serial_println!("[SECURE-BOOT] {}: FALHA NA INICIALIZACAO", gpu.name),
    }
    result
}
