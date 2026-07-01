//! Skill Observer — meta-skill que observa execuções, captura padrões e correções,
//! e alimenta o skill_gen com observações para auto-melhoria.
//! Inspirado em "One Skill to Rule Them All" (rebelytics, CC BY 4.0).

use alloc::string::String;
use alloc::vec::Vec;
use alloc::collections::BTreeMap;
use spin::Mutex;
use core::sync::atomic::{AtomicU32, Ordering};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ObsClass { OpenSource, Internal }

#[derive(Debug, Clone)]
pub struct Observation {
    pub number: u32,
    pub tick: u64,
    pub session_context: String,
    pub skill: String,
    pub classification: ObsClass,
    pub phase: String,
    pub issue: String,
    pub suggestion: String,
    pub principle: String,
    pub status: ObsStatus,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ObsStatus { Open, Actioned, Declined }

static OBSERVATIONS: Mutex<Vec<Observation>> = Mutex::new(Vec::new());
static NEXT_NUMBER: AtomicU32 = AtomicU32::new(1);

/// Registra uma observação de execução de tarefa (padrão que pode virar skill)
pub fn watch_task(name: &str, steps: &[&str], tick: u64) {
    let num = NEXT_NUMBER.fetch_add(1, Ordering::Relaxed);
    let mut obs = OBSERVATIONS.lock();
    obs.push(Observation {
        number: num,
        tick,
        session_context: alloc::format!("Task '{}' executed", name),
        skill: alloc::format!("New skill candidate: {}", name),
        classification: ObsClass::OpenSource,
        phase: "execution".into(),
        issue: alloc::format!("Pattern '{}' detected with {} steps", name, steps.len()),
        suggestion: alloc::format!("Create skill '{}' with {} steps", name, steps.len()),
        principle: "Repeated patterns should become reusable skills".into(),
        status: ObsStatus::Open,
    });
}

/// Registra uma observação de correção (melhoria em skill existente)
pub fn watch_correction(skill: &str, issue: &str, suggestion: &str, principle: &str, tick: u64) {
    let num = NEXT_NUMBER.fetch_add(1, Ordering::Relaxed);
    let mut obs = OBSERVATIONS.lock();
    obs.push(Observation {
        number: num,
        tick,
        session_context: alloc::format!("Correction during '{}'", skill),
        skill: String::from(skill),
        classification: ObsClass::OpenSource,
        phase: "correction".into(),
        issue: String::from(issue),
        suggestion: String::from(suggestion),
        principle: String::from(principle),
        status: ObsStatus::Open,
    });
}

/// Retorna observações abertas (não processadas)
pub fn pending_observations() -> Vec<Observation> {
    OBSERVATIONS.lock().iter()
        .filter(|o| o.status == ObsStatus::Open)
        .cloned()
        .collect()
}

/// Conta observações por skill
pub fn count_by_skill() -> BTreeMap<String, u32> {
    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    let obs = OBSERVATIONS.lock();
    for o in obs.iter() {
        *counts.entry(o.skill.clone()).or_insert(0) += 1;
    }
    counts
}

/// Gera relatório markdown das observações pendentes
pub fn report() -> String {
    let pending = pending_observations();
    if pending.is_empty() {
        return String::from("No open observations.\n");
    }
    let mut out = String::new();
    out.push_str(&alloc::format!("# Observation Report — {} open\n\n", pending.len()));
    for obs in &pending {
        out.push_str(&alloc::format!(
            "### Observation {}: {}\n\
             **Skill:** {}\n\
             **Phase:** {} | **Class:** {:?}\n\
             **Issue:** {}\n\
             **Suggestion:** {}\n\
             **Principle:** {}\n\n",
            obs.number, obs.phase, obs.skill, obs.phase, obs.classification,
            obs.issue, obs.suggestion, obs.principle,
        ));
    }
    out
}

/// Marca observação como actionada
pub fn mark_actioned(number: u32) -> bool {
    let mut obs = OBSERVATIONS.lock();
    if let Some(o) = obs.iter_mut().find(|o| o.number == number) {
        o.status = ObsStatus::Actioned;
        true
    } else { false }
}

/// Marca observação como declineada
pub fn mark_declined(number: u32) -> bool {
    let mut obs = OBSERVATIONS.lock();
    if let Some(o) = obs.iter_mut().find(|o| o.number == number) {
        o.status = ObsStatus::Declined;
        true
    } else { false }
}

/// Gera SKILL.md a partir de uma observação de nova skill
pub fn generate_skill_md(name: &str, steps: &[&str]) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&alloc::format!("name: {}\n", name));
    out.push_str("description: Auto-generated from observation\n");
    out.push_str("required_tokens: [1]\n");
    out.push_str("---\n\n");
    out.push_str(&alloc::format!("# {} Skill\n\n## Workflow\n", name));
    for (i, step) in steps.iter().enumerate() {
        out.push_str(&alloc::format!("{}. {}\n", i + 1, step));
    }
    out.push_str("\n## Pre-Flight Verification\n");
    out.push_str("- [ ] Verify output matches expected format\n");
    out.push_str("- [ ] Check for edge cases\n");
    out.push_str("- [ ] Confirm all steps completed\n");
    out
}
