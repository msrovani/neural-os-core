//! GPU Detection — scan PCI class 0x03, família/IP, caps honestas (has_compute=false até canário).

use alloc::vec::Vec;
use crate::gpu::compute_abi::{ComputeBackendKind, ComputeCaps, IsaTag, BackendState};
use k_nano::pci::PciDevice;
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
    /// Maxwell (GM10x) — LegacyAcr / pré-GSP
    NvidiaMaxwell,
    NvidiaPascal,
    /// Volta (GV100) — LegacyAcr até GspBackend existir
    NvidiaVolta,
    NvidiaTuring, NvidiaAmpere, NvidiaAda, NvidiaBlackwell,
    AmdRdna1, AmdRdna2, AmdRdna3, AmdRdna4,
    AmdGcn, // Vega/APU pré-RDNA (ex. Renoir hint)
    VirtIo,
    Unknown,
}

/// Família NVIDIA para slog / VERDICT (não é SKU).
pub fn nvidia_family_str(arch: GpuArch) -> &'static str {
    match arch {
        GpuArch::NvidiaMaxwell => "maxwell",
        GpuArch::NvidiaPascal => "pascal",
        GpuArch::NvidiaVolta => "volta",
        GpuArch::NvidiaTuring => "turing",
        GpuArch::NvidiaAmpere => "ampere",
        GpuArch::NvidiaAda => "ada",
        GpuArch::NvidiaBlackwell => "blackwell",
        _ => "none",
    }
}

/// Maxwell / Pascal / Volta → caminho LegacyAcr (ACR/WPR), não GSP-RM.
pub fn is_nvidia_legacy_acr(arch: GpuArch) -> bool {
    matches!(
        arch,
        GpuArch::NvidiaMaxwell | GpuArch::NvidiaPascal | GpuArch::NvidiaVolta
    )
}

/// Turing+ → GspBackend (scaffold até RPC pleno).
pub fn is_nvidia_gsp_family(arch: GpuArch) -> bool {
    matches!(
        arch,
        GpuArch::NvidiaTuring
            | GpuArch::NvidiaAmpere
            | GpuArch::NvidiaAda
            | GpuArch::NvidiaBlackwell
    )
}

#[derive(Debug, Clone)]
pub struct GpuInfo {
    pub vendor: GpuVendor,
    pub arch: GpuArch,
    pub device_id: u16,
    pub bar0: u64,
    pub bar2: u64,
    pub vram_size: u64,
    pub has_display_engine: bool,
    /// Só true após canário vector_add (golden). Boot sempre começa false.
    pub has_compute: bool,
    pub is_integrated: bool,
    pub pci_bus: u8, pub pci_dev: u8, pub pci_fn: u8,
    /// D-state PCI (PM capability): 0=D0 ativo, 3=D3hot, 4=D3cold. dGPU
    /// frequentemente dorme (D3) no boot — BARs visíveis mas tocar = hang.
    pub pci_dstate: u8,
    pub name: &'static str,
    /// Backend candidato após family probe (não implica Ready).
    pub backend_kind: ComputeBackendKind,
    pub isa_tag: IsaTag,
    /// Pode receber KernelPack / bring-up de compute (ainda sem golden).
    pub compute_candidate: bool,
}

impl GpuInfo {
    pub fn vram_mb(&self) -> u64 { self.vram_size / (1024 * 1024) }

    pub fn bar0_size(&self) -> u64 {
        match self.vendor {
            GpuVendor::Intel => 0x1000000,
            GpuVendor::Nvidia => 0x1000000,
            GpuVendor::Amd => 0x400000,
            GpuVendor::VirtIo => 0x1000,
            GpuVendor::Unknown => 0x100000,
        }
    }

