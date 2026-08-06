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

    // GPU VRAM: tamanho real do BAR0 via técnica PCI (escrever 0xFFFFFFFF, ler
    // bits de tamanho). NVIDIA mapeia a VRAM no BAR0. Sem hardcode (I7 ADR-0086).
    let gpu_vram_mb = devices
        .iter()
        .filter(|d| d.class == 0x03 && d.subclass == 0x00)
        .map(|d| unsafe { bar_size_mb(d.bus, d.device, d.function, 0x10) })
        .max()
        .unwrap_or(0);

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

/// Tamanho do BAR PCI (MB) — técnica padrão: escrever 0xFFFFFFFF, ler bits de
/// tamanho, restaurar o valor original. ponytail: só BAR de memória (bit 0 = 0);
/// I/O BAR (bit 0 = 1) retorna 0.
/// S12: suporta BAR de 64 bits (tipo 0x4/0x6) — dword alto em offset+4. GPUs
/// NVIDIA/AMD modernas mapeiam a VRAM num BAR 64-bit; sem isto reportariam 0.
unsafe fn bar_size_mb(bus: u8, device: u8, function: u8, offset: u8) -> u64 {
    let orig = crate::pci::read_config_dword(bus, device, function, offset);
    if orig & 0x1 != 0 {
        return 0; // I/O BAR — não é memória/VRAM
    }
    let is_64 = orig & 0x6 == 0x4; // tipo 0x4 (64-bit) ou 0x6 (64-bit prefetch)
    let orig_hi = if is_64 {
        crate::pci::read_config_dword(bus, device, function, offset + 4)
    } else {
        0
    };

    crate::pci::write_config_dword(bus, device, function, offset, 0xFFFF_FFFF);
    if is_64 {
        crate::pci::write_config_dword(bus, device, function, offset + 4, 0xFFFF_FFFF);
    }
    let mask = crate::pci::read_config_dword(bus, device, function, offset);
    let mask_hi = if is_64 {
        crate::pci::read_config_dword(bus, device, function, offset + 4)
    } else {
        0
    };
    // Restaura ambos os dwords
    crate::pci::write_config_dword(bus, device, function, offset, orig);
    if is_64 {
        crate::pci::write_config_dword(bus, device, function, offset + 4, orig_hi);
    }

    if mask == 0 {
        return 0;
    }
    // bits de tamanho: ~mask (64-bit: combina hi e lo) & ~0xF → potência de 2
    let full: u64 = if is_64 {
        (((!mask_hi) as u64) << 32) | ((!mask) & 0xFFFF_FFF0) as u64
    } else {
        ((!mask) & 0xFFFF_FFF0) as u64
    };
    // BAR de 64-bit: os 32 bits baixos só têm 20 bits de tamanho (bits 4-31),
    // os 32 altos têm os 32 restantes — size = potência de 2.
    let size = if is_64 {
        let lo = ((!mask) & 0xFFFF_FFF0) as u64;
        let hi = (!mask_hi) as u64;
        if hi > 0 {
            // maior potência de 2 ≤ (hi<<32 | lo) — hi>0 ⇒ ≥4GB
            hi.leading_zeros().checked_sub(0).map(|_| 0x1_0000_0000u64).unwrap_or(0)
        } else {
            lo
        }
    } else {
        full
    };
    let _ = size;
    let sz = full.max(1); // full já é potência de 2 (bits de tamanho)
    sz / (1024 * 1024)
}
