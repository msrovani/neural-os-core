//! LogAnalystAgent — analisa logs do /logs/ via Cortex LLM.
//! Cada agente/skill escreve em /logs/<agent>/<tick>.log.
//! Este agente le, usa o Cortex para extrair padroes, anomalias, insights.
//! Se detecta necessidade de intervencao, publica no EventBus.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use alloc::string::String;
const LOG_ANALYST_MANIFEST: AgentManifest = AgentManifest {
    name: "log_analyst",
    kind: AgentKind::Skill,
    schedule: ScheduleKind::PollEvery(500),
    auto_start: true,
    persist: true,
};

/// Escreve log de um agente/skill no /logs/
pub fn write_log(agent: &str, msg: &str) {
    let tick = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
    let path = alloc::format!("/logs/{}/{}.log", agent, tick);
    let payload = alloc::format!("[T+{}] {}\n", tick, msg);
    let _ = crate::fs::write_vfs(&path, payload.as_bytes());
}

pub struct LogAnalystAgent;

impl LogAnalystAgent {
    pub fn new() -> Self {
        k_nano::slog_bin!("LOG", "ANALYST", "/logs/ analyst ativo.");
        LogAnalystAgent
    }

    /// Le logs recentes e usa o Cortex para analisar
    fn analyze_logs(&self) {
        // Le lista de agentes com logs em /logs/
        let agents = match crate::fs::list_vfs("/logs") {
            Ok(agents) => agents,
            _ => return,
        };

        let mut combined = String::new();
        for agent in &agents {
            let dir = alloc::format!("/logs/{}", agent);
            if let Ok(files) = crate::fs::list_vfs(&dir) {
                for f in files.iter().rev().take(5) {
                    let path = alloc::format!("/logs/{}/{}", agent, f);
                    if let Ok(data) = crate::fs::read_vfs(&path) {
                        if let Ok(text) = core::str::from_utf8(&data) {
                            combined.push_str(text);
                        }
                    }
                }
            }
        }

        if combined.is_empty() { return; }

        // Envia para o Cortex analisar
        let prompt = alloc::format!(
            "Analise estes logs do sistema e identifique:\n\
             1) Anomalias ou erros\n\
             2) Padroes recorrentes\n\
             3) Recomendacoes de melhoria\n\
             4) Metricas de saude\n\
             Logs:\n{}", combined);

        let _ = k_nano::globals::EVENT_BUS.publish(event_bus::Event {
            id: 0,
            topic: alloc::string::String::from(cortex::cortex::TOPIC_LLM_REQUEST),
            payload: prompt.into_bytes(),
            token: event_bus::CapabilityToken::Legacy(1),
        });
    }
}

impl Agent for LogAnalystAgent {
    fn manifest(&self) -> &AgentManifest { &LOG_ANALYST_MANIFEST }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        // ponytail: scheduler já garante PollEvery(500), sem rate-limiting interno
        self.analyze_logs();
        AgentTickResult::Pending
    }
}