    pub fn to_caps(&self, state: BackendState) -> ComputeCaps {
        use crate::gpu::compute_abi::IntelSubmission;
        let has_ccs = matches!(
            self.arch,
            GpuArch::IntelXe | GpuArch::IntelXe2
        ) && !self.is_integrated;
        let intel_submission = match self.arch {
            GpuArch::IntelGen9 => IntelSubmission::Ring,
            GpuArch::IntelGen12 | GpuArch::IntelXe | GpuArch::IntelXe2 => IntelSubmission::GuC,
            _ => IntelSubmission::None,
        };
        ComputeCaps {
            vendor: self.vendor,
            arch: self.arch,
            backend: self.backend_kind,
            state,
            has_compute: self.has_compute && state == BackendState::Ready,
            has_display: self.has_display_engine,
            is_integrated: self.is_integrated,
            vram_bytes: self.vram_size,
            isa_tag: self.isa_tag,
            has_ccs,
            intel_submission,
            mad_int8_host: false,
        }
    }
}

/// Detecta todas as GPUs no sistema. `has_compute` inicia false.
pub unsafe fn detect_all() -> Vec<GpuInfo> {
    let devices = k_nano::pci::scan_pci();
    let mut gpus = Vec::new();

    for dev in &devices {
        if dev.class != 0x03 { continue; }

        let vendor = match dev.vendor_id {
            VENDOR_INTEL => GpuVendor::Intel,
            VENDOR_NVIDIA => GpuVendor::Nvidia,
            VENDOR_AMD => GpuVendor::Amd,
            VENDOR_VIRTIO => GpuVendor::VirtIo,
            _ => GpuVendor::Unknown,
        };

        let (arch, name, table_vram, is_igpu_hint) = identify_gpu(dev);

        let is_integrated = match vendor {
            GpuVendor::Intel => is_igpu_hint || table_vram == 0,
            GpuVendor::Amd => is_igpu_hint,
            GpuVendor::VirtIo => true,
            _ => false,
        };

        // AIOS: mede o silício, não assume tabela (ADR-0087 §2.0.1).
        // VRAM aperture = maior BAR de memória medido ≥ 64MB (sem ReBAR ≈ 256MB;
        // APU/iGPU não têm BAR grande → DRAM compartilhada, honesto).
        // MMIO = BAR0, exceto quando BAR0 É a aperture (AMD dGPU: VRAM→BAR0,
        // doorbell→BAR2, MMIO→BAR5) → usa o par BAR4/5.
        let (bar0, bar2, vram_bytes) = unsafe {
            let pairs = [
                (
                    crate::pci_bar::decode_bar(dev.bar0, dev.bar1),
                    k_nano::pci::read_bar_size(dev.bus, dev.device, dev.function, 0),
                ),
                (
                    crate::pci_bar::decode_bar(dev.bar2, dev.bar3),
                    k_nano::pci::read_bar_size(dev.bus, dev.device, dev.function, 2),
                ),
                (
                    crate::pci_bar::decode_bar(dev.bar4, dev.bar5),
                    k_nano::pci::read_bar_size(dev.bus, dev.device, dev.function, 4),
                ),
            ];
            let vram_idx = pairs
                .iter()
                .enumerate()
                .filter(|(_, (base, size))| *base != 0 && *size >= 64 * 1024 * 1024)
                .max_by_key(|(_, (_, size))| *size)
                .map(|(i, _)| i);
            let vram = vram_idx
                .map(|i| (pairs[i].0, pairs[i].1))
                .unwrap_or((0, 0));
            let mmio = match vram_idx {
                // AMD dGPU: VRAM→BAR0 ⇒ MMIO=BAR5 (amdgpu Bonaire+); fallback BAR0
                Some(0) if pairs[2].0 != 0 => pairs[2].0,
                // APU: BAR0 ausente (VRAM = carveout de RAM, sem BAR) ⇒ MMIO=BAR5
                None if pairs[0].0 == 0 && pairs[2].0 != 0 => pairs[2].0,
                _ => pairs[0].0,
            };
            (mmio, vram.0, vram.1)
        };

        let (backend_kind, isa_tag, compute_candidate) =
            select_backend_family(vendor, arch, is_integrated);

        let gpu = GpuInfo {
            vendor, arch, device_id: dev.device_id,
            bar0, bar2, vram_size: vram_bytes,
            has_display_engine: is_integrated || dev.subclass == 0x00,
            has_compute: false,
            is_integrated,
            pci_bus: dev.bus, pci_dev: dev.device, pci_fn: dev.function,
            // SESSÃO_260: mede o D-state real (PMCSR) — AIOS mede o silício,
            // não assume. dGPU em D3 não deve receber compute/mapping.
            pci_dstate: k_nano::pci::pci_power_state(dev.bus, dev.device, dev.function).0,
            name,
            backend_kind,
            isa_tag,
            compute_candidate,
        };

        k_nano::slog_hal!("GPU", "info", "{}: {:04x}:{:04x} arch={:?} backend={:?} isa={} bar0={:#x} bar2={:#x} vram={}MB igpu={} Dstate={} compute_cand={} has_compute=false",
            gpu.name, dev.vendor_id, dev.device_id, gpu.arch, gpu.backend_kind,
            gpu.isa_tag.as_str(), gpu.bar0, gpu.bar2, gpu.vram_mb(),
            gpu.is_integrated, gpu.pci_dstate, gpu.compute_candidate);

        gpus.push(gpu);
    }
    gpus
}

