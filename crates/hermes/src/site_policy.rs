//! ADR-0106 D2 — Política de θ por classe de sítio (conversacional vs ação).
//!
//! R3 aplica dentro da faixa sancionada por R2 (`cortex::decision`).
//! `Deny` nunca sobe por confiança.

use cortex::decision::{
    Confidence, Decision, DecisionSource, Noul, OutcomeKind, note_outcome, pinned_theta,
    theta_for_site,
};

use crate::approval::ApprovalLevel;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SiteClass {
    /// Abster → LLM (não HITL).
    Conversational,
    /// Abster → Confirm/Escalate HITL.
    Action,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyAction {
    Auto,
    Confirm,
    Escalate,
    Deny,
    /// Só sítio conversacional.
    RouteLlm,
}

#[derive(Clone, Copy, Debug)]
pub struct SitePolicy {
    pub site: &'static str,
    pub class: SiteClass,
    pub destructive: bool,
    pub theta_auto: u8,
    pub theta_review: u8,
}

impl SitePolicy {
    pub const INTENT_THINK: Self = Self {
        site: "intent.think",
        class: SiteClass::Conversational,
        destructive: false,
        theta_auto: 140,
        theta_review: 100,
    };

    pub const SKILL_RISK: Self = Self {
        site: "skill.risk",
        class: SiteClass::Action,
        destructive: true,
        theta_auto: 180,
        theta_review: 120,
    };

    pub const WASM_HOST: Self = Self {
        site: "wasm.host",
        class: SiteClass::Action,
        destructive: true,
        theta_auto: 170,
        theta_review: 110,
    };

    pub const COMPUTE_TIER: Self = Self {
        site: "compute.tier",
        class: SiteClass::Conversational,
        destructive: false,
        theta_auto: 130,
        theta_review: 90,
    };

    pub const PACKAGE_OP: Self = Self {
        site: "package.op",
        class: SiteClass::Action,
        destructive: true,
        theta_auto: 190,
        theta_review: 130,
    };
}

/// Aplica decisão tipada à política do sítio.
pub fn apply_confidence(
    policy: SitePolicy,
    confidence: Confidence,
    abstained: bool,
    policy_deny: bool,
) -> PolicyAction {
    if policy_deny {
        return PolicyAction::Deny;
    }

    let (mut auto_t, mut review_t, _trusted) =
        theta_for_site(policy.site, policy.theta_auto, policy.theta_review);
    if let Some((a, r)) = pinned_theta() {
        auto_t = a;
        review_t = r;
    }

    if abstained {
        return match policy.class {
            SiteClass::Conversational => {
                note_outcome(policy.site, OutcomeKind::Abstain);
                PolicyAction::RouteLlm
            }
            SiteClass::Action => {
                note_outcome(policy.site, OutcomeKind::Escalate);
                if policy.destructive {
                    PolicyAction::Escalate
                } else {
                    PolicyAction::Confirm
                }
            }
        };
    }

    let c = confidence.0;
    if c >= auto_t {
        note_outcome(policy.site, OutcomeKind::AutoOk);
        PolicyAction::Auto
    } else if c >= review_t {
        note_outcome(policy.site, OutcomeKind::Escalate);
        PolicyAction::Confirm
    } else {
        note_outcome(policy.site, OutcomeKind::Escalate);
        match policy.class {
            SiteClass::Conversational => PolicyAction::RouteLlm,
            SiteClass::Action => PolicyAction::Escalate,
        }
    }
}

pub fn policy_to_approval(action: PolicyAction) -> ApprovalLevel {
    match action {
        PolicyAction::Auto => ApprovalLevel::Auto,
        PolicyAction::Confirm => ApprovalLevel::Confirm,
        PolicyAction::Escalate | PolicyAction::RouteLlm => ApprovalLevel::Escalate,
        PolicyAction::Deny => ApprovalLevel::Deny,
    }
}

/// Noul de destrutividade (heurística tipada — D3). Não usa substring solta
/// no caller: centraliza critérios aqui.
pub fn noul_destructive(skill: &str) -> Noul {
    let s = skill.to_ascii_lowercase();
    let mut score: i16 = 0;
    for n in &[
        "shutdown",
        "reboot",
        "format",
        "delete",
        "rm ",
        "wipe",
        "dma",
        "mmio",
        "ring3_register",
        "llm_generate",
    ] {
        if s.contains(n) {
            score += 40;
        }
    }
    for n in &["write", "exec", "net", "disk", "install", "flash"] {
        if s.contains(n) {
            score += 15;
        }
    }
    for n in &["echo", "calc", "read", "list", "status", "help", "diagnostic"] {
        if s.contains(n) {
            score -= 20;
        }
    }
    let p = if score >= 40 {
        0.92
    } else if score >= 15 {
        0.65
    } else if score <= -10 {
        0.12
    } else {
        0.45 // incerto → abster no decide
    };
    Noul::from_p_yes(p, DecisionSource::Heuristic)
}

/// Classificação de skill via Noul + política (substitui `ApprovalGate::classify` morto).
pub fn classify_skill_risk(skill: &str) -> ApprovalLevel {
    // Deny absoluto — confiança NÃO libera
    let s = skill.to_ascii_lowercase();
    if s.contains("raw_mmio") || s.contains("pin_dma_untrusted") {
        return ApprovalLevel::Deny;
    }

    let noul = noul_destructive(skill);
    match noul.decide(160) {
        // Alta P(destrutivo) → Escalate (nunca Auto de execução)
        Some(true) => {
            note_outcome(SitePolicy::SKILL_RISK.site, OutcomeKind::Escalate);
            if noul.p_yes.0 >= 200 {
                ApprovalLevel::Escalate
            } else {
                ApprovalLevel::Confirm
            }
        }
        // Alta P(seguro) → Auto
        Some(false) => {
            note_outcome(SitePolicy::SKILL_RISK.site, OutcomeKind::AutoOk);
            ApprovalLevel::Auto
        }
        // Incerto → Confirm (ação abster = HITL)
        None => {
            note_outcome(SitePolicy::SKILL_RISK.site, OutcomeKind::Escalate);
            ApprovalLevel::Confirm
        }
    }
}

/// Helper genérico: Decision → PolicyAction.
pub fn apply_decision<T: Copy, const N: usize>(
    policy: SitePolicy,
    d: &Decision<T, N>,
    policy_deny: bool,
) -> PolicyAction {
    apply_confidence(policy, d.confidence, d.choice.is_none(), policy_deny)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deny_immune() {
        let a = apply_confidence(
            SitePolicy::SKILL_RISK,
            Confidence(255),
            false,
            true,
        );
        assert_eq!(a, PolicyAction::Deny);
    }

    #[test]
    fn conversational_abstain_routes_llm() {
        let a = apply_confidence(
            SitePolicy::INTENT_THINK,
            Confidence(50),
            true,
            false,
        );
        assert_eq!(a, PolicyAction::RouteLlm);
    }

    #[test]
    fn action_abstain_escalates() {
        let a = apply_confidence(SitePolicy::SKILL_RISK, Confidence(50), true, false);
        assert_eq!(a, PolicyAction::Escalate);
    }

    #[test]
    fn echo_not_destructive() {
        let lvl = classify_skill_risk("echo");
        assert_eq!(lvl, ApprovalLevel::Auto);
    }

    #[test]
    fn format_escalates() {
        let lvl = classify_skill_risk("format_disk");
        assert!(matches!(
            lvl,
            ApprovalLevel::Escalate | ApprovalLevel::Confirm
        ));
    }
}
