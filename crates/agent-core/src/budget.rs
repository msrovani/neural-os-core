//! Agent Budget + Watchdog (IDEA A-014).
//!
//! Contrato AIOS (pós-SESSION_252 + s370):
//! - `ticks_used` = polls neste ciclo do scheduler (stats/HUD). Reset a cada tick.
//! - Overruns **não** vêm de `consume(1)` com budget=100 (1 poll/ciclo nunca estoura —
//!   mentia segurança). Overruns vêm de **wall-clock**: tick > `TICK_WATCHDOG_MS`
//!   via `note_wall_overrun`.
//! - Watchdog: Warning (>0 overrun no ciclo de vida desde recover) → Paused (>3) →
//!   recover após 1000 polls pausados; crash se `lifetime_paused_polls >= 10000`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// Estado do watchdog para um agente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentWatchdogState {
    Normal,
    Warning,    // Teve overrun wall-clock recente
    Paused,     // Excedeu overruns, pausado
    Crashed,    // Lifetime pause esgotado
}

/// Orçamento e watchdog por agente.
pub struct AgentBudget {
    /// Polls neste ciclo do scheduler (reset_all limpa).
    pub ticks_used: u64,
    /// Limite informativo p/ HUD (não dispara pause sozinho).
    pub tick_budget: u64,
    /// Contagem de overruns wall-clock desde o último recover.
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

    /// Marca 1 poll neste ciclo. Retorna `false` se Paused/Crashed (não tickar).
    pub fn allow_poll(&mut self) -> bool {
        match self.watchdog {
            AgentWatchdogState::Paused | AgentWatchdogState::Crashed => false,
            AgentWatchdogState::Normal | AgentWatchdogState::Warning => {
                self.ticks_used = self.ticks_used.saturating_add(1);
                true
            }
        }
    }

    /// Wall-clock overrun (tick > limiar). Acumula → Warning → Paused.
    pub fn note_wall_overrun(&mut self) {
        if self.watchdog == AgentWatchdogState::Crashed {
            return;
        }
        self.overruns = self.overruns.saturating_add(1);
        if self.overruns > 3 {
            self.watchdog = AgentWatchdogState::Paused;
        } else {
            self.watchdog = AgentWatchdogState::Warning;
        }
    }

    /// Compat: contagem de ticks no ciclo (stats). Não dispara pause.
    pub fn consume(&mut self, ticks: u64) -> bool {
        self.ticks_used = self.ticks_used.saturating_add(ticks);
        !matches!(
            self.watchdog,
            AgentWatchdogState::Paused | AgentWatchdogState::Crashed
        )
    }

    /// Reseta contagem do ciclo. Warning → Normal (overruns permanecem até recover).
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
            true
        }
    }

    pub fn allow_poll(&mut self, name: &str) -> bool {
        if let Some(budget) = self.budgets.get_mut(name) {
            budget.allow_poll()
        } else {
            true
        }
    }

    pub fn note_wall_overrun(&mut self, name: &str) {
        if let Some(budget) = self.budgets.get_mut(name) {
            budget.note_wall_overrun();
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

    pub fn get_state(&self, name: &str) -> Option<AgentWatchdogState> {
        self.budgets.get(name).map(|b| b.watchdog)
    }

    /// Recover a paused agent back to Normal and reset overrun counters.
    pub fn recover(&mut self, name: &str) {
        if let Some(budget) = self.budgets.get_mut(name) {
            budget.ticks_used = 0;
            budget.overruns = 0;
            budget.watchdog = AgentWatchdogState::Normal;
        }
    }

    /// Snapshot of all budget states for Hermes monitoring.
    pub fn stats(&self) -> Vec<(String, u64, AgentWatchdogState)> {
        self.budgets
            .values()
            .map(|b| (b.agent_name.clone(), b.ticks_used, b.watchdog))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wall_overrun_pauses_after_four() {
        let mut b = AgentBudget::new("x", 100);
        assert!(b.allow_poll());
        for _ in 0..3 {
            b.note_wall_overrun();
            assert_eq!(b.watchdog, AgentWatchdogState::Warning);
        }
        b.note_wall_overrun();
        assert_eq!(b.watchdog, AgentWatchdogState::Paused);
        assert!(!b.allow_poll());
    }

    #[test]
    fn reset_clears_ticks_not_paused() {
        let mut b = AgentBudget::new("y", 100);
        b.ticks_used = 5;
        b.note_wall_overrun();
        b.reset();
        assert_eq!(b.ticks_used, 0);
        assert_eq!(b.watchdog, AgentWatchdogState::Normal);
        assert_eq!(b.overruns, 1);
    }
}
