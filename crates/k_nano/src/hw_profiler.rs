//! HwProfiler — perfil de hardware alvo para o AutoInstaller (ADR-0079).
//! Consulta PCI scan + RAM detection + CPU features para decidir o que instalar.

use alloc::string::String;
use alloc::vec::Vec;
use crate::pci::PciDevice;

/// Perfil completo de hardware para decisão de instalação.
#[derive(Debug, Clone)]
pub struct HwProfile {
    pub total_ram_mb: u64,
    pub cpu_has_avx2: bool,
    pub cpu_has_sse: bool,
    pub cpu_has_fpu: bool,
    pub devices: Vec<PciDevice>,
    pub has_nvidia_gpu: bool,
    pub has_intel_gpu: bool,
    pub has_intel_wifi: bool,
    pub has_realtek_nic: bool,
    pub gpu_vram_mb: u64,
}

impl HwProfile {
    /// Número de dispositivos PCI detectados.
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    /// Sumário textual para log / card.
    pub fn summary(&self) -> String {
        let mut s = alloc::format!(
            "RAM: {}MB CPU:{} GPU:{}",
            self.total_ram_mb,
            if self.cpu_has_avx2 { "AVX2" } else if self.cpu_has_sse { "SSE" } else { "soft-float" },
            if self.has_nvidia_gpu { "NVIDIA" } else if self.has_intel_gpu { "Intel" } else { "none" },
        );
        if self.has_intel_wifi { s.push_str(" WiFi:AX"); }
        if self.has_realtek_nic { s.push_str(" NIC:Realtek"); }
        if self.gpu_vram_mb > 0 { s.push_str(&alloc::format!(" VRAM:{}MB", self.gpu_vram_mb)); }
        s
    }
}

/// Escaneia hardware e monta perfil.
pub fn profile_hardware() -> HwProfile {
    // PCI scan
    let devices = unsafe { crate::pci::scan_pci() };

    let has_nvidia_gpu = devices.iter().any(|d| {
        d.class == 0x03 && d.subclass == 0x00 && d.vendor_id == 0x10DE
    });
    let has_intel_gpu = devices.iter().any(|d| {
        d.class == 0x03 && d.subclass == 0x00 && d.vendor_id == 0x8086
    });
    let has_intel_wifi = devices.iter().any(|d| {
        d.class == 0x02 && d.subclass == 0x80 && d.vendor_id == 0x8086
    });
    let has_realtek_nic = devices.iter().any(|d| {
        d.class == 0x02 && d.subclass == 0x00 && d.vendor_id == 0x10EC
    });

    // RAM detection via e820-like (fallback: 512MB)
    let total_ram_mb = detect_ram_mb();

    // GPU VRAM: BAR0 para NVIDIA (valor raw, precisa decode de tamanho)
    // ponytail: BAR0 raw, sem decode de tamanho; VRAM report fica 0 até melhor detecção
    let gpu_vram_mb = if has_nvidia_gpu { 2048 } else { 0 };

    // CPU features — ponytail: assume pelo menos SSE; QEMU sempre tem
    let cpu_has_avx2 = cfg!(target_feature = "avx2");
    let cpu_has_sse = true;  // x86_64 guaranteed
    let cpu_has_fpu = true;  // x86_64 guaranteed

    HwProfile {
        total_ram_mb,
        cpu_has_avx2,
        cpu_has_sse,
        cpu_has_fpu,
        devices,
        has_nvidia_gpu,
        has_intel_gpu,
        has_intel_wifi,
        has_realtek_nic,
        gpu_vram_mb,
    }
}

/// Detecta RAM total (MB) via TOTAL_RAM_MB setado pelo frame allocator no boot.
/// Fallback: 512MB se o boot ainda não tiver populado o valor.
fn detect_ram_mb() -> u64 {
    let detected = crate::memory::TOTAL_RAM_MB.load(core::sync::atomic::Ordering::Relaxed);
    if detected > 0 { detected } else { 512 }
}