fn select_backend_family(
    vendor: GpuVendor,
    arch: GpuArch,
    is_integrated: bool,
) -> (ComputeBackendKind, IsaTag, bool) {
    match vendor {
        GpuVendor::Nvidia => match arch {
            GpuArch::NvidiaMaxwell => (ComputeBackendKind::LegacyAcr, IsaTag::Sm52, true),
            GpuArch::NvidiaPascal => (ComputeBackendKind::LegacyAcr, IsaTag::Sm61, true),
            GpuArch::NvidiaVolta => (ComputeBackendKind::LegacyAcr, IsaTag::Sm70, true),
            GpuArch::NvidiaTuring => (ComputeBackendKind::Gsp, IsaTag::Sm75, true),
            GpuArch::NvidiaAmpere => (ComputeBackendKind::Gsp, IsaTag::Sm80, true),
            GpuArch::NvidiaAda | GpuArch::NvidiaBlackwell => {
                (ComputeBackendKind::Gsp, IsaTag::Sm89, true)
            }
            _ => (ComputeBackendKind::CpuFallback, IsaTag::None, false),
        },
        GpuVendor::Intel => match arch {
            GpuArch::IntelGen9 => (ComputeBackendKind::Gen9Ring, IsaTag::Gen9, true),
            GpuArch::IntelXe | GpuArch::IntelXe2 if !is_integrated => {
                (ComputeBackendKind::XeHpgCcs, IsaTag::Dg2, true)
            }
            GpuArch::IntelGen12 | GpuArch::IntelXe => {
                // iGPU Xe-LP: display-first; compute só se única GPU no plan
                (ComputeBackendKind::Gen9Ring, IsaTag::Gen9, true)
            }
            _ => (ComputeBackendKind::Unknown, IsaTag::None, false),
        },
        GpuVendor::Amd => match arch {
            GpuArch::AmdRdna3 if is_integrated => (ComputeBackendKind::Mes, IsaTag::Gfx1103, true),
            GpuArch::AmdRdna2 if is_integrated => (ComputeBackendKind::KiQ, IsaTag::Gfx1036, true),
            GpuArch::AmdGcn | GpuArch::AmdRdna1 if is_integrated => {
                (ComputeBackendKind::KiQ, IsaTag::Gfx90c, true)
            }
            GpuArch::AmdRdna2 => (ComputeBackendKind::KiQ, IsaTag::Gfx1030, true),
            GpuArch::AmdRdna3 | GpuArch::AmdRdna4 => (ComputeBackendKind::Mes, IsaTag::Gfx1103, true),
            _ => (ComputeBackendKind::Unknown, IsaTag::None, false),
        },
        GpuVendor::VirtIo => (ComputeBackendKind::VirtIoDisplay, IsaTag::None, false),
        GpuVendor::Unknown => (ComputeBackendKind::CpuFallback, IsaTag::None, false),
    }
}

