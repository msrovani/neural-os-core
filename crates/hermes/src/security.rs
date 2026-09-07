//! Ring 1 ownership: permanece em hermes (R3) por depender de EVENT_BUS (tópicos NET_EVENT, SYSTEM_EVENT).
//! ADR-0060 A.4: SecurityPipeline mantido em hermes; detectores base em k_ai::security_detectors.
//!
//! Security Pipeline — EventBus → Detector → Correlation → Response.
//! #260: 5 detectores iniciais para ameaças de rede e sistema.
//! Conectado ao EventBus: subscribe NET_EVENT + SYSTEM_EVENT, publish SECURITY_ALERT.
//!
//! NET_EVENT payload format (structured):
//!   `CONNECT src_ip=X.X.X.X dst_port=N`
//!   `ICMP src_ip=X.X.X.X`
//!   `ARP src_ip=X.X.X.X src_mac=XX:XX:XX:XX:XX:XX`
//!   `DHCP_DISCOVER src_mac=XX:XX:XX:XX:XX:XX`

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use k_nano::interrupts::TIMER_TICKS;
use k_nano::EVENT_BUS;
use event_bus::{Event, CapabilityToken};

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

// ── Helper: network code calls this to publish structured NET_EVENT ────────

/// Publish a structured network event to the security pipeline.
/// Network code (TCP, ARP, ICMP) calls this so SecurityAgent's real detectors fire.
pub fn publish_net_event(event_type: &str, src_ip: [u8; 4], dst_port: u16, src_mac: Option<[u8; 6]>) {
    let mut payload = String::new();
    match event_type {
        "CONNECT" => {
            // PortScanDetector: needs src_ip + dst_port
            payload.push_str("CONNECT src_ip=");
            push_ip(&mut payload, src_ip);
            payload.push_str(" dst_port=");
            push_port(&mut payload, dst_port);
        }
        "ICMP" => {
            // PingFloodDetector: needs src_ip
            payload.push_str("ICMP src_ip=");
            push_ip(&mut payload, src_ip);
        }
        "ARP" => {
            // ArpSpoofDetector: needs src_ip + src_mac
            payload.push_str("ARP src_ip=");
            push_ip(&mut payload, src_ip);
            if let Some(mac) = src_mac {
                payload.push_str(" src_mac=");
                push_mac(&mut payload, mac);
            }
        }
        "DHCP_DISCOVER" => {
            // DhcpStarvationDetector: needs src_mac
            payload.push_str("DHCP_DISCOVER src_mac=");
            if let Some(mac) = src_mac {
                push_mac(&mut payload, mac);
            }
        }
        _ => return,
    }
    let _ = EVENT_BUS.publish(Event {
        id: 0,
        topic: String::from(TOPIC_NET_EVENT),
        payload: payload.into_bytes(),
        token: CapabilityToken::Legacy(1),
    });
}

/// Publish a system event (e.g. timer tick for anomaly detection).
pub fn publish_system_event(event_type: &str) {
    let tick = TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
    let mut payload = String::new();
    match event_type {
        "TIMER" => {
            payload.push_str("TIMER tick=");
            push_u64(&mut payload, tick);
        }
        "DHCP_LEASE" => {
            payload.push_str("DHCP_LEASE");
        }
        _ => return,
    }
    let _ = EVENT_BUS.publish(Event {
        id: 0,
        topic: String::from(TOPIC_SYSTEM_EVENT),
        payload: payload.into_bytes(),
        token: CapabilityToken::Legacy(1),
    });
}

fn push_ip(buf: &mut String, ip: [u8; 4]) {
    let s = alloc::format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3]);
    buf.push_str(&s);
}

fn push_port(buf: &mut String, port: u16) {
    buf.push_str(&alloc::format!("{}", port));
}

