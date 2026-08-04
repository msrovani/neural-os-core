//! HwCapabilityCard — contrato plug-and-play de hardware (AIOS).
//! HWID → family + firmware + regmap + agent + next_action.
//! Fonte curto prazo: tabelas/heurísticas. Médio prazo: HW Expert v4 no MESMO schema.
//! NÃO usar generate() free-text aqui.

use alloc::format;
use alloc::string::String;

/// Topic EventBus: um card serializado por dispositivo detectado a quente.
pub const TOPIC_HW_CAPABILITY: &str = "HW_CAPABILITY";
/// Topic: pedido de ação PnP (Hermes/agentes consomem).
pub const TOPIC_HW_PNP_ACTION: &str = "HW_PNP_ACTION";

/// Família operacional (não prosa).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HwFamily {
    Unknown = 0,
    IntelE1000 = 1,
    VirtioNet = 2,
    RealtekEth = 3,
    IntelIwlWifi = 4,
    RealtekWifi = 5,
    AtherosWifi = 6,
    BroadcomWifi = 7,
    NvidiaGpu = 8,
    IntelI915 = 9,
    AmdGpu = 10,
    QemuVga = 11,
    VirtioGpu = 12,
    UsbHostXhci = 13,
    IntelHda = 14,
    StorageAta = 15,
    PciBridge = 16,
}

/// Próximo passo honesto do plug-and-play (sem stub que inventa HW).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HwNextAction {
    Ready = 0,
    LoadFirmware = 1,
    BindNetwork = 2,
    BindWifiScan = 3,
    BindGpuCompute = 4,
    BindUsbHost = 5,
    BindAudio = 6,
    BindStorage = 7,
    ObserveOnly = 8,
}

/// Card estruturado — o que Hermes/Cortex precisam para USAR o HW.
#[derive(Debug, Clone)]
pub struct HwCapabilityCard {
    pub vid: u16,
    pub did: u16,
    pub class: u8,
    pub subclass: u8,
    pub name: String,
    pub family: HwFamily,
    pub agent: &'static str,
    pub firmware: Option<&'static str>,
    pub caps_bits: u32,
    pub next_action: HwNextAction,
    pub source: &'static str, // "table" | "heuristic" | "expert_v4"
    /// RegMap compacto (0 = ausente). Offsets úteis para net/wifi.
    pub reg_tx: u32,
    pub reg_rx: u32,
    pub reg_db_tx: u32,
    pub reg_db_rx: u32,
    pub ring_size: u16,
}

/// Bits de capability no card (estáveis para treino v4).
pub mod caps {
    pub const NET: u32 = 1 << 0;
    pub const WIFI: u32 = 1 << 1;
    pub const DISPLAY: u32 = 1 << 2;
    pub const COMPUTE: u32 = 1 << 3;
    pub const AUDIO: u32 = 1 << 4;
    pub const USB_HOST: u32 = 1 << 5;
    pub const STORAGE: u32 = 1 << 6;
    pub const NEEDS_FW: u32 = 1 << 7;
    pub const SCAN: u32 = 1 << 8;
    pub const CAPTURE: u32 = 1 << 9;
}

impl HwFamily {
    pub fn as_str(self) -> &'static str {
        match self {
            HwFamily::Unknown => "unknown",
            HwFamily::IntelE1000 => "intel_e1000",
            HwFamily::VirtioNet => "virtio_net",
            HwFamily::RealtekEth => "realtek_eth",
            HwFamily::IntelIwlWifi => "intel_iwlwifi",
            HwFamily::RealtekWifi => "realtek_wifi",
            HwFamily::AtherosWifi => "atheros_wifi",
            HwFamily::BroadcomWifi => "broadcom_wifi",
            HwFamily::NvidiaGpu => "nvidia_gpu",
            HwFamily::IntelI915 => "intel_i915",
            HwFamily::AmdGpu => "amd_gpu",
            HwFamily::QemuVga => "qemu_vga",
            HwFamily::VirtioGpu => "virtio_gpu",
            HwFamily::UsbHostXhci => "usb_xhci",
            HwFamily::IntelHda => "intel_hda",
            HwFamily::StorageAta => "storage_ata",
            HwFamily::PciBridge => "pci_bridge",
        }
    }
}

