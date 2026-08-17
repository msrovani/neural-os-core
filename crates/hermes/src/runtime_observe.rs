//! Observe runtime (BOOT_OBSERVE / HEALTH / CORTEX_POSTURE) → HUD e HITL.
//! SESSION_273: Hermes não manda I5 SLIP nem recipe Escalate para o LLM.

use alloc::format;
use alloc::string::String;
use event_bus::{CapabilityToken, Event};
use k_nano::EVENT_BUS;

/// I5 SLIP e HITL recipe não viram chat LLM (spam + bypass HITL).
pub fn should_escalate_health_to_llm(payload: &str) -> bool {
    if payload.contains("degraded_slip") || payload.contains(":I5:net:") {
        return false;
    }
    if payload.contains("recipe_escalate") || payload.contains("HITL:recipe") {
        return false;
    }
    true
}

/// HEALTH_ISSUE: memoriza; só firmware/skill ausente vira USER_INTENT.
pub fn ingest_health_issue(payload: &str) {
    k_nano::slog_hermes!("Health", "info", "{}", payload);
    if !should_escalate_health_to_llm(payload) {
        k_nano::slog_hermes!(
            "Health",
            "info",
            "observe-only (HITL/degraded) — nao encaminha ao LLM"
        );
        return;
    }
    let _ = EVENT_BUS.publish(Event {
        id: 0,
        topic: String::from(crate::hermes::TOPIC_USER_INTENT),
        payload: format!("diagnostique e corrija: {}", payload).into_bytes(),
        token: CapabilityToken::Legacy(1),
    });
}

/// Linha honesta para greeting / compositor (lê fontes, não cache mentiroso).
pub fn hud_line() -> String {
    let net = k_nano::env::net_hud_label();
    let llm = if cortex::cortex::model_is_loaded() {
        "llm"
    } else {
        "no-llm"
    };
    let moe = if cortex::trinity::moe_posture_trained() {
        "MoE"
    } else {
        "kw"
    };
    format!("{} {} {}", net, llm, moe)
}

#[cfg(test)]
mod tests {
    use super::should_escalate_health_to_llm;

    #[test]
    fn slip_and_recipe_hitl_do_not_page_llm() {
        assert!(!should_escalate_health_to_llm(
            "HEALTH_ISSUE:I5:net:degraded_slip_sandbox"
        ));
        assert!(!should_escalate_health_to_llm(
            "HEALTH_ISSUE:HITL:recipe_escalate"
        ));
        assert!(should_escalate_health_to_llm(
            "HEALTH_ISSUE:I3:10DE:1C82:firmware_hint:gp108"
        ));
    }
}
