//! HITL UI — Hermes pede intervenção; Jarbas é a superfície padrão.
//! Preferência: Jarbas (persona/FB/voz) ou Terminal estilo HANR (slash /xxx).

use alloc::format;
use alloc::string::String;
use core::sync::atomic::{AtomicU8, Ordering};
use k_nano::EVENT_BUS;

/// EventBus: Hermes → Jarbas (precisa UI / confirmação).
pub const TOPIC_HITL_REQUEST: &str = "HITL_REQUEST";
/// EventBus: abrir/focar terminal HANR-style.
pub const TOPIC_HITL_TERMINAL: &str = "HITL_TERMINAL";

const MODE_JARBAS: u8 = 0;
const MODE_TERMINAL: u8 = 1;

static HITL_MODE: AtomicU8 = AtomicU8::new(MODE_JARBAS);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitlMode {
    /// Default: Jarbas interage (avatar/FB/voz/overlay).
    Jarbas,
    /// Terminal estilo HANR com catálogo /xxx.
    Terminal,
}

impl HitlMode {
    pub fn as_str(self) -> &'static str {
        match self {
            HitlMode::Jarbas => "jarbas",
            HitlMode::Terminal => "terminal",
        }
    }
}

pub fn current_mode() -> HitlMode {
    if HITL_MODE.load(Ordering::Relaxed) == MODE_TERMINAL {
        HitlMode::Terminal
    } else {
        HitlMode::Jarbas
    }
}

pub fn set_mode(mode: HitlMode) {
    let v = match mode {
        HitlMode::Jarbas => MODE_JARBAS,
        HitlMode::Terminal => MODE_TERMINAL,
    };
    HITL_MODE.store(v, Ordering::Relaxed);
    k_nano::slog_hermes!("HITL", "info", "ui_mode={}", mode.as_str());
}

pub fn set_mode_str(s: &str) -> Result<HitlMode, &'static str> {
    let m = match s.trim().to_ascii_lowercase().as_str() {
        "jarbas" | "jarvis" | "ui" | "persona" => HitlMode::Jarbas,
        "terminal" | "term" | "cli" | "hanr" | "slash" => HitlMode::Terminal,
        _ => return Err("use: /ui jarbas | /ui terminal"),
    };
    set_mode(m);
    Ok(m)
}

/// Catálogo de slash commands (HANR-style) — copiáveis no terminal.
pub fn slash_catalog() -> &'static str {
    "\
=== Neural Hermes — slash commands (HANR-style) ===\n\
  /help                 esta ajuda\n\
  /commands             catálogo completo\n\
  /ui jarbas|terminal   preferência HITL (default=jarbas)\n\
  /status /hw /netdiag /usage /conv\n\
  /echo <txt> /ping <ip> /fetch <url>\n\
  /model <path|url> /trust allow|deny <tok> <skill>\n\
  /skills               L0 índice curto\n\
  /skill <name>         L1 corpo SKILL.md\n\
  /remember <fato>      MEMORY.md\n\
  /soul [texto]         SOUL Hermes (orquestração)\n\
  /persona [texto]      PERSONA Jarbas (tom/voz/FB)\n\
  /memory               fatia USER+MEMORY+SOUL\n\
  /search <q>           session FTS5-lite + BGE\n\
  /budget [N]           IterationBudget\n\
  /cog                  status cognitivo\n\
  /market               loja local\n\
  /market search <q>\n\
  /market install <kind> <name> [body]\n\
  /market promote <name>\n\
  /market rm <kind> <name>\n\
  /market fetch <kind> <name> <http://ip/...>\n\
  /market index\n\
  /pkg catalog|list|get|install|update|rm\n\
  /approve <id>         HITL sim\n\
  /deny <id>            HITL não\n\
  /pending              listar aprovações\n\
  /mcp tools/list | /mcp {json-rpc}\n\
  /add_skill /learn /rm_skill /reload_skills\n\
---\n\
HITL: Hermes decide; Jarbas mostra (ou terminal se /ui terminal).\n\
SOUL Hermes ≠ PERSONA Jarbas (tom/voz/FB).\n"
}