impl HwNextAction {
    pub fn as_str(self) -> &'static str {
        match self {
            HwNextAction::Ready => "ready",
            HwNextAction::LoadFirmware => "load_firmware",
            HwNextAction::BindNetwork => "bind_network",
            HwNextAction::BindWifiScan => "bind_wifi_scan",
            HwNextAction::BindGpuCompute => "bind_gpu_compute",
            HwNextAction::BindUsbHost => "bind_usb_host",
            HwNextAction::BindAudio => "bind_audio",
            HwNextAction::BindStorage => "bind_storage",
            HwNextAction::ObserveOnly => "observe_only",
        }
    }
}

impl HwCapabilityCard {
    /// Serialização compacta para EventBus (ASCII, uma linha).
    pub fn to_wire(&self) -> String {
        format!(
            "vid={:04X};did={:04X};cls={:02X};sub={:02X};family={};agent={};fw={};caps={:#x};next={};src={};name={}",
            self.vid,
            self.did,
            self.class,
            self.subclass,
            self.family.as_str(),
            self.agent,
            self.firmware.unwrap_or("-"),
            self.caps_bits,
            self.next_action.as_str(),
            self.source,
            self.name.replace(';', ","),
        )
    }

    pub fn log_line(&self) -> String {
        format!(
            "[HW-PnP] {:04X}:{:04X} {} family={} agent={} fw={} next={} caps={:#x} src={}",
            self.vid,
            self.did,
            self.name,
            self.family.as_str(),
            self.agent,
            self.firmware.unwrap_or("-"),
            self.next_action.as_str(),
            self.caps_bits,
            self.source,
        )
    }
}

/// Monta card a partir de PCI (tabela + HW Expert v4 + heurística).
/// Ordem de precedência:
///   1. Tabela direta HWID (curada; sempre vence — nunca deixar o ML sobrepor)
///   2. HW Expert v4 (ML multi-head, se carregado; cobre o que a tabela não tem)
///   3. Heurística por class/vendor (fallback)
pub fn build_card(
    vid: u16,
    did: u16,
    class: u8,
    subclass: u8,
    name: &str,
) -> HwCapabilityCard {
    // --- 1. Tabela direta HWID (plug-and-play conhecido) ---
    if let Some(c) = table_lookup(vid, did, class, subclass, name) {
        return c;
    }
    // --- 2. HW Expert v4 ML (se carregado) ---
    if cortex::cortex::hwexpert_v4_is_loaded() {
        if let Some(pred) = cortex::cortex::hwexpert_v4_predict(vid, did) {
            if let Some(c) = prediction_to_card(vid, did, class, subclass, name, &pred) {
                return c;
            }
        }
    }
    // --- 3. Heurística por class/vendor ---
    heuristic_card(vid, did, class, subclass, name)
}