fn identify_gpu(dev: &PciDevice) -> (GpuArch, &'static str, u64, bool) {
    // (arch, name, vram_hint_bytes, is_integrated_hint)
    match (dev.vendor_id, dev.device_id) {
        // Intel Gen9
        (0x8086, 0x5916) => (GpuArch::IntelGen9, "Intel HD Graphics 620", 0, true),
        (0x8086, 0x5917) => (GpuArch::IntelGen9, "Intel UHD Graphics 620", 0, true),
        (0x8086, 0x591B) => (GpuArch::IntelGen9, "Intel HD Graphics 630", 0, true),
        (0x8086, 0x5912) => (GpuArch::IntelGen9, "Intel HD Graphics 630 DT", 0, true),
        (0x8086, 0x3E92) => (GpuArch::IntelGen9, "Intel UHD Graphics 630", 0, true),
        (0x8086, 0x3E9B) => (GpuArch::IntelGen9, "Intel UHD Graphics 630 Mob", 0, true),
        (0x8086, 0x9BC4) => (GpuArch::IntelGen12, "Intel UHD Graphics", 0, true),
        (0x8086, 0x4680) => (GpuArch::IntelXe, "Intel Iris Xe Graphics", 0, true),
        (0x8086, 0x46A6) => (GpuArch::IntelXe, "Intel Iris Xe Graphics", 0, true),
        // Intel Arc dGPU (LMEM) — separado de Iris Xe iGPU
        (0x8086, 0x56A0) => (GpuArch::IntelXe, "Intel Arc A310", 4096 * 1024 * 1024, false),
        (0x8086, 0x56A1) => (GpuArch::IntelXe, "Intel Arc A380", 6144 * 1024 * 1024, false),
        (0x8086, 0x56A2) => (GpuArch::IntelXe, "Intel Arc A580", 8192 * 1024 * 1024, false),
        (0x8086, 0x56A3) => (GpuArch::IntelXe, "Intel Arc A750", 8192 * 1024 * 1024, false),
        (0x8086, 0x56A4) => (GpuArch::IntelXe, "Intel Arc A770 8GB", 8192 * 1024 * 1024, false),
        (0x8086, 0x56A5) => (GpuArch::IntelXe, "Intel Arc A770 16GB", 16384 * 1024 * 1024, false),
        (0x8086, 0x5690) => (GpuArch::IntelXe, "Intel Arc A370M", 4096 * 1024 * 1024, false),
        (0x8086, 0xE20B) => (GpuArch::IntelXe2, "Intel Arc B580", 12288 * 1024 * 1024, false),
        (0x8086, 0xE20C) => (GpuArch::IntelXe2, "Intel Arc B570", 10240 * 1024 * 1024, false),
        (0x8086, 0xE120) => (GpuArch::IntelXe2, "Intel Arc B580", 12288 * 1024 * 1024, false),
        // Intel desconhecido: GMD_ID / faixa DID
        (0x8086, did) if class_is_display(dev) => identify_intel_unknown(dev, did),
        (0x10DE, 0x1C81) => (GpuArch::NvidiaPascal, "NVIDIA GTX 1050", 2048 * 1024 * 1024, false),
        (0x10DE, 0x1C82) => (GpuArch::NvidiaPascal, "NVIDIA GTX 1050 Ti", 4096 * 1024 * 1024, false),
        (0x10DE, 0x1C8C) => (GpuArch::NvidiaPascal, "NVIDIA GTX 1050 Mobile", 2048 * 1024 * 1024, false),
        (0x10DE, 0x1C8D) => (GpuArch::NvidiaPascal, "NVIDIA GTX 1050 Ti Mobile", 4096 * 1024 * 1024, false),
        (0x10DE, 0x1B06) => (GpuArch::NvidiaPascal, "NVIDIA GTX 1080 Ti", 11264 * 1024 * 1024, false),
        // NVIDIA Turing
        (0x10DE, 0x1F08) => (GpuArch::NvidiaTuring, "NVIDIA RTX 2060", 6144 * 1024 * 1024, false),
        (0x10DE, 0x1F07) => (GpuArch::NvidiaTuring, "NVIDIA RTX 2060 Super", 8192 * 1024 * 1024, false),
        (0x10DE, 0x1E84) => (GpuArch::NvidiaTuring, "NVIDIA RTX 2070 Super", 8192 * 1024 * 1024, false),
        (0x10DE, 0x1F82) => (GpuArch::NvidiaTuring, "NVIDIA RTX 2070", 8192 * 1024 * 1024, false),
        (0x10DE, 0x1F12) => (GpuArch::NvidiaTuring, "NVIDIA RTX 2060 Mobile", 6144 * 1024 * 1024, false),
        // NVIDIA Ampere / Ada
        (0x10DE, 0x2204) => (GpuArch::NvidiaAmpere, "NVIDIA RTX 3060", 12288 * 1024 * 1024, false),
        (0x10DE, 0x2482) => (GpuArch::NvidiaAda, "NVIDIA RTX 4060", 8192 * 1024 * 1024, false),
        (0x10DE, 0x2782) => (GpuArch::NvidiaAda, "NVIDIA RTX 4070 Super", 12288 * 1024 * 1024, false),
        (0x10DE, 0x2783) => (GpuArch::NvidiaAda, "NVIDIA RTX 4070", 12288 * 1024 * 1024, false),
        (0x10DE, 0x2684) => (GpuArch::NvidiaAda, "NVIDIA RTX 4090", 24576 * 1024 * 1024, false),
        (0x10DE, 0x2C82) => (GpuArch::NvidiaBlackwell, "NVIDIA RTX 5090", 32768 * 1024 * 1024, false),
        // AMD APU iGPU
        (0x1002, 0x1638) => (GpuArch::AmdGcn, "AMD Radeon Graphics (Cezanne/5600G)", 0, true),
        (0x1002, 0x1636) => (GpuArch::AmdGcn, "AMD Radeon Graphics (Renoir)", 0, true),
        (0x1002, 0x164C) => (GpuArch::AmdRdna2, "AMD Radeon Graphics (Rembrandt)", 0, true),
        (0x1002, 0x15BF) => (GpuArch::AmdRdna2, "AMD Radeon Graphics (Raphael)", 0, true),
        (0x1002, 0x15C8) => (GpuArch::AmdRdna3, "AMD Radeon Graphics (Phoenix/8700G)", 0, true),
        (0x1002, 0x1900) => (GpuArch::AmdRdna3, "AMD Radeon Graphics (Phoenix2)", 0, true),
        // AMD dGPU
        (0x1002, 0x731F) => (GpuArch::AmdRdna2, "AMD RX 6600", 8192 * 1024 * 1024, false),
        (0x1002, 0x73BF) => (GpuArch::AmdRdna2, "AMD RX 6800 XT", 16384 * 1024 * 1024, false),
        (0x1002, 0x73DF) => (GpuArch::AmdRdna2, "AMD RX 6700 XT", 12288 * 1024 * 1024, false),
        (0x1002, 0x7448) => (GpuArch::AmdRdna3, "AMD RX 7700 XT", 12288 * 1024 * 1024, false),
        (0x1002, 0x7470) => (GpuArch::AmdRdna3, "AMD RX 7800 XT", 16384 * 1024 * 1024, false),
        (0x1002, 0x744C) => (GpuArch::AmdRdna3, "AMD RX 7900 XTX", 24576 * 1024 * 1024, false),
        (0x1002, 0x7510) => (GpuArch::AmdRdna4, "AMD RX 9070 XT", 16384 * 1024 * 1024, false),
        // VirtIO
        (0x1AF4, 0x1050) => (GpuArch::VirtIo, "VirtIO-GPU (modern)", 0, true),
        (0x1AF4, 0x1045) => (GpuArch::VirtIo, "VirtIO-GPU (transitional)", 0, true),
        // NVIDIA desconhecido: faixa DID + PMC_BOOT_0 (BAR0+0)
        (0x10DE, did) => identify_nvidia_unknown(dev, did),
        // AMD desconhecido: faixas DID (PCI hint; IP Discovery confirma no probe)
        (0x1002, did) if class_is_display(dev) => identify_amd_unknown(did),
        (0x1002, _) => (GpuArch::Unknown, "AMD GPU (unknown DID)", 0, false),
        _ => (GpuArch::Unknown, "Unknown GPU", 0, false),
    }
}

