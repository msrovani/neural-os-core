//! SkillOpt — Sprint 106-9/10
//! Escalonamento Evolutivo de Código (JIT Cognitivo):
//! Python efêmero → WASM persistente → Rust no_std via Cortex LLM.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::collections::BTreeMap;
use spin::Mutex;

use crate::structured_decode::SkillOptimizer;
use crate::wasmi_rt;

/// Estágio de evolução de um skill gerado on-demand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillStage {
    EphemeralPython,
    WasmPersistent,
    RustNoStd,
}

#[derive(Debug, Clone)]
pub struct EvolvingSkill {
    pub name: String,
    pub stage: SkillStage,
    pub source: String,
    pub runs: u32,
    pub success_rate: f32,
}

pub static EVOLVING: Mutex<BTreeMap<String, EvolvingSkill>> = Mutex::new(BTreeMap::new());

/// Serializa os testes que tocam `EVOLVING` (mapa global) — sem isto, dois
/// testes em paralelo resetam/gravam no mesmo mapa e corrompem as asserções
/// (ex.: um skill elegível de outro teste aparece no `check_skill_promotion`).
#[cfg(test)]
pub static EVOLVING_TEST_LOCK: Mutex<()> = Mutex::new(());

/// Registra execução Python efêmera (primeiro uso).
pub fn record_python_run(name: &str, source: &str, success: bool) {
    let mut map = EVOLVING.lock();
    let entry = map.entry(String::from(name)).or_insert(EvolvingSkill {
        name: String::from(name),
        stage: SkillStage::EphemeralPython,
        source: String::from(source),
        runs: 0,
        success_rate: 0.0,
    });
    entry.runs += 1;
    let prev = entry.success_rate;
    entry.success_rate = if entry.runs == 1 {
        if success { 1.0 } else { 0.0 }
    } else {
        (prev * (entry.runs as f32 - 1.0) + if success { 1.0 } else { 0.0 }) / entry.runs as f32
    };
    k_nano::slog_hermes!("SkillOpt", "info", "'{}' python run #{} success_rate={:.0}%",
        name,
        entry.runs,
        entry.success_rate * 100.0);
}

/// Marca estágio WASM após N execuções bem-sucedidas (≥3, taxa ≥70%).
/// Retorna path lógico; o caller deve chamar `evolve::promote_ephemeral_to_wasm`
/// para materializar bytecode no runtime (cola agentica PnP / S108).
pub fn maybe_promote_to_wasm(name: &str) -> Option<String> {
    let mut map = EVOLVING.lock();
    let entry = map.get_mut(name)?;
    if entry.stage != SkillStage::EphemeralPython {
        return None;
    }
    if entry.runs < 3 || entry.success_rate < 0.7 {
        return None;
    }
    entry.stage = SkillStage::WasmPersistent;
    let wasm_path = alloc::format!("/skills/{}.wasm", name);
    k_nano::slog_hermes!("SkillOpt", "info", "'{}' elegível WASM (runs={}, rate={:.0}%) → {}",
        name,
        entry.runs,
        entry.success_rate * 100.0,
        wasm_path);
    Some(wasm_path)
}

/// Gera prompt para Cortex traduzir Python → Rust no_std (Sprint 106-10).
pub fn rust_no_std_prompt(python_source: &str, skill_name: &str) -> String {
    alloc::format!(
        "Translate this Python skill to Rust no_std for Neural OS Hermes.\n\
         Rules: #![no_std], use alloc only, no std::, no OS calls.\n\
         Skill name: {skill_name}\n\
         Expose fn execute(payload: &[u8]) -> Result<Vec<u8>, &'static str>\n\n\
         ```python\n{python_source}\n```\n\
         Output only valid Rust code."
    )
}

/// Marca skill como promovido a Rust no_std após compilação bem-sucedida.
pub fn mark_rust_promoted(name: &str, rust_source: &str) {
    let mut map = EVOLVING.lock();
    if let Some(entry) = map.get_mut(name) {
        entry.stage = SkillStage::RustNoStd;
        entry.source = String::from(rust_source);
        k_nano::slog_hermes!("SkillOpt", "info", "'{}' → Rust no_std cravado em pedra", name);
    }
}

/// Promove um skill efêmero para WASM persistente no wasmi runtime.
/// Gera bytecode dummy, valida no wasmi, registra no SkillRegistry (ADR-0059 F5).
/// ponytail: bytecode dummy (i32.const 42; end) — o gerador real é Cortex/LLM.
pub fn promote_skill_to_wasm(name: &str, source: &str) -> Result<(), &'static str> {
    let wasm = wasmi_rt::generate_wasm_module();
    // Valida e testa no wasmi
    wasmi_rt::run_wasm(&wasm, "_start", &[], 0).map_err(|_| "promote: sandbox fail")?;
    // Registra como DynamicSkill persistente
    let skill = crate::dynskill::DynamicSkill::with_wasm(name, source, "", wasm);
    crate::globals::SKILL_REGISTRY.lock().register(Box::new(skill));
    k_nano::slog_hermes!("SkillOpt", "info", "'{}' promoted to WASM (wasmi)", name);
    // ADR-0063: índice skill no SGDB (meta; bytecode WASM residual)
    let _ = k_ai::sgdb::put_skill_blob(name, source);
    Ok(())
}

/// Pipeline completo: analisa mercado WASM + skills evolutivos pendentes.
pub fn run_evolution_pass(market: &crate::skill_market::SkillMarket) -> Vec<String> {
    let mut opt = SkillOptimizer::new();
    let mut actions = opt.analyze(market);

    let map = EVOLVING.lock();
    for (name, skill) in map.iter() {
        if skill.stage == SkillStage::EphemeralPython && skill.runs >= 3 && skill.success_rate >= 0.7 {
            actions.push(alloc::format!(
                "Promote '{}' to WASM (runs={}, rate={:.0}%)",
                name, skill.runs, skill.success_rate * 100.0
            ));
        }
    }
    actions
}

/// Status resumido do pipeline JIT Cognitivo.
pub fn status() -> String {
    let map = EVOLVING.lock();
    let (py, wasm, rust) = map.values().fold((0u32, 0u32, 0u32), |acc, s| {
        match s.stage {
            SkillStage::EphemeralPython => (acc.0 + 1, acc.1, acc.2),
            SkillStage::WasmPersistent => (acc.0, acc.1 + 1, acc.2),
            SkillStage::RustNoStd => (acc.0, acc.1, acc.2 + 1),
        }
    });
    alloc::format!("[SkillOpt] evolving: {} python, {} wasm, {} rust", py, wasm, rust)
}






