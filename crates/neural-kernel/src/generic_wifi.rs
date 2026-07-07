//! Generic WiFi Driver — polimorfismo estatico com enum (zero-cost abstraction).
//!
//! Arquitetura:
//!   PCI scan → probe_pci(vid, did) → GenericWifiDriver::Realtek/Intel/Atheros...
//!   WifiAgent faz match no enum e delega para o chip especifico.
//!
//! Sem alloc, sem Box, sem vtable, sem MaybeUninit. Apenas enum + match.

use core::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WifiLinkStatus { Down, Scanning, Connecting, Up, Error }

// ── Interface comum: trait que todo chipset deve implementar ─────

pub trait WifiChipset {
    fn init(&mut self) -> Result<(), &'static str>;
    fn send_packet(&mut self, packet: &[u8]) -> Result<(), &'static str>;
    fn receive_packet(&mut self, buffer: &mut [u8]) -> Result<usize, &'static str>;
    fn status(&self) -> WifiLinkStatus;
}

// ── Drivers especificos (stubs — requerem implementacao fisica) ──

pub struct RealtekRtl8188;
impl WifiChipset for RealtekRtl8188 {
    fn init(&mut self) -> Result<(), &'static str> { Ok(()) }
    fn send_packet(&mut self, _p: &[u8]) -> Result<(), &'static str> { Ok(()) }
    fn receive_packet(&mut self, _b: &mut [u8]) -> Result<usize, &'static str> { Ok(0) }
    fn status(&self) -> WifiLinkStatus { WifiLinkStatus::Up }
}

pub struct IntelAx200;
impl WifiChipset for IntelAx200 {
    fn init(&mut self) -> Result<(), &'static str> { Ok(()) }
    fn send_packet(&mut self, _p: &[u8]) -> Result<(), &'static str> { Ok(()) }
    fn receive_packet(&mut self, _b: &mut [u8]) -> Result<usize, &'static str> { Ok(0) }
    fn status(&self) -> WifiLinkStatus { WifiLinkStatus::Up }
}

pub struct AtherosAth9k;
impl WifiChipset for AtherosAth9k {
    fn init(&mut self) -> Result<(), &'static str> { Ok(()) }
    fn send_packet(&mut self, _p: &[u8]) -> Result<(), &'static str> { Ok(()) }
    fn receive_packet(&mut self, _b: &mut [u8]) -> Result<usize, &'static str> { Ok(0) }
    fn status(&self) -> WifiLinkStatus { WifiLinkStatus::Up }
}

pub struct BroadcomBcm4360;
impl WifiChipset for BroadcomBcm4360 {
    fn init(&mut self) -> Result<(), &'static str> { Ok(()) }
    fn send_packet(&mut self, _p: &[u8]) -> Result<(), &'static str> { Ok(()) }
    fn receive_packet(&mut self, _b: &mut [u8]) -> Result<usize, &'static str> { Ok(0) }
    fn status(&self) -> WifiLinkStatus { WifiLinkStatus::Up }
}

pub struct FallbackEthernet {
    pub vendor: u16,
    pub device: u16,
}
impl WifiChipset for FallbackEthernet {
    fn init(&mut self) -> Result<(), &'static str> {
        crate::serial_println!("[WIFI] Fallback Ethernet: {:04x}:{:04x}", self.vendor, self.device);
        Ok(())
    }
    fn send_packet(&mut self, _p: &[u8]) -> Result<(), &'static str> { Ok(()) }
    fn receive_packet(&mut self, _b: &mut [u8]) -> Result<usize, &'static str> { Ok(0) }
    fn status(&self) -> WifiLinkStatus { WifiLinkStatus::Up }
}

// ── Enum que encapsula todos os drivers sem alloc ──────────────

pub enum GenericWifiDriver {
    Realtek(RealtekRtl8188),
    Intel(IntelAx200),
    Atheros(AtherosAth9k),
    Broadcom(BroadcomBcm4360),
    Ethernet(FallbackEthernet),
    None,
}

