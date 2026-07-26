//! Self-Optimizing Scheduler + Dynamic Resource Scaling (IDEA #160-#161).
//! Ajusta prioridades do scheduler e tiers de memória baseado no workflow detectado.
//!
//! Conexões: UsagePatternAnalyzer → prioridades → scheduler + MHI.

use alloc::vec::Vec;
use core::sync::atomic::AtomicU64;

/// Prioridade sugerida para um agente baseado no workflow atual.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PriorityHint {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// Sugestão de tier de memória.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TierHint {
    Dram,   // Keep in RAM
    Nvme,   // Can swap to NVMe
    Hdd,    // Cold storage ok
}

/// Dicas de otimização para o scheduler e MHI.
#[derive(Debug, Clone)]
pub struct OptimizationHints {
    pub agent_priorities: Vec<(&'static str, PriorityHint)>,
    pub tier_hints: Vec<(&'static str, TierHint)>,
}

/// Motor de auto-otimização.
pub struct SelfOptimizingScheduler {
    /// Workflow atual (sincronizado do UsagePatternAnalyzer)
    current_workflow: &'static str,
    /// Última hora em que otimizamos
    last_optimization_tick: AtomicU64,
    /// Intervalo entre otimizações (em ticks)
    pub optimization_interval: u64,
}

impl SelfOptimizingScheduler {
    pub fn new() -> Self {
        Self {
            current_workflow: "unknown",
            last_optimization_tick: AtomicU64::new(0),
            optimization_interval: 1000, // ~5s
        }
    }

    /// Atualiza o workflow atual (chamado pelo UsagePatternAnalyzer).
    pub fn set_workflow(&mut self, workflow: &'static str) {
        self.current_workflow = workflow;
    }

    /// Gera dicas de otimização baseadas no workflow atual.
    pub fn generate_hints(&self) -> OptimizationHints {
        let mut hints = OptimizationHints {
            agent_priorities: Vec::new(),
            tier_hints: Vec::new(),
        };

        match self.current_workflow {
            "development" => {
                hints.agent_priorities.push(("cortex_llm", PriorityHint::High));
                hints.agent_priorities.push(("hermes_agent", PriorityHint::High));
                hints.agent_priorities.push(("display", PriorityHint::Normal));
                hints.tier_hints.push(("cortex_weights", TierHint::Dram));
                hints.tier_hints.push(("kv_cache", TierHint::Dram));
            }
            "media" => {
                hints.agent_priorities.push(("display", PriorityHint::Critical));
                hints.agent_priorities.push(("hda_audio", PriorityHint::High));
                hints.agent_priorities.push(("cortex_llm", PriorityHint::Low));
                hints.tier_hints.push(("audio_buffer", TierHint::Dram));
            }
            "communication" => {
                hints.agent_priorities.push(("hermes_agent", PriorityHint::Critical));
                hints.agent_priorities.push(("network_agent", PriorityHint::High));
                hints.agent_priorities.push(("cortex_llm", PriorityHint::High));
                hints.tier_hints.push(("network_buffers", TierHint::Dram));
            }
            _ => {
                // Default: tudo Normal, tiers default
                hints.agent_priorities.push(("*", PriorityHint::Normal));
            }
        }
        hints
    }
}
