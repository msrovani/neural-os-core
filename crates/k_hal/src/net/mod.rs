//! Net/WiFi BE — MMIO drivers (ADR-0041 H3).
//! hermes NetAgent/WifiAgent = FE (netstack/smoltcp); BAR vive aqui.

pub mod generic_wifi;
pub mod wifi_compat;
pub mod wifi_crypto;
pub mod wifi_iwlwifi;
pub mod wifi_msix;

use crate::device_cap::DeviceClass;
use crate::discovery;
use crate::net_port::{self, NetPortStatus};
/// Registra net/wifi bound no DeviceTree (chamado após probe).
pub fn register_net_bound(bus: u8, dev: u8, func: u8, wifi: bool) {
    discovery::mark_bound(bus, dev, func, true);
    net_port::set_status(NetPortStatus::Bound);
    k_nano::slog_hal!("NET", "info", "bound bus={}:{}:{} class={}",
        bus,
        dev,
        func,
        if wifi {
        DeviceClass::Wifi.as_str()
        } else {
        DeviceClass::Net.as_str()
        });
}

pub fn set_link_up() {
    net_port::set_status(NetPortStatus::Up);
}
