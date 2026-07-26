//! HW plug-and-play agentico — Hermes decide o que fazer com o card.
//! Fluxo: HwCapabilityCard → observe (S108) → skill efêmera (SkillOpt)
//! → com uso rotineiro (≥3, ≥70%) promove a WASM (evolve runtime).
//! Hint `next_action` do detect é sugestão, NÃO ordem hardcoded.

use alloc::format;
use alloc::string::String;

use crate::evolve;
use crate::self_evolve;
use crate::skill_gen;
use crate::skill_opt;

/// Resultado da decisão agentica do Hermes sobre um card.
pub struct PnpDecision {
    pub skill_key: String,
    /// Se true, publicar USER_INTENT para Cortex/ReAct decidir de verdade.
    pub escalate_to_llm: bool,
    pub user_intent: Option<String>,
    pub ack: String,
    pub promoted_wasm: bool,
    pub auto_skill_md: Option<String>,
}

fn wire_field(wire: &str, key: &str) -> String {
    let prefix = format!("{}=", key);
    for part in wire.split(';') {
        if let Some(v) = part.strip_prefix(prefix.as_str()) {
            return String::from(v);
        }
    }
    String::from("-")
}

/// Hermes decide a partir do wire `HW_CAPABILITY` (ou action curta `next|agent|fw`).
pub fn hermes_decide_card(payload: &str, tick: u64) -> PnpDecision {
    let (family, next, agent, fw, card_ctx) = if payload.contains("vid=") {
        (
            wire_field(payload, "family"),
            wire_field(payload, "next"),
            wire_field(payload, "agent"),
            wire_field(payload, "fw"),
            String::from(payload),
        )
    } else {
        // Fallback HW_PNP_ACTION: next|agent|fw
        let mut parts = payload.split('|');
        let next = String::from(parts.next().unwrap_or("observe_only"));
        let agent = String::from(parts.next().unwrap_or("?"));
        let fw = String::from(parts.next().unwrap_or("-"));
        (
            String::from("unknown"),
            next.clone(),
            agent,
            fw,
            format!("action_only next={} agent={}", next, payload),
        )
    };

    let intent_text = format!("hw_pnp {} {} agent={}", family, next, agent);
    self_evolve::observe_intent(&intent_text, tick);
    let skill_key = self_evolve::normalize_intent(&intent_text);

    // Skill efêmera: receita textual (ainda não WASM). Cada card = uso.
    let ephemeral_src = format!(
        "# ephemeral hw_pnp\n\
         family: {family}\n\
         next_hint: {next}\n\
         agent: {agent}\n\
         fw: {fw}\n\
         card: {card_ctx}\n\
         rules: honest bind only; no fake wifi scan; no generic MMIO doorbells; \
         NEED_FW → HEALTH/SelfHeal; Ready → acknowledge boot path.\n"
    );
    skill_opt::record_python_run(&skill_key, &ephemeral_src, true);

    let mut promoted_wasm = false;
    if skill_opt::maybe_promote_to_wasm(&skill_key).is_some() {
            match evolve::promote_ephemeral_to_wasm(&skill_key, &ephemeral_src) {
            Ok(()) => {
                promoted_wasm = true;
                k_nano::slog_hermes!("PnP", "info", "skill '{}' promovida efêmera→WASM (uso rotineiro)", skill_key);
            }
            Err(e) => {
                k_nano::slog_hermes!("PnP", "info", "promote WASM '{}' falhou: {}", skill_key, e);
            }
        }
    }

    let auto_skill_md = skill_gen::maybe_auto_skill(&skill_key);

    // HalOffer: qualquer next_action de bind → query/bind sem MMIO
    let mut offer_ack: Option<String> = None;
    if let Some(r) = crate::hal_offer::request_from_pnp_next(next.as_str(), agent.as_str()) {
        offer_ack = Some(r.ack);
    }

    // Hint do detect: Ready/BindNetwork já no boot. NEED_FW já vai por HEALTH_ISSUE.
    // Só escala Cortex quando Hermes precisa decidir bind/uso não-trivial.
    let escalate = matches!(
        next.as_str(),
        "bind_wifi_scan" | "bind_gpu_compute"
    );

    let user_intent = if escalate {
        Some(format!(
            "HW plug-and-play. Card: {card_ctx}\n\
             Hint operacional (heurística do detect, NÃO ordem): {next}\n\
             Agent sugerido: {agent} | fw={fw} | family={family}\n\
             HalOffer já consultado — decida o próximo passo HONESTO: \
             (1) usar topic FE se Bound, \
             (2) pedir skill efêmera, \
             (3) só observar. \
             NÃO invente scan WiFi, frame UVC real, MMIO genérico ou FW como sucesso."
        ))
    } else {
        None
    };

    let ack = if let Some(ref oa) = offer_ack {
        format!(
            "[Hermes-PnP] {} family={} → {}",
            next, family, oa
        )
    } else if promoted_wasm {
        format!(
            "[Hermes-PnP] {} family={} → skill '{}' → WASM",
            next, family, skill_key
        )
    } else if escalate {
        format!(
            "[Hermes-PnP] {} family={} → decide via Cortex (skill efêmera '{}')",
            next, family, skill_key
        )
    } else {
        format!(
            "[Hermes-PnP] {} family={} → observe/boot path (skill efêmera '{}')",
            next, family, skill_key
        )
    };

    PnpDecision {
        skill_key,
        escalate_to_llm: escalate,
        user_intent,
        ack,
        promoted_wasm,
        auto_skill_md,
    }
}