/// Converte predição do HW Expert v4 para HwCapabilityCard.
/// Retorna None se a predição for inválida (family=unknown).
fn prediction_to_card(
    vid: u16, did: u16, class: u8, subclass: u8, name: &str,
    pred: &cortex::tensor::HwPrediction,
) -> Option<HwCapabilityCard> {
    if pred.family_id == 0 { return None; } // Unknown

    let family = match pred.family_id {
        1 => HwFamily::IntelE1000,
        2 => HwFamily::VirtioNet,
        3 => HwFamily::RealtekEth,
        4 => HwFamily::IntelIwlWifi,
        5 => HwFamily::RealtekWifi,
        6 => HwFamily::AtherosWifi,
        7 => HwFamily::BroadcomWifi,
        8 => HwFamily::NvidiaGpu,
        9 => HwFamily::IntelI915,
        10 => HwFamily::AmdGpu,
        11 => HwFamily::QemuVga,
        12 => HwFamily::VirtioGpu,
        13 => HwFamily::UsbHostXhci,
        14 => HwFamily::IntelHda,
        15 => HwFamily::StorageAta,
        16 => HwFamily::PciBridge,
        _ => return None,
    };

    let agent = match pred.agent_id {
        0 => "HwBridgeAgent",
        1 => "NetAgent",
        2 => "WifiAgent",
        3 => "DisplayAgent",
        4 => "GpuBackend",
        5 => "UsbDriverAgent",
        6 => "HdaAudioAgent",
        7 => "DiskAgent",
        8 => "PlatformAgent",
        _ => "PlatformAgent",
    };

    let firmware = match pred.fw_id {
        1 => Some("intel/iwlwifi"),
        2 => Some("rtlwifi"),
        3 => Some("ath9k"),
        4 => Some("brcmfmac"),
        5 => Some("nvidia/gp108"),
        6 => Some("i915"),
        7 => Some("amdgpu"),
        _ => None,
    };

    let next_action = match pred.next_action {
        1 => HwNextAction::LoadFirmware,
        2 => HwNextAction::BindNetwork,
        3 => HwNextAction::BindWifiScan,
        4 => HwNextAction::BindGpuCompute,
        5 => HwNextAction::BindUsbHost,
        6 => HwNextAction::BindAudio,
        7 => HwNextAction::BindStorage,
        8 => HwNextAction::ObserveOnly,
        _ => HwNextAction::Ready,
    };

    Some(HwCapabilityCard {
        vid, did, class, subclass,
        name: alloc::string::String::from(name),
        family, agent, firmware,
        caps_bits: pred.caps_bits,
        next_action, source: "expert_v4",
        reg_tx: 0, reg_rx: 0, reg_db_tx: 0, reg_db_rx: 0, ring_size: 0,
    })
}

fn table_lookup(
    vid: u16,
    did: u16,
    class: u8,
    subclass: u8,
    name: &str,
) -> Option<HwCapabilityCard> {
    use caps::*;
    match (vid, did) {
        (0x8086, 0x100E) | (0x8086, 0x100F) | (0x8086, 0x10D3)
        | (0x8086, 0x1502) | (0x8086, 0x1503) => Some(HwCapabilityCard {
            vid, did, class, subclass,
            name: String::from(name),
            family: HwFamily::IntelE1000,
            agent: "NetAgent",
            firmware: None,
            caps_bits: NET,
            next_action: HwNextAction::BindNetwork,
            source: "table",
            reg_tx: 0x1000, reg_rx: 0x1004, reg_db_tx: 0x2000, reg_db_rx: 0x2004, ring_size: 64,
        }),
        (0x1AF4, 0x1000) | (0x1AF4, 0x1041) => Some(HwCapabilityCard {
            vid, did, class, subclass,
            name: String::from(name),
            family: HwFamily::VirtioNet,
            agent: "NetAgent",
            firmware: None,
            caps_bits: NET,
            next_action: HwNextAction::BindNetwork,
            source: "table",
            reg_tx: 0, reg_rx: 0, reg_db_tx: 0, reg_db_rx: 0, ring_size: 0,
        }),
        (0x10EC, 0x8139) => Some(HwCapabilityCard {
            vid, did, class, subclass,
            name: String::from(name),
            family: HwFamily::RealtekEth,
            agent: "NetAgent",
            firmware: None,
            caps_bits: NET,
            next_action: HwNextAction::BindNetwork,
            source: "table",
            reg_tx: 0x20, reg_rx: 0x30, reg_db_tx: 0, reg_db_rx: 0, ring_size: 4,
        }),
        (0x1234, 0x1111) => Some(HwCapabilityCard {
            vid, did, class, subclass,
            name: String::from(name),
            family: HwFamily::QemuVga,
            agent: "DisplayAgent",
            firmware: None,
            caps_bits: DISPLAY,
            next_action: HwNextAction::Ready,
            source: "table",
            reg_tx: 0, reg_rx: 0, reg_db_tx: 0, reg_db_rx: 0, ring_size: 0,
        }),
        (0x1AF4, 0x1050) => Some(HwCapabilityCard {
            vid, did, class, subclass,
            name: String::from(name),
            family: HwFamily::VirtioGpu,
            agent: "DisplayAgent",
            firmware: None,
            caps_bits: DISPLAY | COMPUTE,
            next_action: HwNextAction::Ready,
            source: "table",
            reg_tx: 0, reg_rx: 0, reg_db_tx: 0, reg_db_rx: 0, ring_size: 0,
        }),
        // Intel WiFi AX/AC comuns
        (0x8086, 0x2723) | (0x8086, 0x2725) | (0x8086, 0x2726)
        | (0x8086, 0x06F0) | (0x8086, 0x02F0) | (0x8086, 0x24FD) => Some(HwCapabilityCard {
            vid, did, class, subclass,
            name: String::from(name),
            family: HwFamily::IntelIwlWifi,
            agent: "WifiAgent",
            firmware: Some("intel/iwlwifi"),
            caps_bits: WIFI | NET | NEEDS_FW | SCAN,
            next_action: HwNextAction::LoadFirmware,
            source: "table",
            reg_tx: 0x1000, reg_rx: 0x1004, reg_db_tx: 0x2000, reg_db_rx: 0x2004, ring_size: 64,
        }),
        // QCA6174 / Note 1050 — ath10k (ADR-0056 golden; NÃO ath9k)
        (0x168C, 0x003E) | (0x168C, 0x0041) => Some(HwCapabilityCard {
            vid, did, class, subclass,
            name: String::from(name),
            family: HwFamily::AtherosWifi,
            agent: "WifiAgent",
            firmware: Some("ath10k/QCA6174"),
            caps_bits: WIFI | NET | NEEDS_FW | SCAN,
            next_action: recipe_next_or(vid, did, HwNextAction::LoadFirmware),
            source: "table",
            reg_tx: 0, reg_rx: 0, reg_db_tx: 0, reg_db_rx: 0, ring_size: 0,
        }),
        _ => None,
    }
}

