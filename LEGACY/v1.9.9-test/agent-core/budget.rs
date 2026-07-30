//! Agent Budget + Watchdog (IDEA A-014).
//! Cada agente tem um tick_budget por ciclo. Se excede, watchdog pausa.
//! Previne runaway agents.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// Estado do watchdog para um agente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentWatchdogState {
    Normal,
    Warning,    // Perto do limite
    Paused,     // Excedeu budget, pausado
    Crashed,    // Excedeu + não respondeu
}

/// Orçamento e watchdog por agente.
pub struct AgentBudget {
    /// ticks gastos no ciclo atual
    pub ticks_used: u64,
    /// ticks máximos permitidos por ciclo
    pub tick_budget: u64,
    /// Total de ciclos que excedeu budget
    pub overruns: u64,
    /// Estado do watchdog
    pub watchdog: AgentWatchdogState,
    /// Nome do agente
    pub agent_name: String,
}

impl AgentBudget {
    pub fn new(name: &str, budget: u64) -> Self {
        Self {
            ticks_used: 0,
            tick_budget: budget,
            overruns: 0,
            watchdog: AgentWatchdogState::Normal,
            agent_name: String::from(name),
        }
    }

    /// Registra ticks consumidos. Retorna false se excedeu budget.
    pub fn consume(&mut self, ticks: u64) -> bool {
        self.ticks_used += ticks;
        if self.ticks_used > self.tick_budget {
            self.overruns += 1;
            if self.overruns > 3 {
                self.watchdog = AgentWatchdogState::Paused;
            } else {
                self.watchdog = AgentWatchdogState::Warning;
            }
            false
        } else {
            true
        }
    }

    /// Reseta contagem para novo ciclo.
    pub fn reset(&mut self) {
        self.ticks_used = 0;
        if self.watchdog == AgentWatchdogState::Warning {
            self.watchdog = AgentWatchdogState::Normal;
        }
    }
}

/// Gerenciador central de budgets.
pub struct BudgetManager {
    budgets: BTreeMap<String, AgentBudget>,
    default_budget: u64,
}

impl BudgetManager {
    pub fn new() -> Self {
        Self {
            budgets: BTreeMap::new(),
            default_budget: 100,
        }
    }

    pub fn register(&mut self, name: &str, budget: Option<u64>) {
        self.budgets.insert(
            String::from(name),
            AgentBudget::new(name, budget.unwrap_or(self.default_budget)),
        );
    }

    pub fn consume(&mut self, name: &str, ticks: u64) -> bool {
        if let Some(budget) = self.budgets.get_mut(name) {
            budget.consume(ticks)
        } else {
            true // unknown agent, no budget
        }
    }

    pub fn reset_all(&mut self) {
        for budget in self.budgets.values_mut() {
            budget.reset();
        }
    }

    pub fn status(&self) -> Vec<&AgentBudget> {
        self.budgets.values().collect()
    }
}