/// Faixas DID AMD comuns — hint só; compute após IP Discovery.
fn identify_amd_unknown(did: u16) -> (GpuArch, &'static str, u64, bool) {
    match did {
        // APU / iGPU faixas modernas
        0x15E7..=0x15FF | 0x1630..=0x164F | 0x1680..=0x16FF => {
            (GpuArch::AmdRdna2, "AMD Radeon Graphics (APU DID range)", 0, true)
        }
        0x1900..=0x19FF => (GpuArch::AmdRdna3, "AMD Radeon Graphics (Phoenix DID range)", 0, true),
        // Navi21/22/23 RDNA2 dGPU
        0x73A0..=0x73FF => {
            (GpuArch::AmdRdna2, "AMD Radeon RX (Navi2x DID range)", 8 * 1024 * 1024 * 1024, false)
        }
        // Navi3x RDNA3
        0x7440..=0x747F => {
            (GpuArch::AmdRdna3, "AMD Radeon RX (Navi3x DID range)", 12 * 1024 * 1024 * 1024, false)
        }
        // RDNA4
        0x7500..=0x75FF => {
            (GpuArch::AmdRdna4, "AMD Radeon RX (RDNA4 DID range)", 16 * 1024 * 1024 * 1024, false)
        }
        // Vega / GFX9 residual
        0x66A0..=0x69FF => (GpuArch::AmdGcn, "AMD Radeon (GFX9 DID range)", 0, false),
        _ => (GpuArch::Unknown, "AMD GPU (unknown DID)", 0, false),
    }
}

