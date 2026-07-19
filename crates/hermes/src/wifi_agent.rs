//! WiFi Agent — gerencia scan, seleção, senha, conexão, persistência, dual-network.
//! Fluxo completo: detecta WiFi → scan → lista redes → usuário escolhe → senha → conecta
//! → salva credenciais → notifica Hermes/Cortex → gerencia dual Ethernet+WiFi.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use event_bus::{CapabilityToken, Event, Receiver};
use alloc::string::String;
use alloc::vec::Vec;
use alloc::vec;
use crate::generic_wifi::{self, ACTIVE_DRIVER};
use core::sync::atomic::Ordering;
const WIFI_MANIFEST: AgentManifest = AgentManifest {
    name: "wifi_agent",
    kind: AgentKind::Network,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

// ── Tipos de rede ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AccessPoint {
    pub ssid: String,
    pub bssid: [u8; 6],
    pub signal_dbm: i32,
    pub channel: u8,
    pub security: SecurityType,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SecurityType { Open, WEP, WPA2, WPA3 }

impl AccessPoint {
    fn bars(&self) -> &'static str {
        if self.signal_dbm > -50 { "▂▄▆█" }
        else if self.signal_dbm > -65 { "▂▄▆" }
        else if self.signal_dbm > -80 { "▂▄" }
        else { "▂" }
    }
    fn lock(&self) -> &'static str {
        match self.security { SecurityType::Open => "🔓", _ => "🔒" }
    }
}

// ── Estado do agente ───────────────────────────────────────────

#[derive(Clone)]
enum WifiState {
    Idle,
    Scanning,
    ScanDone(Vec<AccessPoint>),
    AwaitingChoice,
    AwaitingPassword { ap: AccessPoint },
    Connecting,
    Connected { ssid: String },
    Failed(&'static str),
}

pub struct WifiAgent {
    state: WifiState,
    user_receiver: Receiver,
    hermes_receiver: Receiver,
    credentials_saved: bool,
}

impl WifiAgent {
    pub fn new() -> Self {
        // S0/S1-prep: probe no registro (antes do scheduler) — QEMU sem RF + evidência
        // serial mesmo se NetAgent falhar depois.
        if generic_wifi::detect_wifi() {
            k_nano::slog_bin!(
                "WIFI-HW",
                "info",
                "step=boot_probe status=PRESENT detail=pci_wifi_bound"
            );
            k_nano::slog_bin!(
                "WIFI-HW",
                "info",
                "VERDICT=AWAITING_REAL_HW reason=iwlwifi_scan_unwired_onda7"
            );
        } else {
            k_nano::slog_bin!(
                "WIFI-HW",
                "info",
                "step=boot_probe status=UNSUPPORTED detail=no_wifi_pci"
            );
            k_nano::slog_bin!(
                "WIFI-HW",
                "info",
                "VERDICT=AWAITING_REAL_HW reason=no_wifi_radio_onda7"
            );
        }
        WifiAgent {
            state: WifiState::Idle,
            user_receiver: k_nano::EVENT_BUS.subscribe("USER_INTENT"),
            hermes_receiver: k_nano::EVENT_BUS.subscribe("HERMES_COMMAND"),
            credentials_saved: false,
        }
    }

    fn publish(&self, text: &str) {
        let _ = k_nano::EVENT_BUS.publish(Event {
            id: 0, topic: alloc::string::String::from("HERMES_RESPONSE"),
            payload: alloc::format!("[WIFI] {}", text).into_bytes(),
            token: CapabilityToken::Legacy(1),
        });
    }

    fn notify_hermes_available(&self) {
        // Publica evento para Hermes/Cortex saberem que WiFi esta disponivel
        let _ = k_nano::EVENT_BUS.publish(Event {
            id: 0, topic: alloc::string::String::from("NET_IFACE_AVAILABLE"),
            payload: b"wifi".to_vec(),
            token: CapabilityToken::Legacy(1),
        });
    }

    // ── Scan ──────────────────────────────────────────────────

    fn do_scan(&mut self) {
        self.state = WifiState::Scanning;
        self.publish("Escaneando redes WiFi...");

        // FE: registra inject smoltcp; MMIO BE = k-hal
        k_hal::net::wifi_msix::set_rx_inject(crate::netstack::inject_rx_packet);
        if !generic_wifi::detect_wifi() {
            self.state = WifiState::Failed("Sem WiFi");
            self.publish("Sem WiFi. Ethernet cabeada ativa.");
            k_nano::slog_bin!(
                "WIFI-HW",
                "info",
                "step=detect status=UNSUPPORTED detail=no_wifi_pci"
            );
            k_nano::slog_bin!(
                "WIFI-HW",
                "info",
                "VERDICT=AWAITING_REAL_HW reason=no_wifi_radio_onda7"
            );
            return;
        }

        // Inicializa driver
        ACTIVE_DRIVER.lock(|driver| {
            if let Some(wifi) = driver {
                let _ = wifi.init();
            }
        });

        // Scan real ainda depende de ucode/HW — lista demo NÃO é Ready (S0).
        k_nano::slog_bin!(
            "WIFI-HW",
            "info",
            "step=scan status=UNSUPPORTED detail=demo_ap_list_not_rf"
        );
        k_nano::slog_bin!(
            "WIFI-HW",
            "info",
            "VERDICT=AWAITING_REAL_HW reason=iwlwifi_scan_unwired_onda7"
        );
        let aps = vec![
            AccessPoint { ssid: String::from("JARVIS-NET"),   bssid: [0xAA;6], signal_dbm: -45, channel: 6,  security: SecurityType::WPA2 },
            AccessPoint { ssid: String::from("MeuWiFi"),      bssid: [0xBB;6], signal_dbm: -60, channel: 11, security: SecurityType::WPA2 },
            AccessPoint { ssid: String::from("Rede-Aberta"),  bssid: [0xCC;6], signal_dbm: -72, channel: 1,  security: SecurityType::Open },
        ];

        let mut msg = alloc::format!(
            "DEMO AP list (nao e RF; scan iwlwifi AWAITING) — {} entradas:\n",
            aps.len()
        );
        for (i, ap) in aps.iter().enumerate() {
            msg.push_str(&alloc::format!(
                "  [{}] {} {} ({}dBm) ch.{} {}\n", i, ap.lock(), ap.ssid, ap.signal_dbm, ap.channel, ap.bars()));
        }
        msg.push_str("\nConnect bloqueado ate S1/S3 (RF). Digite NUMERO so para ver o gate S0.");
        self.publish(&msg);
        self.state = WifiState::ScanDone(aps);
    }

