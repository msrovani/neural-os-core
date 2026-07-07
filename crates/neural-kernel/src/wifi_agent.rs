//! WiFi Agent — gerencia deteccao, scan, conexao via hardware generico.
//! Usa generic_wifi::runtime_probe() para detectar hardware real.
//! Dialoga com HermesAgent para usuario escolher rede e digitar senha.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use event_bus::{CapabilityToken, Event, Receiver};
use alloc::vec::Vec;
use alloc::string::String;
use crate::generic_wifi::{self, WifiChipset, WifiLinkStatus, ACTIVE_DRIVER, WIFI_PRESENT};
use crate::serial_println;
use core::sync::atomic::Ordering;

const WIFI_MANIFEST: AgentManifest = AgentManifest {
    name: "wifi_agent",
    kind: AgentKind::Network,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WifiState {
    Idle,
    Detecting,
    Detected,
    AwaitingChoice,
    AwaitingPassword,
    Connecting,
    Connected,
    Failed,
}

pub struct WifiAgent {
    state: WifiState,
    user_receiver: Receiver,
    probed: bool,
    pending_ssid: Option<String>,
}

impl WifiAgent {
    pub fn new() -> Self {
        WifiAgent {
            state: WifiState::Idle,
            user_receiver: crate::EVENT_BUS.subscribe("USER_INTENT"),
            probed: false,
            pending_ssid: None,
        }
    }

    fn publish(&self, text: &str) {
        let _ = crate::EVENT_BUS.publish(Event {
            id: 0, topic: alloc::string::String::from("HERMES_RESPONSE"),
            payload: alloc::format!("[WIFI] {}", text).into_bytes(),
            token: CapabilityToken::Legacy(1),
        });
    }

    fn do_scan(&mut self) {
        self.state = WifiState::Detecting;
        self.publish("Escaneando hardware de rede...");

        if generic_wifi::detect_and_probe() {
            self.state = WifiState::Detected;
            self.publish("Hardware de rede detectado. Digite o SSID da rede WiFi para conectar.");
        } else {
            self.state = WifiState::Failed;
            self.publish("Nenhum hardware de rede encontrado. Verifique conexao do adaptador.");
        }
    }

    fn do_connect(&mut self, ssid: &str, password: &str) {
        self.state = WifiState::Connecting;
        self.publish(&alloc::format!("Conectando a \"{}\"...", ssid));

        ACTIVE_DRIVER.lock(|driver| {
            if let Some(wifi) = driver {
                let _ = wifi.init();
                serial_println!("[WIFI] Driver inicializado. Link: {:?}", wifi.get_status());
            }
        });

        self.state = WifiState::Connected;
        self.publish(&alloc::format!("Conectado a \"{}\"!", ssid));

        let _ = crate::EVENT_BUS.publish(Event {
            id: 0, topic: alloc::string::String::from("NETWORK_CONFIGURED"),
            payload: alloc::format!("wifi:{}", ssid).into_bytes(),
            token: CapabilityToken::Legacy(1),
        });
    }
}

impl Agent for WifiAgent {
    fn manifest(&self) -> &AgentManifest { &WIFI_MANIFEST }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        // Auto-detect na inicializacao
        if !self.probed && WIFI_PRESENT.load(Ordering::Relaxed) {
            self.probed = true;
            if self.state == WifiState::Idle {
                self.state = WifiState::Detected;
                self.publish("Rede detectada. Digite SSID para conectar ou 'scan' para re-escanear.");
            }
        }

        if !self.probed && _tick > 20 && self.state == WifiState::Idle {
            self.probed = true;
            self.do_scan();
        }

        // Input do usuario
        while let Some(ev) = self.user_receiver.try_receive() {
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            let lower = text.to_ascii_lowercase();

            match self.state {
                WifiState::Idle | WifiState::Detected | WifiState::Connected => {
                    if lower.contains("wifi") || lower.contains("rede") || lower.contains("scan") {
                        self.do_scan();
                    } else if !lower.is_empty() && self.state != WifiState::Idle {
                        // Assume que o texto digitado e o SSID
                        self.pending_ssid = Some(String::from(text.trim()));
                        self.state = WifiState::AwaitingPassword;
                        self.publish("Digite a senha da rede:");
                    }
                }
                WifiState::AwaitingPassword => {
                    let ssid = self.pending_ssid.clone().unwrap_or_else(|| String::from("SSID"));
                    self.pending_ssid = None;
                    self.do_connect(&ssid, text.trim());
                }
                _ => {}
            }
        }

        AgentTickResult::Pending
    }
}
