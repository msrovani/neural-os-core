//! #244 Human-in-the-Loop Approval.
//! Skills perigosas requerem confirmacao do usuario antes de executar.
//! Gate unico: request_approval() bloqueia, wait_approval() decide.

use alloc::string::String;
use alloc::vec::Vec;
use k_nano::kjson;

#[derive(Clone)]
pub struct ApprovalRequest {
    pub id: u64,
    pub skill: String,
    pub agent: String,
    pub reason: String,
    pub required_level: ApprovalLevel,
    pub resolved: bool,
    pub approved: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ApprovalLevel {
    Auto,      // sempre permitido
    Confirm,   // pede confirmacao
    Escalate,  // requer aprovacao explicita
    Deny,      // sempre negado
}
impl ApprovalLevel {
    pub fn name(&self) -> &'static str {
        match self { ApprovalLevel::Auto => "auto", ApprovalLevel::Confirm => "confirm",
                     ApprovalLevel::Escalate => "escalate", ApprovalLevel::Deny => "deny" }
    }
}

pub struct ApprovalGate {
    requests: Vec<ApprovalRequest>,
    next_id: u64,
}

impl ApprovalGate {
    pub fn new() -> Self { ApprovalGate { requests: Vec::new(), next_id: 1 } }

    /// Submete skill para aprovacao. Retorna id da requisicao.
    /// Se Confirm/Escalate: pede ao Jarbas (ou terminal HANR conforme /ui).
    pub fn request(&mut self, skill: &str, agent: &str, reason: &str, level: ApprovalLevel) -> u64 {
        let id = self.next_id; self.next_id += 1;
        self.requests.push(ApprovalRequest {
            id, skill: String::from(skill), agent: String::from(agent),
            reason: String::from(reason), required_level: level,
            resolved: false, approved: false,
        });
        kjson!("APPROVAL", agent, "request", "id", id, "skill", skill);
        if matches!(level, ApprovalLevel::Confirm | ApprovalLevel::Escalate) {
            crate::hitl_ui::request_user_intervention(
                id,
                skill,
                agent,
                reason,
                level.name(),
            );
        }
        id
    }

    /// Verifica se pode executar sem bloquear.
    pub fn can_execute(&self, skill: &str) -> bool {
        self.requests.iter().rev().find(|r| r.skill == skill && !r.resolved)
            .map_or(true, |r| r.required_level == ApprovalLevel::Auto)
    }

    /// Lista requisicoes pendentes.
    pub fn pending(&self) -> Vec<&ApprovalRequest> {
        self.requests.iter().filter(|r| !r.resolved).collect()
    }

    /// Usuario aprova ou nega.
    pub fn resolve(&mut self, id: u64, approve: bool) -> bool {
        if let Some(r) = self.requests.iter_mut().find(|r| r.id == id && !r.resolved) {
            r.resolved = true; r.approved = approve;
            kjson!("APPROVAL", "GATE", "resolve", "id", id, "approve", approve as u32);
            let tick = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
            let detail = alloc::format!("{}:{}:{}", id, r.skill, if approve { "ok" } else { "deny" });
            crate::globals::AUDIT_TRAIL.lock().push(
                tick,
                "approval",
                if approve { "approve" } else { "deny" },
                detail.as_bytes(),
            );
            true
        } else { false }
    }

    /// Avalia nivel necessario para uma skill.
    pub fn classify(skill: &str) -> ApprovalLevel {
        let s = skill.to_lowercase();
        if s.contains("shutdown") || s.contains("reboot") || s.contains("format") || s.contains("delete")
            || s == "llm_generate"
        {
            ApprovalLevel::Escalate
        } else if s.contains("write") || s.contains("exec") || s.contains("net") || s.contains("disk") {
            ApprovalLevel::Confirm
        } else if s.contains("echo") || s.contains("calc") || s.contains("read") || s.contains("list") {
            ApprovalLevel::Auto
        } else {
            ApprovalLevel::Confirm
        }
    }

    /// ADR-0051: classificação explícita por tipo de pacote / op / assinatura.
    pub fn classify_package(
        kind: crate::package_hub::PackageKind,
        op: crate::package_hub::PackageOpKind,
        signed: bool,
    ) -> ApprovalLevel {
        crate::package_hub::PackageHub::classify(kind, op, signed)
    }

    pub fn status(&self) -> String {
        let p = self.pending().len();
        alloc::format!("[APPROVAL] {} pending, {} total", p, self.requests.len())
    }
}
