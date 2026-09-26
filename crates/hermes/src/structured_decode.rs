//! Structured Decoding — re-export do cortex-crate + SkillOptimizer (hermes).
//!
//! O FSM `StructuredDecoder` e o enum `DecodeMode` foram movidos para
//! cortex-crate (`cortex_crate::structured_decode`) para uso direto no
//! `generate_speculative` durante a geração constrita.
//! `SkillOptimizer` permanece aqui porque depende de `crate::skill_market::SkillMarket`.

// ponytail: structured_decode module disabled in cortex; stubs defined in cortex::cortex
pub use cortex::cortex::{StructuredDecoder, DecodeMode};

use alloc::vec::Vec;
use alloc::string::String;

/// SkillOpt — otimiza skills baseado em metricas de execucao
pub struct SkillOptimizer {
    pub min_calls: u32,
    pub min_success: f32,
    pub optimized: Vec<String>,
}

impl SkillOptimizer {
    pub fn new() -> Self {
        SkillOptimizer { min_calls: 3, min_success: 0.7, optimized: Vec::new() }
    }

    /// Analisa metricas do SkillMarket e sugere otimizacoes
    pub fn analyze(&mut self, market: &crate::skill_market::SkillMarket) -> Vec<String> {
        let mut suggestions = Vec::new();
        for s in market.top(10) {
            if s.calls >= self.min_calls && s.success_rate < self.min_success {
                let suggestion = alloc::format!(
                    "Skill '{}': success_rate={:.1}% ({} calls) — needs review",
                    s.skill, s.success_rate * 100.0, s.calls
                );
                suggestions.push(suggestion);
                if !self.optimized.contains(&s.skill) {
                    self.optimized.push(s.skill.clone());
                }
            }
        }
        k_nano::kjson!("SKILLOPT", "ANALYZE", "done", "suggestions", suggestions.len() as u32);
        suggestions
    }

    /// Gera nova versao de skill com parametros ajustados
    pub fn optimize_skill(&self, _name: &str, old_ticks: u64, success_rate: f32) -> (u64, f32) {
        let new_fuel = if success_rate > 0.9 {
            (old_ticks as f32 * 0.8) as u64
        } else if success_rate < 0.5 {
            (old_ticks as f32 * 1.5) as u64
        } else {
            old_ticks
        };
        let new_rate = success_rate.min(1.0);
        k_nano::kjson!("SKILLOPT", "SKILL", "optimize", "fuel", new_fuel, "rate", new_rate);
        (new_fuel, new_rate)
    }

    pub fn status(&self) -> String {
        alloc::format!("[SKILLOPT] {} optimized, threshold={}% success/{} calls",
            self.optimized.len(), (self.min_success * 100.0) as u8, self.min_calls)
    }
}

/// Lane B: `OutputGrammar::Skill` → Free (sem FSM nova, sem nova gramática).
/// A constrição op-IR acontece via prompt: este helper anexa o schema hint
/// (`wasm_build::op_ir_schema_hint`) pedindo ao modelo SOMENTE expressão/DSL
/// parseável por `model_text_to_ops`. O caller usa com
/// `cortex::cortex::generate_structured(prompt, OutputGrammar::Free)`.
pub fn model_skill_prompt(task: &str) -> String {
    alloc::format!(
        "{}\n\n{} Responda SOMENTE com a expressão (ex: a*b+7) ou DSL (return ...).",
        task,
        crate::wasm_build::op_ir_schema_hint()
    )
}

#[cfg(test)]
mod lane_b_tests {
    use super::model_skill_prompt;

    #[test]
    fn prompt_carries_task_and_schema_hint() {
        let p = model_skill_prompt("some dois números");
        assert!(p.contains("some dois números"));
        assert!(p.contains("I32Const"));
        // O hint canônico é o mesmo do wasm_build (fonte única).
        assert!(p.contains(crate::wasm_build::op_ir_schema_hint()));
    }
}






