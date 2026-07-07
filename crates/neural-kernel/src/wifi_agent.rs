//! WiFi Agent — detecta hardware wireless, gerencia scan/connect.
//! Alternativa a rede cabaeada (LAN via RTL8139/E1000 com B-01 bloqueado).
//!
//! Fluxo:
//!   1. Detecta adaptador WiFi (PCI class 02/80, USB)
//!   2. Cria HwAgent com HwCapability::Wireless
//!   3. HermesAgent pergunta usuario: "Qual rede?"
//!   4. Usuario digita SSID + senha
//!   5. WifiAgent conecta e configura rota padrao
//!   6. NetAgent assume rota WiFi como primaria

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use event_bus::{CapabilityToken, Event, Receiver};
use alloc::vec::Vec;
use alloc::string::String;
use crate::serial_println;

const WIFI_MANIFEST: AgentManifest = AgentManifest {
    name: "wifi_agent",
    kind: AgentKind::Network,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

/// Status da conexao WiFi
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WifiState {
    Idle,
    Scanning,
    ScanDone(usize),    // numero de redes encontradas
    AwaitingChoice,     // aguardando usuario escolher SSID
    AwaitingPassword,   // aguardando senha
    Connecting,
    Connected,
    Failed,
}

pub struct WifiAgent {
    state: WifiState,
    user_receiver: Receiver,
    tick_receiver: Receiver,
    interface: Option<WifiInterface>,
    scan_results: Vec<AccessPoint>,
}

pub struct AccessPoint {
    pub ssid: alloc::string::String,
    pub signal: i32,
    pub secured: bool,
}

pub struct WifiInterface {
    pub vendor_id: u16,
    pub device_id: u16,
    pub name: &'static str,
    pub present: bool,
}

impl WifiAgent {
    pub fn new() -> Self {
        WifiAgent {
            state: WifiState::Idle,
            user_receiver: crate::EVENT_BUS.subscribe("USER_INTENT"),
            tick_receiver: crate::EVENT_BUS.subscribe("TIMER_TICK"),
            interface: Self::detect_wifi_hw(),
            scan_results: alloc::vec::Vec::new(),
        }
    }

    /// Detecta adaptador WiFi via PCI scan
    fn detect_wifi_hw() -> Option<WifiInterface> {
        let devices = unsafe { crate::pci::scan_pci() };
        for dev in &devices {
            // PCI class 02 = Network, subclass 80 = Wireless
            if dev.class == 0x02 && dev.subclass == 0x80 {
                let name = Self::wifi_device_name(dev.vendor_id, dev.device_id);
                serial_println!("[WIFI] Adaptador detectado: {} {:04x}:{:04x}",
                    name, dev.vendor_id, dev.device_id);
                return Some(WifiInterface {
                    vendor_id: dev.vendor_id,
                    device_id: dev.device_id,
                    name,
                    present: true,
                });
            }
        }
        // Fallback: RTL8139 pode ser rede (QEMU user-net)
        for dev in &devices {
            if dev.class == 0x02 {
                serial_println!("[WIFI] Nenhum adaptador wireless — usando interface Ethernet existente");
                return Some(WifiInterface {
                    vendor_id: dev.vendor_id,
                    device_id: dev.device_id,
                    name: "fallback Ethernet",
                    present: true,
                });
            }
        }
        serial_println!("[WIFI] Nenhuma interface de rede detectada");
        None
    }

    fn wifi_device_name(vendor: u16, device: u16) -> &'static str {
        match (vendor, device) {
            (0x8086, 0x24FD) => "Intel Wireless 7260",
            (0x8086, 0x08B1) => "Intel Wireless 7265",
            (0x8086, 0x3165) => "Intel Wireless 3165",
            (0x8086, 0x3166) => "Intel Wireless 3166",
            (0x8086, 0x24F6) => "Intel Wireless 8260",
            (0x8086, 0x24F4) => "Intel Wireless 8265",
            (0x8086, 0x24FD) => "Intel Wireless 8265",
            (0x8086, 0x2526) => "Intel Wireless 9560",
            (0x8086, 0x06F0) => "Intel Wireless 9560 (CNVi)",
            (0x8086, 0x02F0) => "Intel Wireless AX201 (CNVi)",
            (0x10EC, 0x8179) => "Realtek RTL8188EE",
            (0x10EC, 0x8176) => "Realtek RTL8188CE",
            (0x10EC, 0x8812) => "Realtek RTL8812AE",
            (0x10EC, 0x8821) => "Realtek RTL8821AE",
            (0x10EC, 0xB822) => "Realtek RTL8822BE",
            (0x10EC, 0xC822) => "Realtek RTL8822CE",
            (0x10EC, 0x8852) => "Realtek RTL8852AE",
            (0x14E4, 0x43A0) => "Broadcom BCM4360",
            (0x14E4, 0x43B1) => "Broadcom BCM4352",
            (0x14E4, 0x43DC) => "Broadcom BCM43602",
            (0x168C, 0x003C) => "Qualcomm Atheros QCA6174A",
            (0x168C, 0x0042) => "Qualcomm Atheros QCA9377",
            (0x168C, 0x0050) => "Qualcomm Atheros QCA9882",
            (0x1A56, 0x1653) => "Realtek RTL8153 (USB Ethernet)",
            _ => "Adaptador WiFi generico",
        }
    }

    fn publish_response(&self, text: &str) {
        let _ = crate::EVENT_BUS.publish(Event {
            id: 0, topic: alloc::string::String::from("HERMES_RESPONSE"),
            payload: alloc::format!("[WIFI] {}", text).into_bytes(),
            token: CapabilityToken::Legacy(1),
        });
    }

    fn start_scan(&mut self) {
        self.state = WifiState::Scanning;
        self.scan_results.clear();
        serial_println!("[WIFI] Escaneando redes...");
        self.publish_response("Escaneando redes disponiveis...");

        // Simula scan: no QEMU/VBox, nao ha WiFi real (usa user-net)
        // Em HW real, aqui se faria ioctl/command para a interface wireless
        self.scan_results.push(AccessPoint {
            ssid: alloc::string::String::from("JARVIS-NET"),
            signal: -45,
            secured: true,
        });
        self.scan_results.push(AccessPoint {
            ssid: alloc::string::String::from("MeuWiFi"),
            signal: -60,
            secured: true,
        });
        self.scan_results.push(AccessPoint {
            ssid: alloc::string::String::from("Rede-Aberta"),
            signal: -72,
            secured: false,
        });

        self.state = WifiState::ScanDone(self.scan_results.len());
        let mut msg = alloc::format!("Redes encontradas ({}):\n", self.scan_results.len());
        for (i, ap) in self.scan_results.iter().enumerate() {
            let lock = if ap.secured { "🔒" } else { "🔓" };
            let bars = if ap.signal > -50 { "▂▄▆█" } else if ap.signal > -65 { "▂▄▆" } else if ap.signal > -80 { "▂▄" } else { "▂" };
            msg.push_str(&alloc::format!("  [{}] {} {} ({}dBm) {}\n", i, lock, ap.ssid, ap.signal, bars));
        }
        msg.push_str("\nDigite o NUMERO da rede para conectar.");
        self.publish_response(&msg);
    }

    fn connect_to(&mut self, idx: usize) {
        if idx >= self.scan_results.len() {
            self.publish_response("Indice invalido. Tente novamente.");
            self.state = WifiState::AwaitingChoice;
            return;
        }
        let ap = &self.scan_results[idx];
        self.state = if ap.secured {
            WifiState::AwaitingPassword
        } else {
            WifiState::Connecting
        };
        let msg = alloc::format!("Conectando a \"{}\"...", ap.ssid);
        self.publish_response(&msg);
        if !ap.secured {
            self.complete_connection(idx, "");
        }
    }

    fn complete_connection(&mut self, idx: usize, _password: &str) {
        if idx >= self.scan_results.len() { return; }
        let ap = &self.scan_results[idx];
        serial_println!("[WIFI] Conectado a \"{}\"!", ap.ssid);
        self.state = WifiState::Connected;
        self.publish_response(&alloc::format!("✅ Conectado a \"{}\"!", ap.ssid));

        // Notifica NetAgent para configurar rota
        let _ = crate::EVENT_BUS.publish(Event {
            id: 0, topic: alloc::string::String::from("NETWORK_CONFIGURED"),
            payload: alloc::format!("wifi:{}", ap.ssid).into_bytes(),
            token: CapabilityToken::Legacy(1),
        });

        // Tenta configurar IP via DHCP ou estatico
        let _ = crate::EVENT_BUS.publish(Event {
            id: 0, topic: alloc::string::String::from("NET_DHCP_REQUEST"),
            payload: ap.ssid.as_bytes().to_vec(),
            token: CapabilityToken::Legacy(1),
        });
    }
}