fn class_is_display(dev: &PciDevice) -> bool {
    dev.class == 0x03
}

/// GMD_ID @ 0xd8c (MTL+) ou faixas DID Gen9/12/Arc.
fn identify_intel_unknown(dev: &PciDevice, did: u16) -> (GpuArch, &'static str, u64, bool) {
    let bar0 = crate::pci_bar::decode_bar(dev.bar0, dev.bar1);
    if let Some((arch, name, igpu)) = unsafe { read_intel_gmd_id(bar0) } {
        k_nano::slog_hal!("GPU", "info", "Intel DID={:04x} GMD_ID → {:?} ({}) igpu={}", did, arch, name, igpu);
        let vram = if igpu { 0 } else { 4096 * 1024 * 1024 };
        return (arch, name, vram, igpu);
    }
    // Faixas DID sem GMD (Gen9/11 típicos; Arc 0x56xx)
    let (arch, name, vram, igpu) = match did {
        0x1900..=0x19FF | 0x5900..=0x59FF | 0x3E00..=0x3EFF => {
            (GpuArch::IntelGen9, "Intel Gen9 (DID range)", 0u64, true)
        }
        0x8A00..=0x8AFF | 0x9A00..=0x9AFF | 0x9B00..=0x9BFF => {
            (GpuArch::IntelGen12, "Intel Gen12 (DID range)", 0, true)
        }
        0x4600..=0x46FF => (GpuArch::IntelXe, "Intel Xe-LP iGPU (DID range)", 0, true),
        0x5600..=0x56FF => (
            GpuArch::IntelXe,
            "Intel Arc/DG2 (DID range)",
            4096 * 1024 * 1024,
            false,
        ),
        0xE000..=0xE2FF => (
            GpuArch::IntelXe2,
            "Intel Xe2 (DID range)",
            8192 * 1024 * 1024,
            false,
        ),
        _ => (GpuArch::Unknown, "Intel GPU (unknown DID/GMD)", 0, true),
    };
    k_nano::slog_hal!("GPU", "info", "Intel DID={:04x} GMD=n/a → {:?} via DID range igpu={}", did, arch, igpu);
    (arch, name, vram, igpu)
}

