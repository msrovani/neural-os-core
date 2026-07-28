//! NetFs Fallback — busca firmware ausente na rede quando HW muda (ADR-0079 M4).
//! Se o HW detectado tem dispositivos cujo firmware não está em disco,
//! tenta baixar do mirror HTTP (NetFs, GitHub, HF `aios-k2chj`).

use alloc::string::String;
use alloc::format;
use alloc::vec::Vec;

/// Resultado da busca de firmware.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FirmwareFetchResult {
    Found { name: String, size: usize },
    NotFound(String),
    NetworkError(&'static str),
    Skipped,
}

/// Tenta baixar firmware ausente para um dispositivo detectado.
/// `hw_name` — ex: "i915", "iwlwifi-ax201", "rtl_nic"
pub fn fetch_missing_firmware(hw_name: &str) -> FirmwareFetchResult {
    // 1. Tenta mirror local (NetFs peer)
    if let Some(result) = try_netfs(hw_name) {
        return result;
    }

    // 2. Tenta GitHub releases
    if let Some(result) = try_github(hw_name) {
        return result;
    }

    // 3. Tenta HuggingFace dataset aios-k2chj
    if let Some(result) = try_huggingface(hw_name) {
        return result;
    }

    FirmwareFetchResult::NotFound(format!("No source for firmware: {}", hw_name))
}

/// Tenta buscar via NetFs (peer local na LAN).
fn try_netfs(_hw_name: &str) -> Option<FirmwareFetchResult> {
    // ponytail: NetFs peer lookup stub — implementar quando NetFs estiver ativo
    None
}

/// Tenta buscar via GitHub API.
fn try_github(hw_name: &str) -> Option<FirmwareFetchResult> {
    // ponytail: HTTP GET via smoltcp + DNS
    // Ex: https://raw.githubusercontent.com/msrovani/neural-os-core/main/firmware/FW_I915_*.BIN
    let _ = hw_name;
    None
}

/// Tenta buscar via HuggingFace (aios-k2chj dataset).
fn try_huggingface(hw_name: &str) -> Option<FirmwareFetchResult> {
    // ponytail: HuggingFace download via HTTPS (requer TLS)
    let _ = hw_name;
    None
}

/// Mapeia dispositivo PCI para nome de firmware conhecido.
pub fn hw_to_firmware_name(vendor_id: u16, device_id: u16, class: u8, subclass: u8) -> Option<&'static str> {
    match (class, subclass, vendor_id) {
        (0x03, 0x00, 0x10DE) => Some("nvidia"),       // NVIDIA GPU
        (0x03, 0x00, 0x8086) => Some("i915"),          // Intel GPU
        (0x02, 0x00, 0x10EC) => Some("rtl_nic"),       // Realtek NIC
        (0x02, 0x80, 0x8086) => Some("iwlwifi"),       // Intel WiFi
        (0x02, 0x00, 0x8086) => Some("e1000"),         // Intel NIC
        _ => None,
    }
}
