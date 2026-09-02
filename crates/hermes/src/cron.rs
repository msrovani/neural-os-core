//! Cron Scheduler — agendamento periódico de eventos via LAPIC timer.
//! Cada spec tem nome, intervalo em ticks, e ação (EventBus publish + mensagem).

use alloc::string::String;
use alloc::vec::Vec;
use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use k_nano::interrupts::TIMER_TICKS;
use k_nano::interrupts::TIMER_HZ;
use k_nano::EVENT_BUS;

const CRON_MANIFEST: AgentManifest = AgentManifest {
    name: "cron",
    kind: AgentKind::System,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

struct CronJob {
    name: String,
    interval: u64,     // ticks entre execuções
    last_run: u64,     // tick da última execução
    message: String,   // mensagem publicada no EventBus
    topic: String,     // tópico EventBus
}

pub struct CronAgent {
    jobs: Vec<CronJob>,
}

impl CronAgent {
    pub fn new() -> Self {
        let _tick = TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        CronAgent {
            jobs: Vec::new(),
        }
    }

    /// Registra um job de agendamento
    pub fn schedule(&mut self, name: &str, interval_ticks: u64, topic: &str, message: &str) {
        let tick = TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        self.jobs.push(CronJob {
            name: String::from(name),
            interval: interval_ticks,
            last_run: tick,
            message: String::from(message),
            topic: String::from(topic),
        });
        k_nano::slog_hermes!("Cron", "info", "Job '{}' agendado: a cada {} ticks", name, interval_ticks);
    }

    /// Executa ações programadas (health check, relatórios, etc.)
    /// Pode ser chamado para configurar jobs padrão
    pub fn init_defaults(&mut self) {
        self.schedule("health", 200, "CRON_HEALTH", "Health check");
        self.schedule("memory_report", 500, "CRON_REPORT", "Memory report");
        self.schedule("skill_review", 3000, "SKILL_REVIEW", "Comprehensive skill review");
        // NTP periodic resync every ~200s (3600 ticks at 18Hz)
        self.schedule("ntp_resync", 3600, "NTP_RESYNC", "NTP periodic resync");
        // OTA diário (ADR-0086): 86400s a 18Hz ≈ 1.555.200 ticks
        let hz = TIMER_HZ.load(core::sync::atomic::Ordering::Relaxed).max(1);
        self.schedule("update_check", 86400 * hz, "UPDATE_CHECK", "Daily OTA check");
        // T-026: telemetria periódica POST /api/logs com backoff (interval = backoff)
        // PIT ~18Hz → 2000 ticks ≈ 110s (mesmo que log_agent backoff)
        self.schedule("log_push", 2000, "LOG_PUSH", "Telemetry push");
        k_nano::slog_hermes!("Cron", "info", "{} jobs default registrados", self.jobs.len());
    }

    /// T-026: tenta POST /api/logs (best-effort). Reusa UPDATE.CFG para base URL.
    /// Backoff já é o intervalo do cron (2000 ticks ≈110s). Retorna texto para log.
    /// ponytail: sem spam se net down — retorna skip.
    pub fn try_log_push() -> String {
        let Some(url) = crate::self_update::read_update_cfg() else {
            return String::from("log_push skip: sem UPDATE.CFG");
        };
        // Extrai host:port do UPDATE_URL=http://host:port/UPDATE.MANIFEST
        let (host, port) = match parse_host_port(&url) {
            Some(v) => v,
            None => return String::from("log_push skip: URL invalida"),
        };
        // Log = ramlog snapshot se SGDB tiver, senão boot report filler
        let log_text = k_nano::boot_ramlog::snapshot().unwrap_or_else(|| String::from("[log_push: ramlog vazio]"));
        let body = log_text.as_bytes();
        // Monta POST /api/logs
        let req = build_log_post(host, port, body);
        match crate::net_bridge::tcp_xfer(host, port, &req) {
            Some(resp) => {
                let code = if resp.len() > 12 {
                    String::from_utf8_lossy(&resp[9..12]).into_owned()
                } else {
                    String::from("200")
                };
                alloc::format!("log_push HTTP {} ({} bytes)", code, body.len())
            }
            None => String::from("log_push FAIL (rede indisponivel)"),
        }
    }

    /// Executa revisão comprehensive de observações (função livre, sem borrow)
    pub fn run_review() {
        let pending = crate::skill_observer::pending_observations();
        let count = pending.len();
        if count == 0 { return; }

        k_nano::slog_hermes!("REVIEW", "info", "Running comprehensive review ({} open observations)", count);
        // Sprint 108: review só sinaliza; SelfEvolveAgent registra no SKILL_STORAGE do bin.
        for obs in &pending {
            if obs.skill.starts_with("New skill candidate:") {
                let name = obs.skill.trim_start_matches("New skill candidate:").trim();
                // Garante padrão em skill_gen para maybe_auto_skill
                crate::skill_gen::record_task(name, &obs.suggestion, &["review", "generate", "verify"]);
                k_nano::slog_hermes!("REVIEW", "info", "candidate '{}' (obs #{}) queued for S108", name, obs.number);
            }
        }
        let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
            id: 0,
            topic: alloc::string::String::from(crate::self_evolve::TOPIC_SELF_EVOLVE),
            payload: b"skill_review".to_vec(),
            token: event_bus::CapabilityToken::Legacy(1),
        });
        k_nano::slog_hermes!("REVIEW", "info", "Review complete. {} observations scanned.", count);
    }
}