/// `INTEL_GMD_ID` 0xd8c — presente em MTL+; Gen9 retorna lixo → None.
unsafe fn read_intel_gmd_id(bar0_phys: u64) -> Option<(GpuArch, &'static str, bool)> {
    if bar0_phys == 0 {
        return None;
    }
    let pmoff = k_nano::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    k_nano::apic::map_page_uc(bar0_phys, pmoff);
    // GMD_ID frequentemente em 0xd8c; também tentar 0xd00 page
    let v = core::ptr::read_volatile((bar0_phys + pmoff + 0xd8c) as *const u32);
    if v == 0 || v == 0xffff_ffff {
        return None;
    }
    // Layout simplificado: arch = bits [31:22] ou similar — i915 usa ver/release.
    // Degrau: se valor parece GMD (não forcewake lixo), classificar por faixa.
    let ver = ((v >> 22) & 0x3ff) as u16;
    let release = ((v >> 14) & 0xff) as u16;
    match ver {
        9..=11 => Some((GpuArch::IntelGen9, "Intel Gen9 (GMD_ID)", true)),
        12 if release >= 55 => Some((GpuArch::IntelXe, "Intel Xe-HPG (GMD_ID)", false)),
        12 => Some((GpuArch::IntelGen12, "Intel Gen12/Xe-LP (GMD_ID)", true)),
        20..=29 => Some((GpuArch::IntelXe2, "Intel Xe2 (GMD_ID)", false)),
        _ => {
            k_nano::slog_hal!("GPU", "info", "Intel GMD_ID={:#x} ver={} rel={} (unmapped)", v, ver, release);
            None
        }
    }
}

/// Faixas PCI DID comuns + `NV_PMC_BOOT_0` (Nouveau chipset = bits [28:20]).
fn identify_nvidia_unknown(dev: &PciDevice, did: u16) -> (GpuArch, &'static str, u64, bool) {
    let by_did = match did {
        // Maxwell GM10x (ex. 9xx / M) — faixa larga; PMC confirma
        0x1340..=0x17FF => Some((
            GpuArch::NvidiaMaxwell,
            "NVIDIA Maxwell (DID range)",
            2048 * 1024 * 1024,
        )),
        0x1B00..=0x1CFF => Some((
            GpuArch::NvidiaPascal,
            "NVIDIA Pascal (DID range)",
            2048 * 1024 * 1024,
        )),
        0x1D00..=0x1DFF => Some((
            GpuArch::NvidiaVolta,
            "NVIDIA Volta (DID range)",
            8192 * 1024 * 1024,
        )),
        0x1E00..=0x1FFF => Some((
            GpuArch::NvidiaTuring,
            "NVIDIA Turing (DID range)",
            6144 * 1024 * 1024,
        )),
        0x2200..=0x25FF => Some((
            GpuArch::NvidiaAmpere,
            "NVIDIA Ampere (DID range)",
            8192 * 1024 * 1024,
        )),
        0x2600..=0x27FF => Some((
            GpuArch::NvidiaAda,
            "NVIDIA Ada (DID range)",
            8192 * 1024 * 1024,
        )),
        0x2C00..=0x2FFF => Some((
            GpuArch::NvidiaBlackwell,
            "NVIDIA Blackwell (DID range)",
            16384 * 1024 * 1024,
        )),
        _ => None,
    };

    let bar0 = crate::pci_bar::decode_bar(dev.bar0, dev.bar1);
    let pmc = unsafe { read_nvidia_pmc_boot0(bar0) };
    let by_pmc = pmc.and_then(arch_from_pmc_boot0);

    match (by_pmc, by_did) {
        (Some((arch, name)), _) => {
            let vram = by_did.map(|(_, _, v)| v).unwrap_or(2048 * 1024 * 1024);
            k_nano::slog_hal!("GPU", "info", "NVIDIA DID={:04x} PMC_BOOT_0={:#x} → {:?} ({})",
                did,
                pmc.unwrap_or(0),
                arch,
                name);
            (arch, name, vram, false)
        }
        (None, Some((arch, name, vram))) => {
            k_nano::slog_hal!("GPU", "info", "NVIDIA DID={:04x} PMC_BOOT_0=n/a → {:?} via DID range", did, arch);
            (arch, name, vram, false)
        }
        (None, None) => {
            k_nano::slog_hal!("GPU", "info", "NVIDIA DID={:04x} PMC_BOOT_0={:?} → Unknown (CpuFallback)", did, pmc);
            (GpuArch::Unknown, "NVIDIA GPU (unknown DID/PMC)", 0, false)
        }
    }
}

