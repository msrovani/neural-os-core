//! Self-Optimization — Usage Patterns, Workflow, Scheduling, Config Learning.
//! Bloco 14 — itens #157 a #163 + #135, #136.

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
// #161 Self-Optimizing Scheduler
// ---------------------------------------------------------------------------

const AGENT_PRIORITIES: &[(&str, u8)] = &[
    ("display", 5),   // output sempre responsivo
    ("input", 5),     // entrada do usuário
    ("hermes_console", 4),
    ("intent_router", 4),
    ("cortex_llm", 3),
    ("network_agent", 2),
    ("hw_bridge", 2),
    ("monitor", 1),
    ("security", 4),  // segurança tem prioridade
    ("safety", 5),    // safety é máxima
    ("cron", 2),
    ("mcp", 2),
    ("optimizer", 1),
];

pub fn get_agent_priority(name: &str) -> u8 {
    for (n, p) in AGENT_PRIORITIES {
        if *n == name { return *p; }
    }
    3 // default medium
}

/// #161: Sugere reordenação de agentes baseado no workflow detectado
pub fn suggest_schedule(workflow: &str) -> alloc::string::String {
    match workflow {
        "network" => alloc::format!("📋 Workflow '{}': priorizar NetAgent, NetDriverAgent, MCP", workflow),
        "llm" => alloc::format!("📋 Workflow '{}': priorizar CortexAgent, DisplayAgent", workflow),
        "security" => alloc::format!("📋 Workflow '{}': priorizar SecurityAgent, SafetyAgent", workflow),
        _ => alloc::format!("📋 Workflow '{}': schedule padrão", workflow),
    }
}

// ---------------------------------------------------------------------------
// #163 Hardware Config Learning
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct HardwareConfigSnapshot {
    pub tick: u64,
    pub ring0_mode: u8,
    pub heap_mb: u64,
    pub gpu_present: bool,
    pub net_online: bool,
}

pub struct ConfigLearner {
    snapshots: Vec<HardwareConfigSnapshot>,
}

impl ConfigLearner {
    pub fn new() -> Self {
        ConfigLearner { snapshots: Vec::new() }
    }

    /// Tira um snapshot da configuração atual de hardware
    pub fn snapshot(&mut self) {
        let tick = TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        // Locks individuais (sem aninhamento) para evitar deadlock
        let gpu_present = crate::display::fb::GPU.lock().is_some();
        let net_online = crate::net::NET_CONFIG.lock().online;
        let ring0_mode = crate::SYSTEM_ARCH.lock().as_ref().map_or(0, |a| a.ring0_mode);
        let heap_mb = crate::SYSTEM_ARCH.lock().as_ref().map_or(0, |a| a.heap_size_mb as u64);
        let snap = HardwareConfigSnapshot { tick, ring0_mode, heap_mb, gpu_present, net_online };
        self.snapshots.push(snap);
        if self.snapshots.len() > 20 { self.snapshots.remove(0); }
    }

    /// #135: Sugere mudanças na arquitetura baseado em snapshots
    pub fn suggest_arch_tuning(&self) -> alloc::string::String {
        // Exemplo: se GPU está presente e online, sugere ring1=GPU
        let has_gpu = self.snapshots.last().map_or(false, |s| s.gpu_present);
        let mut suggestions = alloc::string::String::from("🔧 Arch Tuning Suggestions:\n");
        if has_gpu {
            suggestions.push_str("   - GPU detected: set ring1=GPU (GPU compute)\n");
        }
        if !self.has_variance() {
            suggestions.push_str("   - Config estável: manter parâmetros atuais\n");
        }
        suggestions
    }

    fn has_variance(&self) -> bool {
        if self.snapshots.len() < 2 { return false; }
        let first = &self.snapshots[0];
        self.snapshots.iter().skip(1).any(|s| s.ring0_mode != first.ring0_mode)
    }
}

// ---------------------------------------------------------------------------
// #136 LLM decide memory tier (placeholder — integração com CortexAgent)
// ---------------------------------------------------------------------------

pub fn llm_decide_tier(confidence: f32, model_size_kb: u32) -> &'static str {
    if confidence > 0.9 { "Vram" }
    else if model_size_kb > 1024 { "Dram" }
    else { "Dram" }
}

// ---------------------------------------------------------------------------
// Optimizer Agent
// ---------------------------------------------------------------------------

pub struct OptimizerAgent {
    analyzer: UsagePatternAnalyzer,
    config_learner: ConfigLearner,
    tick_counter: u64,
}

impl OptimizerAgent {
    pub fn new() -> Self {
        OptimizerAgent {
            analyzer: UsagePatternAnalyzer::new(),
            config_learner: ConfigLearner::new(),
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

        // MHI Scheduler: promove/demove tiers por padrao de acesso
        crate::fs::mhi_scheduler::mhi_scheduler_tick(self.tick_counter);

        // MegaTrain: prefetch overlap I/O + compute
        crate::mhi::megatrain_tick();

        // #163: Snapshot de config a cada 1000 ticks
        if self.tick_counter % 1000 == 0 {
            self.config_learner.snapshot();
            serial_println!("{}", self.config_learner.suggest_arch_tuning());

            // #161: Sugere schedule baseado no workflow detectado
            if let Some(predicted) = self.analyzer.predict_next_skill() {
                serial_println!("{}", suggest_schedule(predicted));
            }
        }

        AgentTickResult::Pending
    }
}
