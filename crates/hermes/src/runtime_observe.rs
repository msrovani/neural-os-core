//! Observe runtime (BOOT_OBSERVE / HEALTH / CORTEX_POSTURE) → HUD e HITL.
//! SESSION_273: Hermes não manda I5 SLIP nem recipe Escalate para o LLM.

use alloc::format;
use alloc::string::String;
use event_bus::{CapabilityToken, Event};
use k_nano::EVENT_BUS;

/// I5 SLIP e HITL recipe não viram chat LLM (spam + bypass HITL).
/// Mesh 6 (vazamento ~11KB/tick): I4 sched=Violation é lag do scheduler
/// CAUSADO pela carga — publicar "diagnostique e corrija" ao LLM amplifica a
/// carga (loop de feedback: lag→intento→LLM/Hermes/EventBus→mais lag). No
/// boot mesh o Master recebeu 233 intents vs ~35 nos workers = 7×. Observe-only.
pub fn should_escalate_health_to_llm(payload: &str) -> bool {
    if payload.contains("degraded_slip") || payload.contains(":I5:net:") {
        return false;
    }
    // s429-lab: boot_log sem backend é AMBIENTAL (QEMU sem stick MSC; HW real
    // sem enum a tempo) — o LLM não conserteia: só re-custo e re-intento
    // (posture FAIL x2 → churn de council/learner/wasm sob pressão = caminho
    // do panic wasmi "store owner mismatch" no 8c). Observe-only como I5:net.
    if payload.contains(":I5:boot_log:") {
        return false;
    }
    if payload.contains("recipe_escalate") || payload.contains("HITL:recipe") {
        return false;
    }
    // I4 scheduler-lag: sintoma de carga, não tarefa de diagnóstico.
    if payload.contains("sched=Violation") || payload.contains("sched=Warning") {
        return false;
    }
    true
}

/// HEALTH_ISSUE: memoriza; só firmware/skill ausente vira USER_INTENT.
/// s429-lab: gate de CLASSE no CALLER — sob pressão de memória (mesh frag)
/// NENHUM health escala (SESSION_410 (2): escalada amplifica a própria carga;
/// e o wasmi panica sob heap pressionado — o intent não é confiável aí).
/// A função de classificação continua PURA (testável sem estado global).
pub fn ingest_health_issue(payload: &str) {
    k_nano::slog_hermes!("Health", "info", "{}", payload);
    if k_nano::memory::mesh_frag_pressure() {
        k_nano::slog_hermes!(
            "Health",
            "info",
            "observe-only (frag pressure) — nao encaminha ao LLM"
        );
        return;
    }
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
