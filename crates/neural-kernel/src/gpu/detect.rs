//! GPU Detection — scan PCI class 0x03, identifica fabricante + modelo + VRAM.

use alloc::vec::Vec;
use crate::pci::PciDevice;
use crate::serial_println;

pub const VENDOR_INTEL: u16 = 0x8086;
pub const VENDOR_NVIDIA: u16 = 0x10DE;
pub const VENDOR_AMD: u16 = 0x1002;
pub const VENDOR_VIRTIO: u16 = 0x1AF4;
pub const VENDOR_UNKNOWN: u16 = 0xFFFF;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GpuVendor { Intel, Nvidia, Amd, VirtIo, Unknown }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GpuArch {
    IntelGen9, IntelGen12, IntelXe, IntelXe2,
    NvidiaPascal, NvidiaTuring, NvidiaAmpere, NvidiaAda, NvidiaBlackwell,
    AmdRdna1, AmdRdna2, AmdRdna3, AmdRdna4,
    VirtIo,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct GpuInfo {
    pub vendor: GpuVendor,
    pub arch: GpuArch,
    pub device_id: u16,
    pub bar0: u64,  // MMIO registers
    pub bar2: u64,  // VRAM base (BAR2 ou BAR1 dependendo do vendor)
    pub vram_size: u64,  // bytes
    pub has_display_engine: bool,
    pub has_compute: bool,
    pub is_integrated: bool,
    pub pci_bus: u8, pub pci_dev: u8, pub pci_fn: u8,
    pub name: &'static str,
}

impl GpuInfo {
    pub fn vram_mb(&self) -> u64 { self.vram_size / (1024 * 1024) }
}

/// Detecta todas as GPUs no sistema
pub unsafe fn detect_all() -> Vec<GpuInfo> {
    let devices = crate::pci::scan_pci();
    let mut gpus = Vec::new();

    for dev in &devices {
        if dev.class != 0x03 { continue; } // Display controller

        let vendor = match dev.vendor_id {
            VENDOR_INTEL => GpuVendor::Intel,
            VENDOR_NVIDIA => GpuVendor::Nvidia,
            VENDOR_AMD => GpuVendor::Amd,
            VENDOR_VIRTIO => GpuVendor::VirtIo,
            _ => GpuVendor::Unknown,
        };

        let (arch, name, vram_size) = identify_gpu(dev);
        // Intel iGPU = Intel + sem VRAM dedicada (vram_size == 0)
        // Intel dGPU (Arc) tem VRAM dedicada (vram_size > 0)
        let is_intel_igpu = dev.vendor_id == VENDOR_INTEL && vram_size == 0;

        // BAR0 = MMIO registers (memory BAR, bits 0-3 indicam tipo)
        let bar0_raw = (dev.bar0 as u64) | ((dev.bar1 as u64) << 32);
        let bar0 = if dev.bar0 & 1 == 0 { bar0_raw & !0xF } else { bar0_raw & 0xFFFF };
        // BAR2 = VRAM (NVIDIA/AMD) ou Intel Arc dGPU
        let (bar2, vram_bytes) = if is_intel_igpu {
            (0u64, 0u64)
        } else {
            let bar2_raw = (dev.bar2 as u64) | ((dev.bar3 as u64) << 32);
            let bar2 = if dev.bar2 & 1 == 0 { bar2_raw & !0xF } else { bar2_raw & 0xFFFF };
            (bar2, vram_size)
        };

        let gpu = GpuInfo {
            vendor, arch, device_id: dev.device_id,
            bar0, bar2, vram_size: vram_bytes,
            has_display_engine: is_intel_igpu || dev.subclass == 0x00,
            has_compute: true,
            is_integrated: is_intel_igpu,
            pci_bus: dev.bus, pci_dev: dev.device, pci_fn: dev.function,
            name,
        };

        serial_println!("[GPU] {}: {:04x}:{:04x} arch={:?} bar0={:#x} bar2={:#x} vram={}MB display={}",
            gpu.name, dev.vendor_id, dev.device_id, gpu.arch,
            gpu.bar0, gpu.bar2, gpu.vram_mb(), gpu.has_display_engine);

        gpus.push(gpu);
    }
    gpus
}

fn identify_gpu(dev: &PciDevice) -> (GpuArch, &'static str, u64) {
    match (dev.vendor_id, dev.device_id) {
        // Intel
        (0x8086, 0x5916) => (GpuArch::IntelGen9, "Intel HD Graphics 620", 0),
        (0x8086, 0x591B) => (GpuArch::IntelGen9, "Intel HD Graphics 630", 0),
        (0x8086, 0x9BC4) => (GpuArch::IntelGen12, "Intel UHD Graphics", 0),
        (0x8086, 0x4680) => (GpuArch::IntelXe, "Intel Iris Xe Graphics", 0),
        (0x8086, 0x56A0) => (GpuArch::IntelXe, "Intel Arc A310", 4096 * 1024 * 1024),
        (0x8086, 0x56A1) => (GpuArch::IntelXe, "Intel Arc A380", 6144 * 1024 * 1024),
        (0x8086, 0x56A2) => (GpuArch::IntelXe, "Intel Arc A580", 8192 * 1024 * 1024),
        (0x8086, 0x56A3) => (GpuArch::IntelXe, "Intel Arc A750", 8192 * 1024 * 1024),
        (0x8086, 0x56A4) => (GpuArch::IntelXe, "Intel Arc A770 8GB", 8192 * 1024 * 1024),
        (0x8086, 0x56A5) => (GpuArch::IntelXe, "Intel Arc A770 16GB", 16384 * 1024 * 1024),
        (0x8086, 0xE120) => (GpuArch::IntelXe2, "Intel Arc B580", 12288 * 1024 * 1024),
        // NVIDIA
        (0x10DE, 0x1C81) => (GpuArch::NvidiaPascal, "NVIDIA GTX 1050", 2048 * 1024 * 1024),
        (0x10DE, 0x1C82) => (GpuArch::NvidiaPascal, "NVIDIA GTX 1050 Ti", 4096 * 1024 * 1024),
        (0x10DE, 0x1B06) => (GpuArch::NvidiaPascal, "NVIDIA GTX 1080 Ti", 11264 * 1024 * 1024),
        (0x10DE, 0x1F82) => (GpuArch::NvidiaTuring, "NVIDIA RTX 2070", 8192 * 1024 * 1024),
        (0x10DE, 0x2204) => (GpuArch::NvidiaAmpere, "NVIDIA RTX 3060", 12288 * 1024 * 1024),
        (0x10DE, 0x2482) => (GpuArch::NvidiaAda, "NVIDIA RTX 4060", 8192 * 1024 * 1024),
        (0x10DE, 0x2684) => (GpuArch::NvidiaAda, "NVIDIA RTX 4090", 24576 * 1024 * 1024),
        (0x10DE, 0x2C82) => (GpuArch::NvidiaBlackwell, "NVIDIA RTX 5090", 32768 * 1024 * 1024),
        // AMD
        (0x1002, 0x731F) => (GpuArch::AmdRdna2, "AMD RX 6600", 8192 * 1024 * 1024),
        (0x1002, 0x73BF) => (GpuArch::AmdRdna2, "AMD RX 6800 XT", 16384 * 1024 * 1024),
        (0x1002, 0x7448) => (GpuArch::AmdRdna3, "AMD RX 7700 XT", 12288 * 1024 * 1024),
        (0x1002, 0x7470) => (GpuArch::AmdRdna3, "AMD RX 7800 XT", 16384 * 1024 * 1024),
        (0x1002, 0x744C) => (GpuArch::AmdRdna3, "AMD RX 7900 XTX", 24576 * 1024 * 1024),
        (0x1002, 0x7510) => (GpuArch::AmdRdna4, "AMD RX 9070 XT", 16384 * 1024 * 1024),
        // VirtIO
        (0x1AF4, 0x1050) => (GpuArch::VirtIo, "VirtIO-GPU (modern)", 0),
        (0x1AF4, 0x1045) => (GpuArch::VirtIo, "VirtIO-GPU (transitional)", 0),
        _ => (GpuArch::Unknown, "Unknown GPU", 0),
    }
}

/// Encontra a melhor GPU para compute (prioriza dGPU com mais VRAM)
pub fn best_compute_gpu(gpus: &[GpuInfo]) -> Option<&GpuInfo> {
    gpus.iter()
        .filter(|g| g.has_compute && !g.is_integrated)
        .max_by_key(|g| g.vram_size)
        .or_else(|| gpus.iter().filter(|g| g.has_compute).max_by_key(|g| g.vram_size))
}

/// Encontra a GPU para display (prioriza iGPU ou primeira com display engine)
pub fn best_display_gpu(gpus: &[GpuInfo]) -> Option<&GpuInfo> {
    gpus.iter()
        .filter(|g| g.has_display_engine)
        .max_by_key(|g| if g.is_integrated { 1 } else { 0 })
}