/// Consulta DeviceRecipe (k-hal): NeedsFw → LoadFirmware; Ok → BindWifiScan (≠ RF Ready).
fn recipe_next_or(vid: u16, did: u16, fallback: HwNextAction) -> HwNextAction {
    use k_hal::device_cap::DeviceClass;
    use k_hal::device_recipe::{evaluate_device, RecipePromote};
    match evaluate_device(vid, did, DeviceClass::Wifi) {
        RecipePromote::NeedsFw => HwNextAction::LoadFirmware,
        RecipePromote::Ok => HwNextAction::BindWifiScan,
        RecipePromote::Escalate => HwNextAction::ObserveOnly,
        RecipePromote::None => fallback,
    }
}

fn heuristic_card(
    vid: u16,
    did: u16,
    class: u8,
    subclass: u8,
    name: &str,
) -> HwCapabilityCard {
    use caps::*;
    let (family, agent, fw, bits, next, reg) = match class {
        0x02 => {
            // Network — Ethernet vs WiFi by subclass / vendor
            if subclass == 0x80 || class == 0x0D {
                wifi_heuristic(vid)
            } else if vid == 0x8086 {
                (HwFamily::IntelE1000, "NetAgent", None, NET, HwNextAction::BindNetwork,
                 (0x1000, 0x1004, 0x2000, 0x2004, 32u16))
            } else if vid == 0x10EC || vid == 0x0BDA {
                (HwFamily::RealtekEth, "NetAgent", None, NET, HwNextAction::BindNetwork,
                 (0xA0, 0xA4, 0xD0, 0xD4, 16u16))
            } else {
                (HwFamily::Unknown, "NetAgent", None, NET, HwNextAction::BindNetwork,
                 (0, 0, 0, 0, 0u16))
            }
        }
        0x0D => wifi_heuristic(vid),
        0x03 => match vid {
            0x10DE => (HwFamily::NvidiaGpu, "GpuBackend", Some("nvidia/gp108"),
                       DISPLAY | COMPUTE | NEEDS_FW, HwNextAction::LoadFirmware, (0, 0, 0, 0, 0u16)),
            0x8086 => (HwFamily::IntelI915, "DisplayAgent", Some("i915"),
                       DISPLAY | COMPUTE | NEEDS_FW, HwNextAction::LoadFirmware, (0, 0, 0, 0, 0u16)),
            0x1002 => (HwFamily::AmdGpu, "GpuBackend", Some("amdgpu"),
                       DISPLAY | COMPUTE | NEEDS_FW, HwNextAction::LoadFirmware, (0, 0, 0, 0, 0u16)),
            _ => (HwFamily::Unknown, "DisplayAgent", None, DISPLAY, HwNextAction::Ready, (0, 0, 0, 0, 0u16)),
        },
        0x04 => (HwFamily::IntelHda, "HdaAudioAgent", None, AUDIO, HwNextAction::BindAudio, (0, 0, 0, 0, 0u16)),
        0x0C if subclass == 0x03 => (
            HwFamily::UsbHostXhci, "UsbDriverAgent", None,
            USB_HOST | CAPTURE, HwNextAction::BindUsbHost, (0, 0, 0, 0, 0u16),
        ),
        0x01 => (HwFamily::StorageAta, "DiskAgent", None, STORAGE, HwNextAction::BindStorage, (0, 0, 0, 0, 0u16)),
        0x06 => (HwFamily::PciBridge, "PlatformAgent", None, 0, HwNextAction::ObserveOnly, (0, 0, 0, 0, 0u16)),
        _ => (HwFamily::Unknown, "HwBridgeAgent", None, 0, HwNextAction::ObserveOnly, (0, 0, 0, 0, 0u16)),
    };

    HwCapabilityCard {
        vid, did, class, subclass,
        name: String::from(name),
        family,
        agent,
        firmware: fw,
        caps_bits: bits,
        next_action: next,
        source: "heuristic",
        reg_tx: reg.0, reg_rx: reg.1, reg_db_tx: reg.2, reg_db_rx: reg.3, ring_size: reg.4,
    }
}

