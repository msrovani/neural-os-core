//! Net/WiFi BE â€” MMIO drivers (ADR-0041 H3).
//! hermes NetAgent/WifiAgent = FE (netstack/smoltcp); BAR vive aqui.

pub mod ath10k_ce_bmi;
pub mod ath10k_fw;
pub mod ath10k_htc_wmi;
pub mod ath10k_wmi_scan;
pub mod ath10k_wmi_assoc;
pub mod generic_wifi;
pub mod iwl_fw;
pub mod wifi_ath10k;
pub mod wifi_compat;
pub mod wifi_crypto;
pub mod wifi_iwlwifi;
pub mod wifi_msix;
pub mod wifi_softmac;

use crate::device_cap::DeviceClass;
use crate::discovery;
use crate::net_port::{self, NetPortStatus};
/// Registra net/wifi bound no DeviceTree (chamado apÃ³s probe).
pub fn register_net_bound(bus: u8, dev: u8, func: u8, wifi: bool) {
    discovery::mark_bound(bus, dev, func, true);
    net_port::set_status(NetPortStatus::Bound);
    k_nano::slog_hal_home!(
        "NET",
        "ok",
        "k_hal::net",
        "bound bus={}:{}:{} class={}",
        bus,
        dev,
        func,
        if wifi {
            DeviceClass::Wifi.as_str()
        } else {
            DeviceClass::Net.as_str()
        }
    );
}

pub fn set_link_up() {
    net_port::set_status(NetPortStatus::Up);
}

// ── NIC MMIO backends (FASE B migration from k_nano) ──

pub mod e1000;
pub mod rtl8139;
pub mod i225;
pub mod virtio_net;

// Re-export detection functions for k_nano nic_globals
pub use crate::net::e1000::{is_e1000_family, E1000_VENDOR_INTEL};
pub use crate::net::i225::{is_i225_family, I225_VENDOR_INTEL};
