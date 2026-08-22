//! ADR-0090 Tier 4 - Multi-Monitor via EDID
//!
//! EDID 1.3/1.4 parser for monitor detection via I2C/DDC.
//! Gate: AWAITING_HW (QEMU does not emulate DDC/I2C).

use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone)]
pub struct EdidInfo {
    pub manufacturer: [char; 3],
    pub product_code: u16,
    pub serial: u32,
    pub name: String,
    pub preferred_width: u16,
    pub preferred_height: u16,
    pub max_width: u16,
    pub max_height: u16,
    pub refresh_hz: u8,
    pub is_connected: bool,
}

impl EdidInfo {
    pub fn unknown() -> Self {
        Self {
            manufacturer: ['?', '?', '?'], product_code: 0, serial: 0,
            name: String::from("Unknown"), preferred_width: 0, preferred_height: 0,
            max_width: 0, max_height: 0, refresh_hz: 60, is_connected: false,
        }
    }
    pub fn manufacturer_str(&self) -> String {
        String::from_iter(self.manufacturer.iter())
    }
}

pub fn parse_edid(data: &[u8]) -> Option<EdidInfo> {
    if data.len() < 128 { return None; }
    if &data[0..8] != &[0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00] { return None; }
    let mfg_raw = u16::from_be_bytes([data[8], data[9]]);
    let m1 = ((mfg_raw >> 10) & 0x1F) as u8;
    let m2 = ((mfg_raw >> 5) & 0x1F) as u8;
    let m3 = (mfg_raw & 0x1F) as u8;
    let manufacturer = [
        (b'A' + m1 - 1) as char,
        (b'A' + m2 - 1) as char,
        (b'A' + m3 - 1) as char,
    ];
    let product_code = u16::from_le_bytes([data[10], data[11]]);
    let serial = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
    let mut name = String::from("Monitor");
    let mut preferred_w: u16 = 0;
    let mut preferred_h: u16 = 0;
    if data[54] != 0 || data[55] != 0 {
        preferred_w = data[58] as u16 | ((data[56] as u16 & 0xF0) << 4);
        preferred_h = data[61] as u16 | ((data[59] as u16 & 0xF0) << 4);
    }
    for desc_start in (72..126).step_by(18) {
        if desc_start + 18 > data.len() { break; }
        if data[desc_start] == 0x00 && data[desc_start + 1] == 0x00 {
            if data[desc_start + 3] == 0xFC {
                let mut n = String::new();
                for i in 0..13 {
                    let ch = data[desc_start + 5 + i];
                    if ch == 0x0A || ch == 0x00 { break; }
                    n.push(ch as char);
                }
                name = n;
            }
        }
    }
    Some(EdidInfo {
        manufacturer, product_code, serial, name,
        preferred_width: preferred_w, preferred_height: preferred_h,
        max_width: preferred_w, max_height: preferred_h,
        refresh_hz: 60, is_connected: true,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorType { Hdmi, DisplayPort, Vga, Dvi, Embedded }

#[derive(Debug, Clone)]
pub struct MonitorOutput {
    pub index: usize,
    pub connector_type: ConnectorType,
    pub edid: Option<EdidInfo>,
    pub active: bool,
    pub resolution: (u16, u16),
}

pub struct MultiMonitorState {
    pub outputs: Vec<MonitorOutput>,
    pub primary: usize,
}

impl MultiMonitorState {
    pub fn new() -> Self {
        let mut outputs = Vec::new();
        outputs.push(MonitorOutput {
            index: 0, connector_type: ConnectorType::Embedded,
            edid: None, active: true, resolution: (1280, 800),
        });
        Self { outputs, primary: 0 }
    }

    pub fn detect(&mut self) {
        k_nano::slog_jarbas!("EDID", "detect", "outputs: {} (AWAITING_HW DDC)", self.outputs.len());
    }

    pub fn primary_output(&self) -> &MonitorOutput {
        &self.outputs[self.primary.min(self.outputs.len().saturating_sub(1))]
    }

    pub fn primary_resolution(&self) -> (u16, u16) {
        self.primary_output().resolution
    }
}

pub static MULTI_MONITOR: spin::Mutex<MultiMonitorState> = spin::Mutex::new(MultiMonitorState {
    outputs: Vec::new(), primary: 0,
});
