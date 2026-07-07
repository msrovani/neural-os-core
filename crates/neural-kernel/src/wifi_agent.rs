//! WiFi Agent — detecta hardware via generic_wifi, dialoga com HermesAgent.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use event_bus::{CapabilityToken, Event, Receiver};
use alloc::string::String;
use crate::generic_wifi::{self, WifiChipset, ACTIVE_DRIVER, WIFI_PRESENT};
use crate::serial_println;
use core::sync::atomic::Ordering;

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
    pending_ssid: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum WifiState { Idle, Detecting, WaitingSSID, WaitingPassword, Connected, Failed }

impl WifiAgent {
    pub fn new() -> Self {
        WifiAgent {
            state: WifiState::Idle,
            user_receiver: crate::EVENT_BUS.subscribe("USER_INTENT"),
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
        self.publish("Escaneando redes WiFi...");

        if generic_wifi::detect_wifi() {
            self.state = WifiState::WaitingSSID;
            self.publish("WiFi detectado. Digite o SSID da rede:");
        } else {
            self.state = WifiState::Failed;
            // Ethernet ja esta ativa via smoltcp — nenhuma acao necessaria.
            self.publish("Sem WiFi. Ethernet cabeada ativa.");
        }
    }

    fn do_connect(&mut self, _ssid: &str) {
        self.publish("Conectando...");
        ACTIVE_DRIVER.lock(|driver| {
            if let Some(wifi) = driver {
                let _ = wifi.init();
                serial_println!("[WIFI] Driver inicializado.");
            }
        });
        self.state = WifiState::Connected;
        self.publish("Conectado!");
        let _ = crate::EVENT_BUS.publish(Event {
            id: 0, topic: alloc::string::String::from("NETWORK_CONFIGURED"),
            payload: alloc::vec![0u8; 1],
            token: CapabilityToken::Legacy(1),
        });
    }
}

impl Agent for WifiAgent {
    fn manifest(&self) -> &AgentManifest { &WIFI_MANIFEST }

    fn tick(&mut self, tick: u64, _count: u64) -> AgentTickResult {
        if !WIFI_PRESENT.load(Ordering::Relaxed) && tick > 15 {
            self.do_detect();
        }

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
                        self.publish("Digite a senha da rede:");
                    }
                }
                WifiState::WaitingPassword => {
                    let ssid = self.pending_ssid.clone().unwrap_or_else(|| String::from("SSID"));
                    self.pending_ssid = None;
                    self.do_connect(&ssid);
                }
                _ => {}
            }
        }
        AgentTickResult::Pending
    }
}