fn wifi_heuristic(vid: u16) -> (HwFamily, &'static str, Option<&'static str>, u32, HwNextAction, (u32, u32, u32, u32, u16)) {
    use caps::*;
    match vid {
        0x8086 => (HwFamily::IntelIwlWifi, "WifiAgent", Some("intel/iwlwifi"),
                   WIFI | NET | NEEDS_FW | SCAN, HwNextAction::LoadFirmware,
                   (0x1000, 0x1004, 0x2000, 0x2004, 64)),
        0x10EC | 0x0BDA => (HwFamily::RealtekWifi, "WifiAgent", Some("rtlwifi"),
                   WIFI | NET | NEEDS_FW | SCAN, HwNextAction::LoadFirmware,
                   (0xA0, 0xA4, 0xD0, 0xD4, 16)),
        // Atheros: default ath10k (QCA61x4); ath9k SoftMAC = recipe dedicada futura
        0x168C => (HwFamily::AtherosWifi, "WifiAgent", Some("ath10k"),
                   WIFI | NET | NEEDS_FW | SCAN, HwNextAction::LoadFirmware,
                   (0, 0, 0, 0, 0)),
        0x14E4 => (HwFamily::BroadcomWifi, "WifiAgent", Some("brcmfmac"),
                   WIFI | NET | NEEDS_FW | SCAN, HwNextAction::LoadFirmware,
                   (0x500, 0x504, 0x600, 0x604, 32)),
        _ => (HwFamily::Unknown, "WifiAgent", None,
              WIFI | SCAN, HwNextAction::BindWifiScan, (0, 0, 0, 0, 0)),
    }
}

/// Labels estáveis para treino HW Expert v4 (mesma ordem / IDs do enum).
pub fn v4_label_names() -> &'static [&'static str] {
    &[
        "family", "fw_id", "agent_id", "caps_bits", "next_action",
    ]
}
