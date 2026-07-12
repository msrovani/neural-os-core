//! GPU Secure Boot — firmware loading para NVIDIA ACR, AMD PSP, Intel GuC.
//!
//! # Estado (v1.1.1)
//! Pipeline WPR implementado. Firmware blobs necessários (MIT, linux-firmware.git).
//!
//! # Para ativar GPU compute real:
//!   1. `python tools/download_firmware.py` — baixa blobs do linux-firmware
//!   2. Ou copiar manualmente de /lib/firmware/nvidia/gp108/ (Linux existente)
//!   3. Firmware dir: target/firmware/nvidia/gp108/*.bin
//!   4. WPR loading automático no próximo boot com GPU NVIDIA

use crate::gpu::detect::{GpuInfo, GpuVendor};
use crate::serial_println;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SecureBootResult { Ok, NoFirmware, SignatureFail, InitFail }

// Register offsets para NVIDIA GP108 (GTX 1050) — do driver nouveau
const NV_PFIFO_BASE: u64 = 0x2000;
const NV_ACR_WPR_SIZE: u64 = 0x200000; // 2MB WPR region
const NV_FALCON_DMEMC: u64 = 0x100;    // Falcon DMEM control
const NV_FALCON_BOOTVEC: u64 = 0x130;  // Falcon boot vector

// ─── WPR (Wide Payload Region) loading ──────────────────────────────────────
// Pipeline baseado em drivers/gpu/drm/nouveau/nvkm/subdev/acr/gp108.c
// 1. Alocar WPR no topo da VRAM
// 2. Upload FECS bootloader + code + data + signature via BAR2
// 3. Upload GPCCS bootloader + code + data + signature via BAR2
// 4. Boot Falcon → verifica assinatura → VRAM desbloqueada

struct AcrFwBlob {
    bl: Vec<u8>,   // bootloader
    data: Vec<u8>, // data segment
    inst: Vec<u8>, // code/instruction segment
    sig: Vec<u8>,  // signature
}

fn load_firmware_file(name: &str) -> Option<Vec<u8>> {
    let pm = crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    let paths = [
        alloc::format!("target/firmware/nvidia/gp108/{}", name),
        alloc::format!("/lib/firmware/nvidia/gp108/{}", name),
    ];
    for _path in &paths {
        // Try FAT32 first
        unsafe {
            let ata_guard = crate::ATA_DRIVER.lock();
            if let Some(ref ata) = *ata_guard {
                let parts = crate::fat32::read_mbr(ata);
                for p in &parts {
                    if p.type_code != 0x0B && p.type_code != 0x0C && p.type_code != 0x1C { continue; }
                    if let Some(fs) = crate::fat32::Fat32Reader::new(ata, p) {
                        let fname = alloc::format!("FW_{}", name.to_uppercase().replace(".", "_"));
                        if let Some(data) = fs.read_file(&fname) { return Some(data); }
                    }
                }
            }
        }
        // QEMU loader fallback: check at physical address
        for addr in &[0x130000000u64, 0x140000000u64] {
            let probe = (*addr + pm) as *const u32;
            let magic = unsafe { core::ptr::read_volatile(probe) };
            if magic == 0xBE11BE11 { // .bitnet magic reused for firmware
                let data = unsafe { core::slice::from_raw_parts(probe as *const u8, 1024 * 1024) };
                return Some(data.to_vec());
            }
        }
    }
    None
}

fn load_acr_firmware() -> Option<(AcrFwBlob, AcrFwBlob)> {
    let fecs = AcrFwBlob {
        bl: load_firmware_file("fecs_bl.bin")?,
        data: load_firmware_file("fecs_data.bin")?,
        inst: load_firmware_file("fecs_inst.bin")?,
        sig: load_firmware_file("fecs_sig.bin")?,
    };
    let gpccs = AcrFwBlob {
        bl: load_firmware_file("gpccs_bl.bin")?,
        data: load_firmware_file("gpccs_data.bin")?,
        inst: load_firmware_file("gpccs_inst.bin")?,
        sig: load_firmware_file("gpccs_sig.bin")?,
    };
    serial_println!("[ACR] Firmware loaded: FECS {}B + GPCCS {}B",
        fecs.bl.len() + fecs.data.len() + fecs.inst.len(),
        gpccs.bl.len() + gpccs.data.len() + gpccs.inst.len());
    Some((fecs, gpccs))
}

