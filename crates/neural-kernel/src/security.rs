//! Security Pipeline — EventBus → Detector → Correlation → Response.
//! #260: 5 detectores iniciais para ameaças de rede e sistema.
//! Conectado ao EventBus: subscribe NET_EVENT + SYSTEM_EVENT, publish SECURITY_ALERT.

use alloc::string::String;
use alloc::vec::Vec;
use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use crate::interrupts::TIMER_TICKS;
use crate::{serial_println, println};
use crate::EVENT_BUS;
use crate::{Event, CapabilityToken};

const SEC_MANIFEST: AgentManifest = AgentManifest {
    name: "security",
    kind: AgentKind::System,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

pub const TOPIC_NET_EVENT: &str = "NET_EVENT";
pub const TOPIC_SYSTEM_EVENT: &str = "SYSTEM_EVENT";
pub const TOPIC_SECURITY_ALERT: &str = "SECURITY_ALERT";

/// Evento de segurança detectado
pub struct SecurityEvent {
    pub detector: &'static str,
    pub severity: u8,        // 1-5 (5 = crítico)
    pub description: String,
    pub tick: u64,
}

pub struct SecurityAgent {
    net_receiver: crate::Receiver,
    sys_receiver: crate::Receiver,
    events: Vec<SecurityEvent>,
    port_scan_counter: u64,
    ping_flood_counter: u64,
    last_arp_check: u64,
}

impl SecurityAgent {
    pub fn new() -> Self {
        SecurityAgent {
            net_receiver: EVENT_BUS.subscribe(TOPIC_NET_EVENT),
            sys_receiver: EVENT_BUS.subscribe(TOPIC_SYSTEM_EVENT),
            events: Vec::new(),
            port_scan_counter: 0,
            ping_flood_counter: 0,
            last_arp_check: 0,
        }
    }

    /// Processa eventos de rede recebidos via EventBus
    fn process_net_event(&mut self, payload: &[u8], tick: u64) {
        let text = core::str::from_utf8(payload).unwrap_or("");
        if text.contains("SYN") || text.contains("connect") {
            self.detect_port_scan(tick, payload);
        }
        if text.contains("ICMP") || text.contains("ping") {
            self.detect_ping_flood(tick);
        }
        if text.contains("ARP") {
            self.detect_arp_spoof(tick);
        }
    }

    /// Processa eventos de sistema recebidos via EventBus
    fn process_sys_event(&mut self, payload: &[u8], tick: u64) {
        let text = core::str::from_utf8(payload).unwrap_or("");
        if text.contains("timer") || text.contains("drift") {
            self.detect_timer_anomaly(tick);
        }
        if text.contains("dhcp") || text.contains("lease") {
            self.detect_dhcp_starvation(tick);
        }
    }

    /// Publica alerta de segurança no EventBus
    fn publish_alert(&self, event: &SecurityEvent) {
        let msg = alloc::format!("[SECURITY] {} severidade={}: {}",
            event.detector, event.severity, event.description);
        let _ = EVENT_BUS.publish(Event {
            id: event.tick,
            topic: String::from(TOPIC_SECURITY_ALERT),
            payload: msg.into_bytes(),
            token: CapabilityToken::Legacy(1),
        });
    }

    fn detect_port_scan(&mut self, tick: u64, _payload: &[u8]) {
        self.port_scan_counter += 1;
        if self.port_scan_counter > 50 {
            let event = SecurityEvent {
                detector: "PortScan",
                severity: 4,
                description: alloc::format!("Port scan detectado: {} acessos", self.port_scan_counter),
                tick,
            };
            self.publish_alert(&event);
            self.events.push(event);
            self.port_scan_counter = 0;
        }
    }

    fn detect_arp_spoof(&mut self, tick: u64) {
        if tick > self.last_arp_check + 200 {
            self.last_arp_check = tick;
            let cfg = crate::net::NET_CONFIG.lock();
            let gw_mac = cfg.gateway_mac;
            drop(cfg);
            if gw_mac != [0; 6] {
                // Em producao: comparar com ARP cache
            }
        }
    }

    fn detect_ping_flood(&mut self, tick: u64) {
        self.ping_flood_counter += 1;
        if self.ping_flood_counter > 100 {
            let event = SecurityEvent {
                detector: "PingFlood",
                severity: 3,
                description: alloc::format!("Ping flood: {} pacotes ICMP", self.ping_flood_counter),
                tick,
            };
            self.publish_alert(&event);
            self.events.push(event);
            self.ping_flood_counter = 0;
        }
    }

    fn detect_dhcp_starvation(&mut self, _tick: u64) {}

    fn detect_timer_anomaly(&mut self, tick: u64) {
        if tick > 1000 && tick % 1000 == 0 {
            let expected = tick;
            let actual = TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
            if expected.abs_diff(actual) > 10 {
                let event = SecurityEvent {
                    detector: "TimerAnomaly",
                    severity: 2,
                    description: alloc::format!("Timer drift: esperado={} real={}", expected, actual),
                    tick,
                };
                self.publish_alert(&event);
                self.events.push(event);
            }
        }
    }

    fn correlate(&mut self, tick: u64) {
        if self.events.len() >= 3 {
            let sev: u8 = self.events.iter().map(|e| e.severity).max().unwrap_or(0);
            serial_println!("[SECURITY] Correlacao: {} eventos, severidade max={}", self.events.len(), sev);
            if sev >= 4 {
                let msg = alloc::format!("ALERTA: {} eventos detectados, severidade {}", self.events.len(), sev);
                let _ = EVENT_BUS.publish(Event {
                    id: tick,
                    topic: String::from(TOPIC_SECURITY_ALERT),
                    payload: msg.into_bytes(),
                    token: CapabilityToken::Legacy(1),
                });
                // Notifica Hermes
                let _ = EVENT_BUS.publish(Event {
                    id: tick,
                    topic: String::from(crate::hermes::TOPIC_HERMES_RESPONSE),
                    payload: alloc::format!("[SECURITY] Correlacao: {} eventos, severidade {}", self.events.len(), sev).into_bytes(),
                    token: CapabilityToken::Legacy(1),
                });
            }
            self.events.clear();
        }
    }
}

impl Agent for SecurityAgent {
    fn manifest(&self) -> &AgentManifest { &SEC_MANIFEST }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        let tick = TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;

        // Poll EventBus por eventos de rede e sistema
        while let Some(event) = self.net_receiver.try_receive() {
            self.process_net_event(&event.payload, tick);
        }
        while let Some(event) = self.sys_receiver.try_receive() {
            self.process_sys_event(&event.payload, tick);
        }

        // Poll detectores legacy
        self.detect_arp_spoof(tick);
        self.detect_dhcp_starvation(tick);
        self.detect_timer_anomaly(tick);

        if tick % 100 == 0 {
            self.correlate(tick);
        }

        AgentTickResult::Pending
    }
}

