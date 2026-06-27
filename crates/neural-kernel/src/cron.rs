//! Cron Scheduler — agendamento periódico de eventos via LAPIC timer.
//! Cada spec tem nome, intervalo em ticks, e ação (EventBus publish + mensagem).

use alloc::string::String;
use alloc::vec::Vec;
use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use crate::interrupts::TIMER_TICKS;
use crate::{serial_println, println};
use crate::EVENT_BUS;

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
        let tick = TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
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
        serial_println!("[CRON] Job '{}' agendado: a cada {} ticks", name, interval_ticks);
    }

    /// Executa ações programadas (health check, relatórios, etc.)
    /// Pode ser chamado para configurar jobs padrão
    pub fn init_defaults(&mut self) {
        self.schedule("health", 200, "CRON_HEALTH", "Health check");
        self.schedule("memory_report", 500, "CRON_REPORT", "Memory report");
        serial_println!("[CRON] {} jobs default registrados", self.jobs.len());
    }
}

impl Agent for CronAgent {
    fn manifest(&self) -> &AgentManifest { &CRON_MANIFEST }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        let now = TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        let _ = _tick; let _ = _count;
        for job in &mut self.jobs {
            if now >= job.last_run + job.interval {
                job.last_run = now;
                serial_println!("[CRON] Job '{}' disparado @ tick {}", job.name, now);
                let _ = EVENT_BUS.publish(crate::Event {
                    id: 0,
                    topic: job.topic.clone(),
                    payload: job.message.as_bytes().to_vec(),
                    token: crate::CapabilityToken(1),
                });
            }
        }
        AgentTickResult::Pending
    }
}