/// `NV_PMC_BOOT_0` @ BAR0+0. Mapeia 1 página UC; None se bus morto.
unsafe fn read_nvidia_pmc_boot0(bar0_phys: u64) -> Option<u32> {
    if bar0_phys == 0 {
        return None;
    }
    let pmoff = k_nano::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    k_nano::apic::map_page_uc(bar0_phys, pmoff);
    let v = core::ptr::read_volatile((bar0_phys + pmoff) as *const u32);
    if v == 0 || v == 0xffff_ffff {
        None
    } else {
        Some(v)
    }
}

/// Chipset Nouveau: `(boot0 & 0x1ff00000) >> 20` (GP107=0x137, TU106=0x166, …).
fn arch_from_pmc_boot0(boot0: u32) -> Option<(GpuArch, &'static str)> {
    let chipset = (boot0 & 0x1ff0_0000) >> 20;
    let arch = match chipset {
        0x110..=0x12f => GpuArch::NvidiaMaxwell, // GM107–GM20x
        0x130..=0x13f => GpuArch::NvidiaPascal,  // GP100–GP108
        0x140..=0x14f => GpuArch::NvidiaVolta,   // GV100
        0x160..=0x16f => GpuArch::NvidiaTuring,
        0x170..=0x17f => GpuArch::NvidiaAmpere,
        0x190..=0x19f => GpuArch::NvidiaAda,
        0x1b0..=0x1bf => GpuArch::NvidiaBlackwell,
        _ => return None,
    };
    let name = match arch {
        GpuArch::NvidiaMaxwell => "NVIDIA Maxwell (PMC_BOOT_0)",
        GpuArch::NvidiaPascal => "NVIDIA Pascal (PMC_BOOT_0)",
        GpuArch::NvidiaVolta => "NVIDIA Volta (PMC_BOOT_0)",
        GpuArch::NvidiaTuring => "NVIDIA Turing (PMC_BOOT_0)",
        GpuArch::NvidiaAmpere => "NVIDIA Ampere (PMC_BOOT_0)",
        GpuArch::NvidiaAda => "NVIDIA Ada (PMC_BOOT_0)",
        GpuArch::NvidiaBlackwell => "NVIDIA Blackwell (PMC_BOOT_0)",
        _ => "NVIDIA (PMC_BOOT_0)",
    };
    Some((arch, name))
}

/// Melhor candidato a compute (ainda pode estar sem golden).
pub fn best_compute_gpu(gpus: &[GpuInfo]) -> Option<&GpuInfo> {
    gpus.iter()
        .filter(|g| g.compute_candidate && !g.is_integrated)
        .max_by_key(|g| g.vram_size)
        .or_else(|| gpus.iter().filter(|g| g.compute_candidate).max_by_key(|g| g.vram_size))
}

/// GPU para display (prioriza iGPU).
pub fn best_display_gpu(gpus: &[GpuInfo]) -> Option<&GpuInfo> {
    gpus.iter()
        .filter(|g| g.has_display_engine)
        .max_by_key(|g| if g.is_integrated { 1 } else { 0 })
}