// ─── T-026 helpers (POST /api/logs) ───────────────────────────────────────────
fn parse_host_port(url: &str) -> Option<([u8; 4], u16)> {
    let rest = url.strip_prefix("http://").or_else(|| url.strip_prefix("HTTP://"))?;
    let hostport = rest.split('/').next().unwrap_or(rest);
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().unwrap_or(8080)),
        None => (hostport, 8080),
    };
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() != 4 { return None; }
    let mut ip = [0u8; 4];
    for (i, p) in parts.iter().enumerate() {
        ip[i] = p.parse::<u8>().ok()?;
    }
    Some((ip, port))
}

fn build_log_post(host: [u8; 4], port: u16, body: &[u8]) -> Vec<u8> {
    let mut req = Vec::new();
    req.extend_from_slice(b"POST /api/logs HTTP/1.1\r\n");
    req.extend_from_slice(
        alloc::format!("Host: {}.{}.{}.{}\r\n", host[0], host[1], host[2], host[3]).as_bytes(),
    );
    req.extend_from_slice(b"Content-Type: text/plain\r\n");
    req.extend_from_slice(alloc::format!("Content-Length: {}\r\n", body.len()).as_bytes());
    req.extend_from_slice(b"Connection: close\r\n\r\n");
    req.extend_from_slice(body);
    let _ = port;
    req
}

#[cfg(test)]
mod cron_tests {
    use super::{build_log_post, parse_host_port};

    #[test]
    fn host_port_parse() {
        let (ip, port) = parse_host_port("http://10.0.2.2:8080/UPDATE.MANIFEST").unwrap();
        assert_eq!(ip, [10, 0, 2, 2]);
        assert_eq!(port, 8080);
        let (ip, port) = parse_host_port("http://192.168.137.1:8080/UPDATE.MANIFEST").unwrap();
        assert_eq!(ip, [192, 168, 137, 1]);
        assert!(parse_host_port("http://bad/UPDATE.MANIFEST").is_none());
    }

    #[test]
    fn post_has_length() {
        let req = build_log_post([10, 0, 2, 2], 8080, b"hello");
        let s = String::from_utf8_lossy(&req);
        assert!(s.starts_with("POST /api/logs HTTP/1.1"));
        assert!(s.contains("Content-Length: 5"));
        assert!(s.ends_with("hello"));
    }
}

impl Agent for CronAgent {
    fn manifest(&self) -> &AgentManifest { &CRON_MANIFEST }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        let now = TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        let _ = _tick; let _ = _count;
        // Labor 10: uma tentativa NTP se ainda não synced (non-fatal)
        if !crate::ntp::is_synced() && now > 200 {
            let _ = crate::ntp::try_sync();
        }
        for job in &mut self.jobs {
            if now >= job.last_run + job.interval {
                job.last_run = now;
                if let Some(u) = crate::ntp::now_unix() {
                    k_nano::slog_hermes!("Cron", "info", "Job '{}' disparado @ tick {} unix={}", job.name, now, u);
                } else {
                    k_nano::slog_hermes!("Cron", "info", "Job '{}' disparado @ tick {}", job.name, now);
                }

                // Skill review é executado inline, não via EventBus
                if job.name == "skill_review" {
                    Self::run_review();
                    continue;
                }
                // NTP periodic resync
                if job.name == "ntp_resync" {
                    let _ = crate::ntp::try_sync();
                    continue;
                }
                // OTA diário (ADR-0086) — check inline (UPDATE.CFG -> manifest -> slot A/B)
                if job.name == "update_check" {
                    let r = crate::self_update::check_for_update();
                    k_nano::slog_hermes!("Cron", "info", "update_check: {}", r);
                    continue;
                }
                // T-026: LogAgent POST /api/logs com backoff (reuse cron interval = 2000 ticks)
                if job.name == "log_push" {
                    let r = Self::try_log_push();
                    k_nano::slog_hermes!("Cron", "info", "log_push: {}", r);
                    continue;
                }

                let _ = EVENT_BUS.publish(event_bus::Event {
                    id: 0,
                    topic: job.topic.clone(),
                    payload: job.message.as_bytes().to_vec(),
                    token: event_bus::CapabilityToken::Legacy(1),
                });
            }
        }
        AgentTickResult::Pending
    }
}






