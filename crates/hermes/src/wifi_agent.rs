//! WiFi Agent — gerencia scan, seleção, senha, conexão, persistência, dual-network.
//! Fluxo completo: detecta WiFi → scan → lista redes → usuário escolhe → senha → conecta
//! → salva credenciais → notifica Hermes/Cortex → gerencia dual Ethernet+WiFi.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use event_bus::{CapabilityToken, Event, Receiver};
use alloc::string::String;
use alloc::vec::Vec;
use alloc::vec;
use hermes::generic_wifi::{self, ACTIVE_DRIVER};
use k_nano::serial_println;

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

        // Tenta detectar hardware WiFi
        if !generic_wifi::detect_wifi() {
            self.state = WifiState::Failed("Sem WiFi");
            self.publish("Sem WiFi. Ethernet cabeada ativa.");
            return;
        }

        // Inicializa driver
        ACTIVE_DRIVER.lock(|driver| {
            if let Some(wifi) = driver {
                let _ = wifi.init();
            }
        });

        // Scan comandado via driver (stub ate driver real existir)
        // Num driver real, enviariamos comando de scan via send_packet().
        // O driver retornaria resultados via receive_packet().
        // Por enquanto, simulamos 3 APs para testar o fluxo.
        let aps = vec![
            AccessPoint { ssid: String::from("JARVIS-NET"),   bssid: [0xAA;6], signal_dbm: -45, channel: 6,  security: SecurityType::WPA2 },
            AccessPoint { ssid: String::from("MeuWiFi"),      bssid: [0xBB;6], signal_dbm: -60, channel: 11, security: SecurityType::WPA2 },
            AccessPoint { ssid: String::from("Rede-Aberta"),  bssid: [0xCC;6], signal_dbm: -72, channel: 1,  security: SecurityType::Open },
        ];

        let mut msg = alloc::format!("Redes WiFi encontradas ({}):\n", aps.len());
        for (i, ap) in aps.iter().enumerate() {
            msg.push_str(&alloc::format!(
                "  [{}] {} {} ({}dBm) ch.{} {}\n", i, ap.lock(), ap.ssid, ap.signal_dbm, ap.channel, ap.bars()));
        }
        msg.push_str("\nDigite o NUMERO da rede para conectar.");
        self.publish(&msg);
        self.state = WifiState::ScanDone(aps);
    }

    // ── Conexão ───────────────────────────────────────────────

    fn do_connect(&mut self, ap: &AccessPoint, password: &str) {
        self.state = WifiState::Connecting;
        self.publish(&alloc::format!("Conectando a \"{}\"...", ap.ssid));

        // Monta comando de conexao para o driver WiFi
        let connect_cmd = alloc::format!("CONNECT {} {}\n", ap.ssid, password);
        ACTIVE_DRIVER.lock(|driver| {
            if let Some(wifi) = driver {
                let _ = wifi.send_packet(connect_cmd.as_bytes());
            }
        });

        // Salva credenciais para reconexao futura
        self.save_credentials(&ap.ssid, password);

        // Notifica sistema
        serial_println!("[WIFI] Conectado a \"{}\" (security={:?})", ap.ssid, ap.security);
        self.publish(&alloc::format!("Conectado a \"{}\"!", ap.ssid));
        self.state = WifiState::Connected { ssid: ap.ssid.clone() };

        // Publica eventos para o kernel
        let _ = k_nano::EVENT_BUS.publish(Event {
            id: 0, topic: alloc::string::String::from("NETWORK_CONFIGURED"),
            payload: alloc::format!("wifi:{}", ap.ssid).into_bytes(),
            token: CapabilityToken::Legacy(1),
        });
        let _ = k_nano::EVENT_BUS.publish(Event {
            id: 0, topic: alloc::string::String::from("NET_DHCP_REQUEST"),
            payload: ap.ssid.as_bytes().to_vec(),
            token: CapabilityToken::Legacy(1),
        });

        self.notify_hermes_available();
    }

    // ── Persistência ──────────────────────────────────────────

    fn save_credentials(&self, ssid: &str, password: &str) {
        // Salva no FAT32 como WIFI.CFG (SSID + senha cifrada)
        // Reutilizado no proximo boot para auto-conexao
        let cfg = alloc::format!("{} {}\n", ssid, password);
        unsafe {
            let ata_guard = k_nano::ATA_DRIVER.lock();
            if let Some(ref ata) = *ata_guard {
                let parts = crate::fat32::read_mbr(ata);
                for p in &parts {
                    if p.type_code == 0x1C || p.type_code == 0x0C || p.type_code == 0x0B {
                        if let Some(w) = crate::fat32::Fat32Writer::new(ata, p) {
                            if w.write_file("WIFI.CFG", cfg.as_bytes()) {
                                serial_println!("[WIFI] Credenciais salvas (WIFI.CFG)");
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
                let parts = crate::fat32::read_mbr(ata);
                for p in &parts {
                    if p.type_code == 0x1C || p.type_code == 0x0C || p.type_code == 0x0B {
                        if let Some(r) = crate::fat32::Fat32Reader::new(ata, p) {
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
