//! Link Watcher — saude eth/wifi, failover, histerese (FE only — ADR-0041).
//! Sem MMIO: status via HalOffer / drivers já bound no k-hal.

use k_hal::device_cap::DeviceClass;
use k_hal::offer::{self, OfferStatus};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LinkStatus {
    Up,
    Down,
    Degraded(i8),
}

pub trait NetworkInterface {
    fn name(&self) -> &'static str;
    fn check_health(&mut self) -> LinkStatus;
    fn set_active(&mut self, active: bool);
}

pub struct EthInterface;
impl NetworkInterface for EthInterface {
    fn name(&self) -> &'static str {
        "eth0"
    }
    fn check_health(&mut self) -> LinkStatus {
        let ok = crate::net::RTL8139.lock().is_some()
            || crate::net::E1000.lock().is_some()
            || crate::net::VIRTIO_DEV.lock().is_some()
            || matches!(
                offer::query(DeviceClass::Net),
                OfferStatus::Available | OfferStatus::Bound
            );
        if ok {
            LinkStatus::Up
        } else {
            LinkStatus::Down
        }
    }
    fn set_active(&mut self, _a: bool) {}
}

pub struct WlanInterface {
    pub rssi: i8,
    pub drops: u32,
}
impl NetworkInterface for WlanInterface {
    fn name(&self) -> &'static str {
        "wlan0"
    }
    fn check_health(&mut self) -> LinkStatus {
        if self.drops > 10 {
            return LinkStatus::Down;
        }
        // RSSI real viria do BE k-hal via telemetria HalOffer — sem BAR no R3
        match offer::query(DeviceClass::Wifi) {
            OfferStatus::Bound | OfferStatus::Available => {
                self.rssi = -60; // placeholder honesto até telemetria FE
                LinkStatus::Up
            }
            _ => LinkStatus::Down,
        }
    }
    fn set_active(&mut self, _a: bool) {}
}

#[derive(Debug, Clone, Copy)]
pub struct WifiProfile {
    pub ssid: &'static str,
    pub prio: u8,
}

pub struct LinkWatcher {
    pub active: usize,
    pub deg_count: u32,
    pub profile_idx: usize,
    pub profiles: [Option<WifiProfile>; 4],
}

impl LinkWatcher {
    pub fn new() -> Self {
        LinkWatcher {
            active: 0,
            deg_count: 0,
            profile_idx: 0,
            profiles: [None; 4],
        }
    }

    pub fn tick(&mut self, eth: &mut EthInterface, wlan: &mut WlanInterface) {
        let es = eth.check_health();
        let ws = wlan.check_health();
        let wlan_recovered = matches!(ws, LinkStatus::Up);

        if self.active == 0 {
            if matches!(es, LinkStatus::Down | LinkStatus::Degraded(_)) {
                if matches!(ws, LinkStatus::Up) {
                    self.switch_to(1, eth, wlan);
                }
            }
        } else if self.active == 1 {
            match ws {
                LinkStatus::Down => {
                    if matches!(es, LinkStatus::Up) {
                        self.switch_to(0, eth, wlan);
                    } else {
                        self.switch_to_next_wifi(wlan);
                    }
                }
                LinkStatus::Degraded(_) => {
                    self.deg_count = self.deg_count.saturating_add(1);
                    if self.deg_count > 5 && matches!(es, LinkStatus::Up) {
                        self.switch_to(0, eth, wlan);
                    }
                }
                LinkStatus::Up => {
                    self.deg_count = 0;
                }
            }
        } else if self.active == 0 && matches!(es, LinkStatus::Up) && wlan_recovered {
            // politica: manter Ethernet
        }
    }

    fn switch_to(&mut self, target: usize, eth: &mut EthInterface, wlan: &mut WlanInterface) {
        self.active = target;
        eth.set_active(target == 0);
        wlan.set_active(target == 1);
        self.deg_count = 0;
        if target == 0 {
            k_nano::slog_hermes!("LINK", "info", "Switch para Ethernet");
        } else {
            k_nano::slog_hermes!("LINK", "info", "Switch para WiFi");
        }
    }

    fn switch_to_next_wifi(&mut self, wlan: &mut WlanInterface) {
        let next = (self.profile_idx + 1) % 4;
        if self.profiles[next].is_some() {
            self.profile_idx = next;
            self.switch_to(1, &mut EthInterface, wlan);
            // Reassoc: pedido via HalOffer Wifi FE — sem MMIO cold
            let _ = crate::hal_offer::request_device(DeviceClass::Wifi, "wifi");
            k_nano::slog_hermes!("LINK", "wifi", "reassoc via HalOffer (sem MMIO R3)");
        }
    }
}