fn push_mac(buf: &mut String, mac: [u8; 6]) {
    buf.push_str(&alloc::format!(
        "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    ));
}

fn push_u64(buf: &mut String, v: u64) {
    buf.push_str(&alloc::format!("{}", v));
}

// ── Parse helpers ───────────────────────────────────────────────────────────

/// Parse `src_ip=X.X.X.X` from payload. Returns None if not found.
fn parse_src_ip(payload: &str) -> Option<[u8; 4]> {
    let start = payload.find("src_ip=")? + 7;
    let end = payload[start..].find(char::is_whitespace).unwrap_or(payload[start..].len()) + start;
    let ip_str = &payload[start..end];
    let mut parts = [0u8; 4];
    for (i, p) in ip_str.split('.').enumerate() {
        parts[i] = p.parse().ok()?;
    }
    Some(parts)
}

/// Parse `src_mac=XX:XX:XX:XX:XX:XX` from payload. Returns None if not found.
fn parse_src_mac(payload: &str) -> Option<[u8; 6]> {
    let start = payload.find("src_mac=")? + 8;
    let end = payload[start..].find(char::is_whitespace).unwrap_or(payload[start..].len()) + start;
    let mac_str = &payload[start..end];
    let mut mac = [0u8; 6];
    for (i, p) in mac_str.split(':').enumerate() {
        if i >= 6 { break; }
        mac[i] = u8::from_str_radix(p, 16).ok()?;
    }
    Some(mac)
}

/// Parse `dst_port=N` from payload. Returns None if not found.
fn parse_dst_port(payload: &str) -> Option<u16> {
    let start = payload.find("dst_port=")? + 9;
    let end = payload[start..].find(char::is_whitespace).unwrap_or(payload[start..].len()) + start;
    payload[start..end].parse().ok()
}

/// Parse `src_mac=XX:XX:XX:XX:XX:XX` for DHCP_DISCOVER (same as ARP but MAC-only).
fn parse_dhcp_mac(payload: &str) -> Option<[u8; 6]> {
    parse_src_mac(payload) // Same format, re-use parser
}

// ── SecurityAgent ───────────────────────────────────────────────────────────

pub struct SecurityAgent {
    net_receiver: event_bus::Receiver,
    sys_receiver: event_bus::Receiver,
    // Real detectors from k_ai
    port_scan: k_ai::security_detectors::PortScanDetector,
    arp_spoof: k_ai::security_detectors::ArpSpoofDetector,
    ping_flood: k_ai::security_detectors::PingFloodDetector,
    dhcp_starvation: k_ai::security_detectors::DhcpStarvationDetector,
    timer_anomaly: k_ai::security_detectors::TimerAnomalyDetector,
    // Correlation buffer
    alerts: Vec<k_ai::security_detectors::SecurityAlert>,
}

impl SecurityAgent {
    pub fn new() -> Self {
        SecurityAgent {
            net_receiver: EVENT_BUS.subscribe(TOPIC_NET_EVENT),
            sys_receiver: EVENT_BUS.subscribe(TOPIC_SYSTEM_EVENT),
            port_scan: k_ai::security_detectors::PortScanDetector::new(),
            arp_spoof: k_ai::security_detectors::ArpSpoofDetector::new(),
            ping_flood: k_ai::security_detectors::PingFloodDetector::new(),
            dhcp_starvation: k_ai::security_detectors::DhcpStarvationDetector::new(),
            timer_anomaly: k_ai::security_detectors::TimerAnomalyDetector::new(),
            alerts: Vec::new(),
        }
    }

    /// Publish a structured alert to SECURITY_ALERT + Hermes
    fn publish_alert(&self, alert: &k_ai::security_detectors::SecurityAlert) {
        let msg = alloc::format!(
            "[SECURITY] {} (sev={:?}): {}",
            alert.detector,
            alert.severity,
            alert.message
        );
        let _ = EVENT_BUS.publish(Event {
            id: alert.timestamp,
            topic: String::from(TOPIC_SECURITY_ALERT),
            payload: msg.into_bytes(),
            token: CapabilityToken::Legacy(1),
        });
    }

    /// Feed a NET_EVENT payload to the appropriate real detector.
    /// Returns Some(alert) if the detector found something suspicious.
    fn feed_net_event(&mut self, payload: &[u8], tick: u64) {
        let text = match core::str::from_utf8(payload) {
            Ok(s) => s,
            Err(_) => return,
        };

        if let Some(rest) = text.strip_prefix("CONNECT ") {
            // PortScanDetector: src_ip + dst_port
            if let Some(src_ip) = parse_src_ip(rest) {
                let ip_u32 = u32::from_be_bytes(src_ip);
                let dst_port = parse_dst_port(rest).unwrap_or(0);
                if let Some(alert) = self.port_scan.feed(ip_u32, dst_port, tick) {
                    self.publish_alert(&alert);
                    self.alerts.push(alert);
                }
            }
        } else if let Some(rest) = text.strip_prefix("ICMP ") {
            // PingFloodDetector: src_ip
            if let Some(src_ip) = parse_src_ip(rest) {
                let ip_u32 = u32::from_be_bytes(src_ip);
                if let Some(alert) = self.ping_flood.feed(ip_u32, tick) {
                    self.publish_alert(&alert);
                    self.alerts.push(alert);
                }
            }
        } else if let Some(rest) = text.strip_prefix("ARP ") {
            // ArpSpoofDetector: src_ip + src_mac
            if let Some(src_ip) = parse_src_ip(rest) {
                let ip_u32 = u32::from_be_bytes(src_ip);
                let mac = parse_src_mac(rest).unwrap_or([0u8; 6]);
                if let Some(alert) = self.arp_spoof.feed(ip_u32, mac, tick) {
                    self.publish_alert(&alert);
                    self.alerts.push(alert);
                }
            }
        } else if let Some(rest) = text.strip_prefix("DHCP_DISCOVER ") {
            // DhcpStarvationDetector: src_mac
            if let Some(mac) = parse_dhcp_mac(rest) {
                if let Some(alert) = self.dhcp_starvation.feed(mac, tick) {
                    self.publish_alert(&alert);
                    self.alerts.push(alert);
                }
            }
        }
    }

    /// Feed a SYSTEM_EVENT payload to the appropriate real detector.
    fn feed_sys_event(&mut self, payload: &[u8], tick: u64) {
        let text = match core::str::from_utf8(payload) {
            Ok(s) => s,
            Err(_) => return,
        };

        if text.starts_with("TIMER ") {
            // TimerAnomalyDetector: call with tick
            if let Some(alert) = self.timer_anomaly.feed(tick) {
                self.publish_alert(&alert);
                self.alerts.push(alert);
            }
        } else if text.starts_with("DHCP_LEASE") {
            // DhcpStarvationDetector: track lease frequency
            if let Some(alert) = self.dhcp_starvation.feed_lease(tick) {
                self.publish_alert(&alert);
                self.alerts.push(alert);
            }
        }
    }

    /// Correlate multiple alerts: if 3+ alerts in short window, escalate.
    fn correlate(&mut self, tick: u64) {
        if self.alerts.len() >= 3 {
            use k_ai::security_detectors::AlertSeverity;
            let max_sev = self.alerts.iter().map(|a| match a.severity {
                AlertSeverity::Critical => 5,
                AlertSeverity::High => 4,
                AlertSeverity::Medium => 3,
                AlertSeverity::Low => 2,
            }).max().unwrap_or(0);

            k_nano::slog_hermes!(
                "Sec", "warn",
                "Correlacao: {} alertas, severidade max={}",
                self.alerts.len(), max_sev
            );

            if max_sev >= 4 {
                let msg = alloc::format!(
                    "ALERTA CRÍTICO: {} alertas correlacionados, severidade {}",
                    self.alerts.len(), max_sev
                );
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
                    payload: alloc::format!(
                        "[SECURITY] Correlacao: {} alertas, severidade {}",
                        self.alerts.len(), max_sev
                    ).into_bytes(),
                    token: CapabilityToken::Legacy(1),
                });
            }
            self.alerts.clear();
        }
    }
}

impl Agent for SecurityAgent {
    fn manifest(&self) -> &AgentManifest { &SEC_MANIFEST }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        let tick = TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;

        // Poll EventBus for NET_EVENT (TCP, ARP, ICMP, DHCP)
        while let Some(event) = self.net_receiver.try_receive() {
            self.feed_net_event(&event.payload, tick);
        }

        // Poll EventBus for SYSTEM_EVENT (timer drift)
        while let Some(event) = self.sys_receiver.try_receive() {
            self.feed_sys_event(&event.payload, tick);
        }

        // Periodic timer anomaly check (every 1000 ticks, self-contained)
        if tick > 1000 && tick % 1000 == 0 {
            if let Some(alert) = self.timer_anomaly.feed(tick) {
                self.publish_alert(&alert);
                self.alerts.push(alert);
            }
        }

        // Correlate alerts every 100 ticks
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
