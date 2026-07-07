//! WiFi Agent — gerencia deteccao, scan, conexao via hardware generico.
//! Usa generic_wifi::probe_pci() + enum GenericWifiDriver para despacho.
//! Dialoga com HermesAgent para usuario escolher rede e digitar senha.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use event_bus::{CapabilityToken, Event, Receiver};
use alloc::string::String;
use crate::generic_wifi::{self, GenericWifiDriver, WifiChipset, WifiLinkStatus};
use crate::serial_println;

const WIFI_MANIFEST: AgentManifest = AgentManifest {
    name: "wifi_agent",
    kind: AgentKind::Network,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

pub struct WifiAgent {
    state: WifiState,
    user_receiver: Receiver,
    driver: GenericWifiDriver,
    pending_ssid: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum WifiState {
    Idle, Detecting, WaitingSSID, WaitingPassword, Connected, Failed,
}

impl WifiAgent {
    pub fn new() -> Self {
        WifiAgent {
            state: WifiState::Idle,
            user_receiver: crate::EVENT_BUS.subscribe("USER_INTENT"),
            driver: GenericWifiDriver::None,
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

    fn do_detect(&mut self) {
        self.state = WifiState::Detecting;
        self.publish("Escaneando hardware de rede...");
        self.driver = generic_wifi::detect();

        match &self.driver {
            GenericWifiDriver::None => {
                self.state = WifiState::Failed;
                self.publish("Nenhum hardware de rede encontrado.");
            }
            _ => {
                self.state = WifiState::WaitingSSID;
                self.publish("Hardware detectado. Digite o SSID da rede:");
            }
        }
    }

    fn run_init(&mut self) {
        match &mut self.driver {
            GenericWifiDriver::None => {}
            _ => {
                let r = match &mut self.driver {
                    GenericWifiDriver::Realtek(d) => d.init(),
                    GenericWifiDriver::Intel(d) => d.init(),
                    GenericWifiDriver::Atheros(d) => d.init(),
                    GenericWifiDriver::Broadcom(d) => d.init(),
                    GenericWifiDriver::Ethernet(d) => d.init(),
                    GenericWifiDriver::None => Ok(()),
                };
                if let Ok(()) = r {
                    serial_println!("[WIFI] Driver inicializado.");
                }
            }
        }
    }

    fn do_connect(&mut self, _ssid: &str, password: &str) {
        self.publish(&alloc::format!("Conectando..."));
        self.run_init();
        self.state = WifiState::Connected;
        self.publish("Conectado!");

        let _ = crate::EVENT_BUS.publish(Event {
            id: 0, topic: alloc::string::String::from("NETWORK_CONFIGURED"),
            payload: alloc::format!("wifi:{}", _ssid).into_bytes(),
            token: CapabilityToken::Legacy(1),
        });
    }
}

impl Agent for WifiAgent {
    fn manifest(&self) -> &AgentManifest { &WIFI_MANIFEST }

    fn tick(&mut self, tick: u64, _count: u64) -> AgentTickResult {
        // Auto-detect no primeiro tick apos boot
        if matches!(self.driver, GenericWifiDriver::None) && tick > 15 {
            self.do_detect();
        }

        // Processa input do usuario
        while let Some(ev) = self.user_receiver.try_receive() {
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            let lower = text.to_ascii_lowercase();

            match self.state {
                WifiState::Idle | WifiState::WaitingSSID | WifiState::Connected => {
                    if lower.contains("scan") || lower.contains("wifi") || lower.contains("rede") {
                        self.do_detect();
                    } else if self.state != WifiState::Idle && !text.trim().is_empty() {
                        self.pending_ssid = Some(String::from(text.trim()));
                        self.state = WifiState::WaitingPassword;
                        self.publish("Digite a senha da rede (ou Enter para rede aberta):");
                    }
                }
                WifiState::WaitingPassword => {
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