// ---------------------------------------------------------------------------
// Path Confinement + Mask Secrets
// ---------------------------------------------------------------------------

/// Politica de confinamento de path para skills
/// Impede que skills acessem diretorios ou arquivos proibidos
#[derive(Debug, Clone)]
pub struct PathPolicy {
    pub allowed_prefixes: &'static [&'static str],
    pub denied_prefixes: &'static [&'static str],
    pub mask_patterns: &'static [&'static str],
}

impl PathPolicy {
    pub const fn new(allowed: &'static [&'static str], denied: &'static [&'static str], masks: &'static [&'static str]) -> Self {
        PathPolicy { allowed_prefixes: allowed, denied_prefixes: denied, mask_patterns: masks }
    }

    /// Verifica se um path eh permitido
    pub fn check_path(&self, path: &str) -> Result<(), &'static str> {
        for denied in self.denied_prefixes {
            if path.starts_with(denied) {
                return Err("Path negado pela politica de seguranca");
            }
        }
        if self.allowed_prefixes.is_empty() {
            return Ok(());
        }
        for allowed in self.allowed_prefixes {
            if path.starts_with(allowed) {
                return Ok(());
            }
        }
        Err("Path nao permitido pela politica de seguranca")
    }

    /// Aplica mascaramento de segredos em uma string
    pub fn mask_secrets(&self, input: &str) -> String {
        let mut result = String::from(input);
        for pattern in self.mask_patterns {
            let mut search_start = 0;
            loop {
                if let Some(pos) = result[search_start..].find(pattern) {
                    let abs_pos = search_start + pos;
                    let end = core::cmp::min(abs_pos + 32, result.len());
                    let masked: String = result.chars().take(abs_pos + pattern.len())
                        .chain("[REDACTED]".chars())
                        .chain(result.chars().skip(end))
                        .collect();
                    result = masked;
                    search_start = abs_pos + pattern.len() + 10;
                } else {
                    break;
                }
            }
        }
        result
    }
}

/// Politica global de seguranca
pub static SECURITY_POLICY: PathPolicy = PathPolicy::new(
    &["/system/", "/data/", "/tmp/"],
    &["/system/secure/", "/system/keys/", "//"],
    &["sk-", "-----BEGIN", "AKIA", "ghp_"],
);
