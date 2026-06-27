//! Self-Optimization — Usage Patterns, Workflow Prediction, Dynamic Scaling.
//! Bloco 14 — itens #157 a #163.

use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;
use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use crate::interrupts::TIMER_TICKS;
use crate::{serial_println, println};
use crate::EVENT_BUS;

const OPT_MANIFEST: AgentManifest = AgentManifest {
    name: "optimizer",
    kind: AgentKind::System,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

// ---------------------------------------------------------------------------
// #157 Usage Pattern Analyzer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct UsageRecord {
    tick: u64,
    intent: String,
    skill: String,
    duration_ticks: u64,
}

pub struct UsagePatternAnalyzer {
    history: VecDeque<UsageRecord>,
    last_tick: u64,
    current_intent: Option<String>,
    current_skill: Option<String>,
}

impl UsagePatternAnalyzer {
    pub fn new() -> Self {
        UsagePatternAnalyzer {
            history: VecDeque::new(),
            last_tick: 0,
            current_intent: None,
            current_skill: None,
        }
    }

    pub fn record_intent(&mut self, intent: &str, skill: &str) {
        let tick = TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        if let Some(ref prev_intent) = self.current_intent {
            let duration = tick - self.last_tick;
            self.history.push_back(UsageRecord {
                tick: self.last_tick,
                intent: prev_intent.clone(),
                skill: self.current_skill.clone().unwrap_or_default(),
                duration_ticks: duration,
            });
            if self.history.len() > 100 { self.history.pop_front(); }
        }
        self.current_intent = Some(String::from(intent));
        self.current_skill = Some(String::from(skill));
        self.last_tick = tick;
    }

    /// #158: Retorna a skill mais frequente nas últimas N amostras
    pub fn predict_next_skill(&self) -> Option<&str> {
        if self.history.is_empty() { return None; }
        let mut counts: Vec<(&str, usize)> = Vec::new();
        for record in &self.history {
            if let Some(pos) = counts.iter().position(|(s, _)| *s == record.skill.as_str()) {
                counts[pos].1 += 1;
            } else {
                counts.push((record.skill.as_str(), 1));
            }
        }
        counts.into_iter().max_by_key(|(_, c)| *c).map(|(s, _)| s)
    }

    /// #157: Relatório de padrões de uso
    pub fn report(&self) -> String {
        let mut msg = String::from("📊 Usage Pattern Report\n");
        msg.push_str(&alloc::format!("   Total records: {}\n", self.history.len()));
        if let Some(next) = self.predict_next_skill() {
            msg.push_str(&alloc::format!("   Most frequent: {}\n", next));
        }
        msg
    }
}

// ---------------------------------------------------------------------------
// #160 Dynamic Resource Scaling (MHI auto-ajuste)
// ---------------------------------------------------------------------------

fn auto_scale_memory() {
    let mem = crate::memory::global_hardware_context();
    let occupancy = mem[0];
    if occupancy > 0.85 {
        serial_println!("[OPTIMIZER] ⚠️ Memória acima de 85% ({:.0}%). Compactando...", occupancy * 100.0);
        // Sugestão: compactar heap, liberar caches
    } else if occupancy < 0.30 {
        serial_println!("[OPTIMIZER] ✅ Memória folgada ({:.0}%). Pode expandir cache.", occupancy * 100.0);
    }
}

// ---------------------------------------------------------------------------
// #139 Reflex Threshold Tuning
// ---------------------------------------------------------------------------

pub fn should_bypass_llm(confidence: f32) -> bool {
    confidence > 0.9
}

// ---------------------------------------------------------------------------
// Optimizer Agent
// ---------------------------------------------------------------------------

pub struct OptimizerAgent {
    analyzer: UsagePatternAnalyzer,
    tick_counter: u64,
}

impl OptimizerAgent {
    pub fn new() -> Self {
        OptimizerAgent {
            analyzer: UsagePatternAnalyzer::new(),
            tick_counter: 0,
        }
    }

    pub fn record(&mut self, intent: &str, skill: &str) {
        self.analyzer.record_intent(intent, skill);
    }
}

impl Agent for OptimizerAgent {
    fn manifest(&self) -> &AgentManifest { &OPT_MANIFEST }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        self.tick_counter += 1;

        // #160: Auto-scaling a cada 200 ticks
        if self.tick_counter % 200 == 0 {
            auto_scale_memory();
        }

        // #157: Relatório a cada 500 ticks
        if self.tick_counter % 500 == 0 {
            serial_println!("{}", self.analyzer.report());
        }

        AgentTickResult::Pending
    }
}