    // ── Conexão ───────────────────────────────────────────────

    fn do_connect(&mut self, ap: &AccessPoint, _password: &str) {
        // S0 honesty: nunca Connected/Ready sem RF + assoc real.
        let has_radio = generic_wifi::WIFI_PRESENT.load(Ordering::Relaxed);
        if !has_radio {
            k_nano::slog_bin!(
                "WIFI-HW",
                "info",
                "step=connect status=UNSUPPORTED detail=no_wifi_pci ssid={}",
                ap.ssid
            );
            k_nano::slog_bin!(
                "WIFI-HW",
                "info",
                "VERDICT=AWAITING_REAL_HW reason=no_wifi_radio"
            );
        } else {
            k_nano::slog_bin!(
                "WIFI-HW",
                "info",
                "step=connect status=UNSUPPORTED detail=link_unwired ssid={}",
                ap.ssid
            );
            k_nano::slog_bin!(
                "WIFI-HW",
                "info",
                "VERDICT=AWAITING_REAL_HW reason=iwlwifi_link_unwired"
            );
        }
        self.publish(&alloc::format!(
            "Connect bloqueado: sem RF/ucode (S0) — \"{}\"",
            ap.ssid
        ));
        self.state = WifiState::Failed("WiFi RF AWAITING");
    }

    // ── Persistência ──────────────────────────────────────────

    fn save_credentials(&self, ssid: &str, password: &str) {
        // Salva no FAT32 como WIFI.CFG (SSID + senha cifrada)
        // Reutilizado no proximo boot para auto-conexao
        let cfg = alloc::format!("{} {}\n", ssid, password);
        unsafe {
            let ata_guard = k_nano::ATA_DRIVER.lock();
            if let Some(ref ata) = *ata_guard {
                let parts = k_nano::fat32::read_mbr(ata);
                for p in &parts {
                    if p.type_code == 0x1C || p.type_code == 0x0C || p.type_code == 0x0B {
                        if let Some(w) = k_nano::fat32::Fat32Writer::new(ata, p) {
                            if w.write_file("WIFI.CFG", cfg.as_bytes()) {
                                k_nano::slog_hermes!("Wifi", "info", "Credenciais salvas (WIFI.CFG)");
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    fn load_credentials(&self) -> Option<(String, String)> {
        unsafe {
            let ata_guard = k_nano::ATA_DRIVER.lock();
            if let Some(ref ata) = *ata_guard {
                let parts = k_nano::fat32::read_mbr(ata);
                for p in &parts {
                    if p.type_code == 0x1C || p.type_code == 0x0C || p.type_code == 0x0B {
                        if let Some(r) = k_nano::fat32::Fat32Reader::new(ata, p) {
                            if let Some(data) = r.read_file("WIFI.CFG") {
                                let s = core::str::from_utf8(&data).unwrap_or("");
                                let mut parts = s.splitn(2, ' ');
                                let ssid = parts.next().unwrap_or("").trim();
                                let pass = parts.next().unwrap_or("").trim();
                                if !ssid.is_empty() {
                                    return Some((String::from(ssid), String::from(pass)));
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }
}

impl Agent for WifiAgent {
    fn manifest(&self) -> &AgentManifest { &WIFI_MANIFEST }

    fn tick(&mut self, tick: u64, _count: u64) -> AgentTickResult {
        // Auto-scan se WiFi presente, apos boot
        if matches!(self.state, WifiState::Idle) && tick > 20 {
            self.do_scan();
        }

        // Processa input do usuario
        while let Some(ev) = self.user_receiver.try_receive() {
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            let lower = text.to_ascii_lowercase();

            match &self.state {
                WifiState::Idle | WifiState::Connected { .. } => {
                    if lower.contains("wifi") || lower.contains("scan") {
                        self.do_scan();
                    }
                }
                WifiState::ScanDone(aps) => {
                    if let Ok(idx) = text.trim().parse::<usize>() {
                        if idx < aps.len() {
                            let ap = aps[idx].clone();
                            if ap.security == SecurityType::Open {
                                self.do_connect(&ap, "");
                            } else {
                                self.state = WifiState::AwaitingPassword { ap };
                                self.publish("Digite a senha da rede:");
                            }
                        }
                    } else if lower.contains("scan") {
                        self.do_scan();
                    }
                }
                WifiState::AwaitingChoice => {
                    if lower.contains("scan") {
                        self.do_scan();
                    }
                }
                WifiState::AwaitingPassword { .. } => {
                    match self.state.clone() {
                        WifiState::AwaitingPassword { ap } => {
                            self.do_connect(&ap, text.trim());
                        }
                        _ => {}
                    }
                }
                WifiState::Failed(_) => {
                    if lower.contains("wifi") || lower.contains("scan") {
                        self.do_scan();
                    }
                }
                _ => {}
            }
        }

        AgentTickResult::Pending
    }
}