/// Notifica o usuário que precisa intervir.
/// - Modo Jarbas: publica HITL_REQUEST (Jarbas abre overlay / fala).
/// - Modo Terminal: publica HITL_TERMINAL + catálogo /xxx + pending hint.
pub fn request_user_intervention(
    approval_id: u64,
    skill: &str,
    agent: &str,
    reason: &str,
    level: &str,
) {
    let summary = format!(
        "HITL #{} level={} agent={} skill={} — {}\n\
         /approve {}   ou   /deny {}\n",
        approval_id, level, agent, skill, reason, approval_id, approval_id
    );

    match current_mode() {
        HitlMode::Jarbas => {
            let payload = format!(
                "{{\"id\":{},\"level\":\"{}\",\"agent\":\"{}\",\"skill\":\"{}\",\"reason\":\"{}\",\
                 \"hint\":\"/approve {}|/deny {}\",\"mode\":\"jarbas\"}}",
                approval_id,
                level,
                agent,
                skill,
                reason.replace('"', "'"),
                approval_id,
                approval_id
            );
            let _ = EVENT_BUS.publish(event_bus::Event {
                id: approval_id,
                topic: String::from(TOPIC_HITL_REQUEST),
                payload: payload.into_bytes(),
                token: event_bus::CapabilityToken::Legacy(1),
            });
            // Espelho textual para HermesChat
            let _ = EVENT_BUS.publish(event_bus::Event {
                id: 0,
                topic: String::from(crate::hermes::TOPIC_HERMES_RESPONSE),
                payload: format!(
                    "[Jarbas] Preciso da sua confirmação:\n{}",
                    summary
                )
                .into_bytes(),
                token: event_bus::CapabilityToken::Legacy(1),
            });
            k_nano::slog_hermes!("HITL", "info", "→ jarbas id={}", approval_id);
        }
        HitlMode::Terminal => {
            let mut msg = String::from("[TERMINAL HITL — estilo HANR]\n");
            msg.push_str(&summary);
            msg.push_str(slash_catalog());
            let _ = EVENT_BUS.publish(event_bus::Event {
                id: approval_id,
                topic: String::from(TOPIC_HITL_TERMINAL),
                payload: msg.as_bytes().to_vec(),
                token: event_bus::CapabilityToken::Legacy(1),
            });
            let _ = EVENT_BUS.publish(event_bus::Event {
                id: 0,
                topic: String::from(crate::hermes::TOPIC_HERMES_RESPONSE),
                payload: msg.into_bytes(),
                token: event_bus::CapabilityToken::Legacy(1),
            });
            k_nano::slog_hermes!("HITL", "info", "→ terminal id={}", approval_id);
        }
    }
}

pub fn mode_status() -> String {
    format!(
        "[HITL] ui_mode={} — /ui jarbas | /ui terminal\n{}",
        current_mode().as_str(),
        "Jarbas=persona/FB/voz; Terminal=slash /xxx estilo HANR"
    )
}

/// Lista IDs pendentes formatados para o terminal.
pub fn pending_hints() -> String {
    let gate = crate::globals::APPROVAL_GATE.lock();
    let pending = gate.pending();
    if pending.is_empty() {
        return String::from("[HITL] nenhuma pendência");
    }
    let mut s = format!("[HITL] {} pendente(s):\n", pending.len());
    for r in pending {
        s.push_str(&format!(
            "  #{} [{}] {} / {} — /approve {} | /deny {}\n",
            r.id,
            r.required_level.name(),
            r.agent,
            r.skill,
            r.id,
            r.id
        ));
    }
    s
}

pub fn open_terminal_help() -> String {
    let mut s = String::from(slash_catalog());
    s.push_str(&pending_hints());
    s.push_str(&mode_status());
    // Foca overlay terminal via EventBus
    let _ = EVENT_BUS.publish(event_bus::Event {
        id: 0,
        topic: String::from(TOPIC_HITL_TERMINAL),
        payload: s.as_bytes().to_vec(),
        token: event_bus::CapabilityToken::Legacy(1),
    });
    s
}