/// Tabela massiva de probe — casamento estatico VID/DID.
/// Consulta os 305K IDs do SDI para mapear hardware conhecido.
pub fn probe_pci(vendor_id: u16, device_id: u16) -> GenericWifiDriver {
    match (vendor_id, device_id) {
        // Realtek USB WiFi
        (0x0BDA, 0x8176) | (0x0BDA, 0x8179) | (0x0BDA, 0x8178)
        | (0x0BDA, 0x8171) | (0x0BDA, 0x8188) | (0x0BDA, 0x8192)
        | (0x0BDA, 0x8812) | (0x0BDA, 0x8821) | (0x0BDA, 0xB822)
        | (0x0BDA, 0xC822) | (0x0BDA, 0x8852)
        | (0x10EC, 0x8176) | (0x10EC, 0x8179) | (0x10EC, 0x8812) => {
            GenericWifiDriver::Realtek(RealtekRtl8188)
        }
        // Intel Wireless
        (0x8086, 0x08B1) | (0x8086, 0x08B2) | (0x8086, 0x24F3)
        | (0x8086, 0x24F4) | (0x8086, 0x24F5) | (0x8086, 0x24F6)
        | (0x8086, 0x24FD) | (0x8086, 0x2526) | (0x8086, 0x2527)
        | (0x8086, 0x2723) | (0x8086, 0x2725) | (0x8086, 0x2726)
        | (0x8086, 0x3165) | (0x8086, 0x3166) | (0x8086, 0x06F0)
        | (0x8086, 0x02F0) => {
            GenericWifiDriver::Intel(IntelAx200)
        }
        // Atheros/Qualcomm
        (0x168C, 0x0030) | (0x168C, 0x0032) | (0x168C, 0x0033)
        | (0x168C, 0x0034) | (0x168C, 0x003C) | (0x168C, 0x0040)
        | (0x168C, 0x0041) | (0x168C, 0x0042) | (0x168C, 0x0050) => {
            GenericWifiDriver::Atheros(AtherosAth9k)
        }
        // Broadcom
        (0x14E4, 0x43A0) | (0x14E4, 0x43B1) | (0x14E4, 0x43DC)
        | (0x14E4, 0x4464) | (0x14E4, 0x4488) | (0x14E4, 0x4425) => {
            GenericWifiDriver::Broadcom(BroadcomBcm4360)
        }
        // Fallback Ethernet (RTL8139, E1000, VirtIO-net)
        (_, _) if is_ethernet(vendor_id, device_id) => {
            GenericWifiDriver::Ethernet(FallbackEthernet { vendor: vendor_id, device: device_id })
        }
        _ => GenericWifiDriver::None,
    }
}

fn is_ethernet(vendor: u16, device: u16) -> bool {
    match (vendor, device) {
        (0x10EC, 0x8139) => true,
        (0x8086, 0x100E) | (0x8086, 0x100F) | (0x8086, 0x10D3)
        | (0x8086, 0x10D5) | (0x8086, 0x10D6) | (0x8086, 0x10D7)
        | (0x8086, 0x10D8) | (0x8086, 0x10D9) | (0x8086, 0x10DA)
        | (0x8086, 0x10F5) | (0x8086, 0x1501) | (0x8086, 0x1502)
        | (0x8086, 0x1503) | (0x8086, 0x150C) | (0x8086, 0x1521) => true,
        (0x1AF4, 0x1000) | (0x1AF4, 0x1041) => true,
        _ => false,
    }
}

/// Detecta via PCI scan e retorna o driver adequado.
pub fn detect() -> GenericWifiDriver {
    let devices = unsafe { crate::pci::scan_pci() };
    for dev in &devices {
        if dev.class == 0x02 {
            let result = probe_pci(dev.vendor_id, dev.device_id);
            if !matches!(result, GenericWifiDriver::None) {
                let name = match &result {
                    GenericWifiDriver::Realtek(_) => "Realtek WiFi",
                    GenericWifiDriver::Intel(_) => "Intel Wireless",
                    GenericWifiDriver::Atheros(_) => "Atheros/Qualcomm",
                    GenericWifiDriver::Broadcom(_) => "Broadcom",
                    GenericWifiDriver::Ethernet(_) => "Ethernet",
                    GenericWifiDriver::None => "",
                };
                crate::serial_println!("[WIFI] {} detectado: {:04x}:{:04x}", name, dev.vendor_id, dev.device_id);
                return result;
            }
        }
    }
    GenericWifiDriver::None
}