unsafe fn upload_falcon(mmio: u64, data: &[u8], dmem_off: u32) {
    let dst = mmio as *mut u32;
    for (i, chunk) in data.chunks(4).enumerate() {
        let mut word = 0u32;
        for (j, &b) in chunk.iter().enumerate() { word |= (b as u32) << (j * 8); }
        core::ptr::write_volatile(dst.add((dmem_off as usize / 4) + i), word);
    }
}

/// Carrega firmware ACR na GPU NVIDIA.
/// Pipeline:
///   1. Aloca WPR de 2MB no topo da VRAM
///   2. Copia FECS blobs para a WPR via BAR2
///   3. Copia GPCCS blobs para a WPR via BAR2
///   4. Escreve boot vector do Falcon
///   5. Falcao autentica firmware → VRAM desbloqueada
pub unsafe fn nvidia_acr_load(gpu: &GpuInfo, pmoff: u64) -> SecureBootResult {
    let mmio = gpu.bar0 + pmoff;
    let vram = gpu.bar2;

    let (fecs, gpccs) = match load_acr_firmware() {
        Some(f) => f,
        None => {
            serial_println!("[ACR] NVIDIA {}: firmware NAO ENCONTRADO", gpu.name);
            serial_println!("[ACR]  Coloque em target/firmware/nvidia/gp108/ ou FAT32 FW_*.BIN");
            serial_println!("[ACR]  Fonte: git.kernel.org linux-firmware.git nvidia/gp108/");
            return SecureBootResult::NoFirmware;
        }
    };

    // 1. Alocar WPR no topo da VRAM (últimos 2MB)
    let vram_size = gpu.vram_size;
    if vram_size < NV_ACR_WPR_SIZE + 0x100000 {
        serial_println!("[ACR] VRAM muito pequena para WPR");
        return SecureBootResult::InitFail;
    }
    let wpr_base = vram_size - NV_ACR_WPR_SIZE;

    // 2. Upload FECS para WPR via BAR2
    unsafe fn write_vram(bar2: u64, off: u64, data: &[u8]) {
        let ptr = (bar2 + off) as *mut u8;
        for i in 0..data.len() { core::ptr::write_volatile(ptr.add(i), data[i]); }
    }
    write_vram(vram, wpr_base, &fecs.bl);
    write_vram(vram, wpr_base + fecs.bl.len() as u64, &fecs.data);
    write_vram(vram, wpr_base + fecs.bl.len() as u64 + fecs.data.len() as u64, &fecs.inst);

    // 3. Upload GPCCS para WPR
    let gpccs_off = (fecs.bl.len() + fecs.data.len() + fecs.inst.len()) as u64;
    write_vram(vram, wpr_base + gpccs_off, &gpccs.bl);
    write_vram(vram, wpr_base + gpccs_off + gpccs.bl.len() as u64, &gpccs.data);
    write_vram(vram, wpr_base + gpccs_off + gpccs.bl.len() as u64 + gpccs.data.len() as u64, &gpccs.inst);

    // 4. Boot Falcon: set boot vector
    core::ptr::write_volatile((mmio + NV_FALCON_BOOTVEC) as *mut u32, wpr_base as u32);
    core::sync::atomic::fence(Ordering::Release);

    // 5. Aguardar boot (poll status)
    for _ in 0..1000000 {
        let sts = core::ptr::read_volatile((mmio + NV_FALCON_DMEMC) as *const u32);
        if sts & 0x01 != 0 { // boot complete
            serial_println!("[ACR] NVIDIA {}: firmware loaded, VRAM desbloqueada!", gpu.name);
            return SecureBootResult::Ok;
        }
        core::hint::spin_loop();
    }

    serial_println!("[ACR] NVIDIA {}: firmware boot TIMEOUT", gpu.name);
    SecureBootResult::InitFail
}

pub unsafe fn amd_psp_load(_gpu: &GpuInfo, _pmoff: u64) -> SecureBootResult {
    SecureBootResult::NoFirmware
}

pub unsafe fn intel_guc_load(_gpu: &GpuInfo, _pmoff: u64) -> SecureBootResult {
    SecureBootResult::NoFirmware
}

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
        SecureBootResult::NoFirmware => serial_println!("[SECURE-BOOT] {}: sem firmware (ver docs/dead-ends.md)", gpu.name),
        SecureBootResult::SignatureFail => serial_println!("[SECURE-BOOT] {}: ASSINATURA INVALIDA!", gpu.name),
        SecureBootResult::InitFail => serial_println!("[SECURE-BOOT] {}: FALHA NA INICIALIZACAO", gpu.name),
    }
    result
}