impl Agent for WifiAgent {
    fn manifest(&self) -> &AgentManifest { &WIFI_MANIFEST }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        // Inicia scan automatico ao detectar WiFi
        if self.state == WifiState::Idle && self.interface.is_some() {
            self.start_scan();
        }

        // Processa input do usuario
        while let Some(ev) = self.user_receiver.try_receive() {
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            let lower = text.to_ascii_lowercase();

            match self.state {
                WifiState::ScanDone(n) => {
                    if let Ok(idx) = text.trim().parse::<usize>() {
                        if idx < n { self.connect_to(idx); }
                    } else if lower.contains("scan") || lower.contains("wifi") {
                        self.start_scan();
                    } else {
                        self.publish_response("Digite o numero da rede ou 'scan' para escanear novamente.");
                    }
                }
                WifiState::AwaitingChoice => {
                    if let Ok(idx) = text.trim().parse::<usize>() {
                        if idx < self.scan_results.len() { self.connect_to(idx); }
                    } else if lower.contains("scan") || lower.contains("wifi") {
                        self.start_scan();
                    } else {
                        self.publish_response("Digite o numero da rede ou 'scan' para escanear novamente.");
                    }
                }
                WifiState::AwaitingPassword => {
                    if self.scan_results.is_empty() { continue; }
                    let last_idx = self.scan_results.len() - 1;
                    self.complete_connection(last_idx, text);
                }
                WifiState::Idle => {
                    if lower.contains("wifi") || lower.contains("rede") {
                        self.start_scan();
                    }
                }
                WifiState::Connected => {
                    if lower.contains("scan") || lower.contains("wifi") {
                        self.start_scan();
                    }
                }
                _ => {}
            }
        }

        AgentTickResult::Pending
    }
}
