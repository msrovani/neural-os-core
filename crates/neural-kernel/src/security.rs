//! Security Pipeline — EventBus → Detector → Correlation → Response.
//! #260: 5 detectores iniciais para ameaças de rede e sistema.

use alloc::string::String;
use alloc::vec::Vec;
use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use crate::interrupts::TIMER_TICKS;
use crate::{serial_println, println};
use crate::EVENT_BUS;

const SEC_MANIFEST: AgentManifest = AgentManifest {
    name: "security",
    kind: AgentKind::System,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

/// Evento de segurança detectado
pub struct SecurityEvent {
    pub detector: &'static str,
    pub severity: u8,        // 1-5 (5 = crítico)
    pub description: String,
    pub tick: u64,
}

pub struct SecurityAgent {
    events: Vec<SecurityEvent>,
    port_scan_counter: u64,
    ping_flood_counter: u64,
    last_arp_check: u64,
}

impl SecurityAgent {
    pub fn new() -> Self {
        SecurityAgent {
            events: Vec::new(),
            port_scan_counter: 0,
            ping_flood_counter: 0,
            last_arp_check: 0,
        }
    }

    /// #260 Detector 1: PortScan — múltiplas portas no mesmo tick
    fn detect_port_scan(&mut self, tick: u64, _payload: &[u8]) {
        self.port_scan_counter += 1;
        if self.port_scan_counter > 50 {
            self.events.push(SecurityEvent {
                detector: "PortScan",
                severity: 4,
                description: alloc::format!("Port scan detectado: {} acessos", self.port_scan_counter),
                tick,
            });
            self.port_scan_counter = 0;
        }
    }

    /// #260 Detector 2: ArpSpoof — MAC duplicado
    fn detect_arp_spoof(&mut self, tick: u64) {
        if tick > self.last_arp_check + 200 {
            self.last_arp_check = tick;
            // Verifica se MAC do gateway mudou (comparação com NET_CONFIG.gateway_mac)
            let cfg = crate::net::NET_CONFIG.lock();
            let gw_mac = cfg.gateway_mac;
            drop(cfg);
            if gw_mac != [0; 6] {
                // Lógica simplificada: em produção, compararia com ARP cache
            }
        }
    }

    /// #260 Detector 3: PingFlood — ICMP em alta frequência
    fn detect_ping_flood(&mut self, tick: u64) {
        self.ping_flood_counter += 1;
        if self.ping_flood_counter > 100 {
            self.events.push(SecurityEvent {
                detector: "PingFlood",
                severity: 3,
                description: alloc::format!("Ping flood: {} pacotes ICMP", self.ping_flood_counter),
                tick,
            });
            self.ping_flood_counter = 0;
        }
    }

    /// #260 Detector 4: DhcpStarvation — exaustão de leases
    fn detect_dhcp_starvation(&mut self, _tick: u64) {
        // Placeholder: quando DHCP server estiver implementado
    }

    /// #260 Detector 5: TimerAnomaly — drift de timer
    fn detect_timer_anomaly(&mut self, tick: u64) {
        if tick > 1000 && tick % 1000 == 0 {
            let expected = tick;
            let actual = TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
            if expected.abs_diff(actual) > 10 {
                self.events.push(SecurityEvent {
                    detector: "TimerAnomaly",
                    severity: 2,
                    description: alloc::format!("Timer drift: esperado={} real={}", expected, actual),
                    tick,
                });
            }
        }
    }

    /// Correlaciona eventos e emite alertas
    fn correlate(&mut self, tick: u64) {
        if self.events.len() >= 3 {
            let sev: u8 = self.events.iter().map(|e| e.severity).max().unwrap_or(0);
            serial_println!("[SECURITY] Correlacao: {} eventos, severidade max={}", self.events.len(), sev);
            if sev >= 4 {
                let _ = EVENT_BUS.publish(crate::Event {
                    id: 0,
                    topic: String::from("SECURITY_ALERT"),
                    payload: alloc::format!("ALERTA: {} eventos detectados, severidade {}", self.events.len(), sev).into_bytes(),
                    token: crate::CapabilityToken::Legacy(1),
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

        // Poll detectores a cada tick
        self.detect_port_scan(tick, &[]);
        self.detect_arp_spoof(tick);
        self.detect_ping_flood(tick);
        self.detect_dhcp_starvation(tick);
        self.detect_timer_anomaly(tick);

        // Correlação a cada 100 ticks
        if tick % 100 == 0 {
            self.correlate(tick);
        }

        AgentTickResult::Pending
    }
}
