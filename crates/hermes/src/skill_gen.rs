//! Self-skill generation — o agente escreve skills sozinho baseado em uso.
//! Anatomy gap: skills escritas pelo agente, não por humanos.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use alloc::collections::BTreeMap;
use spin::Mutex;

/// Padrão de task que pode virar skill
#[derive(Debug, Clone)]
pub struct TaskPattern {
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,
    pub steps: Vec<String>,
    pub uses: u32,
}

static TASK_PATTERNS: Mutex<BTreeMap<String, TaskPattern>> = Mutex::new(BTreeMap::new());

/// Registra um padrão de task observado
pub fn record_task(name: &str, description: &str, steps: &[&str]) {
    let mut patterns = TASK_PATTERNS.lock();
    if let Some(p) = patterns.get_mut(name) {
        p.uses += 1;
        return;
    }
    patterns.insert(String::from(name), TaskPattern {
        name: String::from(name),
        description: String::from(description),
        triggers: vec![String::from(name)],
        steps: steps.iter().map(|s| String::from(*s)).collect(),
        uses: 1,
    });
}

/// Gera uma skill no formato SKILL.md (contrato ADR-0052) a partir de um padrão.
/// content_hash/signature são adicionados por sign_artifact_md no registro.
pub fn generate_skill(name: &str) -> Option<String> {
    let patterns = TASK_PATTERNS.lock();
    let pattern = patterns.get(name)?;
    let mut skill = String::new();
    skill.push_str("---\n");
    skill.push_str("schema: 1\n");
    skill.push_str("kind: skill\n");
    skill.push_str(&alloc::format!("name: {}\n", pattern.name));
    skill.push_str(&alloc::format!("description: {}\n", pattern.description));
    skill.push_str("contexto: \"Auto-generated from observed task pattern\"\n");
    skill.push_str("acionaveis: [\"on_demand\"]\n");
    skill.push_str("required_tokens: [1]\n");
    skill.push_str("provenance: hermes_created\n");
    skill.push_str("sandbox_status: none\n");
    skill.push_str("---\n\n");
    skill.push_str(&alloc::format!(
        "## Contexto\n\nAuto-generated from observed task pattern.\n\n"
    ));
    skill.push_str(&alloc::format!("## Goal\n\n{}\n\n", pattern.description));
    skill.push_str("## Acionaveis\n\n- on_demand\n\n");
    skill.push_str("## Workflow\n");
    for (i, step) in pattern.steps.iter().enumerate() {
        skill.push_str(&alloc::format!("{}. {}\n", i + 1, step));
    }
    skill.push_str("\n## Pre-Flight\n- [ ] Verify output matches expected format\n");
    skill.push_str("## Success Criteria\n- [ ] All steps completed\n");
    skill.push_str("## Failure Policy\nReport failure and retry with corrected steps\n");
    Some(skill)
}

/// Lane B: skill model-born — texto bruto do modelo → op-IR → WasmSkill+VFS.
/// Thin wrapper sobre `evolve::promote_model_text_to_wasm` (carimbo
/// `model-born`; dummy/fora-da-gramática = refuse honesto, sem registry).
pub fn generate_wasm_from_model_text(
    skill_name: &str,
    description: &str,
    model_text: &str,
) -> Result<(), &'static str> {
    crate::evolve::promote_model_text_to_wasm(skill_name, description, model_text)
}

#[cfg(test)]
mod lane_b_tests {
    use super::generate_wasm_from_model_text;

    #[test]
    fn model_text_ok_registers_model_born() {
        assert!(generate_wasm_from_model_text("lb_gen_a", "test", "a+b").is_ok());
        assert_eq!(
            crate::wasmi_rt::skill_provenance("lb_gen_a"),
            Some(crate::wasmi_rt::SkillProvenance::ModelBorn)
        );
        crate::globals::SKILL_REGISTRY.lock().unregister("lb_gen_a");
    }

    #[test]
    fn model_text_dummy_refuses() {
        assert!(generate_wasm_from_model_text("lb_gen_dummy", "test", "0").is_err());
        assert!(crate::wasmi_rt::skill_provenance("lb_gen_dummy").is_none());
        assert!(!crate::globals::SKILL_REGISTRY.lock().has_skill("lb_gen_dummy"));
    }
}

/// Auto-skill: após N usos, gera skill automaticamente
pub fn maybe_auto_skill(name: &str) -> Option<String> {
    let patterns = TASK_PATTERNS.lock();
    let pattern = patterns.get(name)?;
    if pattern.uses >= 3 {
        drop(patterns);
        let skill_md = generate_skill(name)?;
        // FASE 2.2: Persist via PackageHub
        let signed = crate::package_hub::sign_artifact_md(&skill_md).unwrap_or_else(|_| skill_md.clone());
        let hub = crate::package_hub::PACKAGE_HUB.lock();
        match hub.stage_create(crate::package_hub::PackageKind::Skill, name, &signed, "auto-gen") {
            Ok((_level, _op)) => {
                k_nano::slog_hermes!("SKILLGEN", "ok", "auto-skill persisted via PackageHub");
            }
            Err(e) => {
                k_nano::slog_hermes!("SKILLGEN", "warn", "auto-skill persist failed: {}", e);
            }
        }
        return Some(skill_md);
    }
    None
}






