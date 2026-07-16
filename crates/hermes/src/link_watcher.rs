//! Link Watcher — monitoramento de saude, failover automatico, histerese.
//! Gerencia dual WiFi+Ethernet: decide qual interface roteia trafego IP.
//! Histerese: evita flapping com janela temporal e margem de recuperacao.

use core::ptr::{read_volatile, write_volatile};

// ── 1. STATUS DE LINK ─────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LinkStatus { Up, Down, Degraded(i8) }

pub trait NetworkInterface {
    fn name(&self) -> &'static str;
    fn check_health(&mut self) -> LinkStatus;
    fn set_active(&mut self, active: bool);
}

// ── 2. INTERFACES CONCRETAS ──────────────────────────────────

pub struct EthInterface;
impl NetworkInterface for EthInterface {
    fn name(&self) -> &'static str { "eth0" }
    fn check_health(&mut self) -> LinkStatus {
        let ok = crate::net::RTL8139.lock().is_some()
            || crate::net::E1000.lock().is_some()
            || crate::net::VIRTIO_DEV.lock().is_some();
        if ok { LinkStatus::Up } else { LinkStatus::Down }
    }
    fn set_active(&mut self, _a: bool) {}
}

pub struct WlanInterface {
    pub rssi: i8,
    pub drops: u32,
}
impl NetworkInterface for WlanInterface {
    fn name(&self) -> &'static str { "wlan0" }
    fn check_health(&mut self) -> LinkStatus {
        if self.drops > 10 { return LinkStatus::Down; }
        // Le RSSI do registrador do chip WiFi (offset 0x90)
        let rssi = unsafe { read_volatile((0x10000000 + 0x90) as *const u32) };
        self.rssi = (rssi & 0xFF) as i8;
        if self.rssi < -85 { LinkStatus::Degraded(self.rssi) } else { LinkStatus::Up }
    }
    fn set_active(&mut self, _a: bool) {}
}

// ── 3. PERFIS WIFI DE BACKUP ─────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct WifiProfile {
    pub ssid: [u8; 32], pub ssid_len: usize,
    pub pass: [u8; 64], pub pass_len: usize,
}

// ── 4. MOTOR DE FAILOVER COM HISTERESE ───────────────────────

pub struct FailoverEngine {
    pub profiles: [Option<WifiProfile>; 4],
    pub profile_idx: usize,
    pub active: usize,          // 0=eth, 1=wlan
    pub deg_count: u32,         // contagem de degradacoes consecutivas
    pub last_rssi: i8,          // RSSI na ultima medicao
}

impl FailoverEngine {
    pub const fn new() -> Self {
        Self {
            profiles: [None, None, None, None],
            profile_idx: 0, active: 0, deg_count: 0, last_rssi: 0,
        }
    }

    /// Monitora e executa fallback se necessario.
    /// Chamado ciclicamente pelo timer do kernel.
    pub fn tick(&mut self, eth: &mut EthInterface, wlan: &mut WlanInterface) {
        let es = eth.check_health();
        let ws = wlan.check_health();

        // Histerese: so considera Down apos 5 medicoes consecutivas ruins
        if ws == LinkStatus::Down || matches!(ws, LinkStatus::Degraded(_)) {
            self.deg_count += 1;
        } else {
            self.deg_count = 0;
        }

        let wlan_dead = self.deg_count >= 5;
        let wlan_recovered = wlan.rssi > -75; // margem de recuperacao

        // Logica de decisao
        if self.active == 1 && es == LinkStatus::Up {
            // WiFi ativo, Ethernet conectada → migra para Ethernet
            self.switch_to(0, eth, wlan);
        } else if self.active == 1 && wlan_dead {
            // WiFi morreu → tenta Ethernet
            if es == LinkStatus::Up {
                self.switch_to(0, eth, wlan);
            } else {
                self.switch_to_next_wifi(wlan);
            }
        } else if self.active == 0 && ws == LinkStatus::Up && wlan_recovered {
            // Ethernet ativo, WiFi recuperou → opcional: migrar de volta
            // (politica: manter Ethernet se estiver Up)
        }
    }

    fn switch_to(&mut self, target: usize, eth: &mut EthInterface, wlan: &mut WlanInterface) {
        self.active = target;
        eth.set_active(target == 0);
        wlan.set_active(target == 1);
        self.deg_count = 0;
        if target == 0 {
            k_nano::serial_println!("[LINK] Switch para Ethernet");
        } else {
            k_nano::serial_println!("[LINK] Switch para WiFi");
        }
    }

    fn switch_to_next_wifi(&mut self, _wlan: &mut WlanInterface) {
        let next = (self.profile_idx + 1) % 4;
        if self.profiles[next].is_some() {
            self.profile_idx = next;
            self.switch_to(1, &mut EthInterface, _wlan);
            // Comando de reassociacao via register MMIO
            unsafe { write_volatile(0x1000000C as *mut u32, 0x02); }
        }
    }
}
