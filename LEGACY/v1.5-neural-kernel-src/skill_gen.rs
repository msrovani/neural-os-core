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

/// Gera uma skill no formato SKILL.md a partir de um padrão
pub fn generate_skill(name: &str) -> Option<String> {
    let patterns = TASK_PATTERNS.lock();
    let pattern = patterns.get(name)?;
    let mut skill = String::new();
    skill.push_str("---\n");
    skill.push_str(&alloc::format!("name: {}\n", pattern.name));
    skill.push_str(&alloc::format!("description: {}\n", pattern.description));
    skill.push_str("required_tokens: [1]\n");
    skill.push_str("---\n\n");
    skill.push_str(&alloc::format!("Steps for '{}':\n", pattern.name));
    for (i, step) in pattern.steps.iter().enumerate() {
        skill.push_str(&alloc::format!("{}. {}\n", i + 1, step));
    }
    skill.push_str(&alloc::format!("\nTrigger phrases:\n"));
    for trigger in &pattern.triggers {
        skill.push_str(&alloc::format!("- \"{}\"\n", trigger));
    }
    Some(skill)
}

/// Auto-skill: após N usos, gera skill automaticamente
pub fn maybe_auto_skill(name: &str) -> Option<String> {
    let patterns = TASK_PATTERNS.lock();
    let pattern = patterns.get(name)?;
    if pattern.uses >= 3 { // após 3 usos, vira skill
        drop(patterns);
        return generate_skill(name);
    }
    None
}
