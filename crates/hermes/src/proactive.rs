//! Proactive Heartbeats (IDEA #315.14).
//! JARVIS inicia conversa quando o sistema está ocioso.
//! Gatilho: CronAgent detecta idle > 30s sem interação do usuário.

use alloc::string::String;
use core::sync::atomic::Ordering;
use k_nano::EVENT_BUS;

/// Tópico para mensagens proativas do JARBAS.
pub const TOPIC_JARBAS_PROACTIVE: &str = "JARBAS_PROACTIVE";

/// Tipos de heartbeat proativo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProactiveType {
    Greeting,
    Reminder,
    Suggestion,
    StatusReport,
}

/// Gera uma mensagem proativa baseada no contexto do sistema.
pub fn generate_proactive(msg_type: ProactiveType) -> Option<String> {
    let msg = match msg_type {
        ProactiveType::Greeting => {
            let hour = current_hour();
            let greeting = if hour < 12 { "Bom dia" } else if hour < 18 { "Boa tarde" } else { "Boa noite" };
            alloc::format!("{}! Como posso ajudar?", greeting)
        }
        ProactiveType::Reminder => {
            // TODO: check pending reminders
            return None; // No reminders yet
        }
        ProactiveType::Suggestion => {
            // Simple suggestions based on time of day
            let hour = current_hour();
            if hour < 12 {
                String::from("Que tal revisar seus emails esta manhã?")
            } else if hour < 14 {
                String::from("Horário de almoço! Posso sugerir uma receita?")
            } else if hour < 18 {
                String::from("Tarde produtiva! Precisa de algo?")
            } else {
                String::from("Boa noite! Quer que eu prepare algo para amanhã?")
            }
        }
        ProactiveType::StatusReport => {
            let uptime = get_uptime_ticks();
            let n_ag = agent_core::LAST_SCHED_AGENTS.load(Ordering::Relaxed);
            String::from(alloc::format!(
                "Sistema estável há {} ticks. {} agentes ativos (nativos + carregados sob demanda, sandbox WASM).",
                uptime, n_ag
            ))
        }
    };
    Some(msg)
}

/// Publica uma mensagem proativa no EventBus.
pub fn publish_proactive(msg: &str) {
    let _ = EVENT_BUS.publish(event_bus::Event {
        id: 0,
        topic: String::from(TOPIC_JARBAS_PROACTIVE),
        payload: msg.as_bytes().to_vec(),
        token: event_bus::CapabilityToken::Legacy(1),
    });
}

fn current_hour() -> u8 {
    // Based on tick counter (approximate, ~1000 ticks/sec)
    let ticks = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
    ((ticks / 3600000) % 24) as u8
}

fn get_uptime_ticks() -> u64 {
    k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64
}
