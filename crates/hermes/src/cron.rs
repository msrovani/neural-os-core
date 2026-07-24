//! Cron Scheduler — agendamento periódico de eventos via LAPIC timer.
//! Cada spec tem nome, intervalo em ticks, e ação (EventBus publish + mensagem).

use alloc::string::String;
use alloc::vec::Vec;
use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use k_nano::interrupts::TIMER_TICKS;
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
        k_nano::slog_hermes!("Cron", "info", "{} jobs default registrados", self.jobs.len());
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
