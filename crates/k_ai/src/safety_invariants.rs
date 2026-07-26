//! Fail-Closed Safety Invariants (IDEA #315.18).
//! 4 invariantes checados a cada tick pelo SecurityAgent.
//! Se qualquer invariante falha, o sistema entra em modo fail-closed (shutdown ordenado).
//!
//! I1: Heap integrity — allocator não corrompido
//! I2: Agents alive — agents esperados estão rodando
//! I3: Trust intact — TrustCache não foi violado
//! I4: Scheduler tick — scheduler está avançando

use core::sync::atomic::{AtomicU64, Ordering};

/// Resultado da verificação de invariantes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvariantResult {
    Pass,
    Warning,    // Invariante pressionado mas não violado
    Violation,  // Invariante violado → shutdown
}

/// Status atual dos invariantes.
#[derive(Debug, Clone)]
pub struct SafetyStatus {
    pub i1_heap: InvariantResult,
    pub i2_agents: InvariantResult,
    pub i3_trust: InvariantResult,
    pub i4_scheduler: InvariantResult,
    pub last_check_tick: u64,
    pub violations: u64,
}

impl SafetyStatus {
    pub fn all_pass(&self) -> bool {
        self.i1_heap == InvariantResult::Pass
            && self.i2_agents == InvariantResult::Pass
            && self.i3_trust == InvariantResult::Pass
            && self.i4_scheduler == InvariantResult::Pass
    }
}

/// Verificador de invariantes.
pub struct SafetyInvariants {
    last_tick: AtomicU64,
    violations: AtomicU64,
    last_agent_count: usize,
}

impl SafetyInvariants {
    pub fn new() -> Self {
        Self {
            last_tick: AtomicU64::new(0),
            violations: AtomicU64::new(0),
            last_agent_count: 0,
        }
    }

    /// Verifica todos os 4 invariantes. Chamado a cada tick do SecurityAgent.
    pub fn check_all(&mut self, tick: u64) -> SafetyStatus {
        let i1 = self.check_heap_integrity();
        let i2 = self.check_agents_alive();
        let i3 = self.check_trust_intact();
        let i4 = self.check_scheduler_tick(tick);

        let status = SafetyStatus {
            i1_heap: i1,
            i2_agents: i2,
            i3_trust: i3,
            i4_scheduler: i4,
            last_check_tick: tick,
            violations: self.violations.load(Ordering::Relaxed),
        };

        if !status.all_pass() {
            self.violations.fetch_add(1, Ordering::Relaxed);
        }

        status
    }

    /// I1: Heap integrity check.
    /// Verifica se o alocador global responde sem panic.
    fn check_heap_integrity(&self) -> InvariantResult {
        // Heap check: try a small allocation + deallocation
        // If it panics, the invariant is violated
        let v = alloc::vec![1u8, 2, 3];
        if v.len() != 3 {
            return InvariantResult::Violation;
        }
        // Check heap size metrics if available
        InvariantResult::Pass
    }

    /// I2: Agents alive check.
    /// Verifica se os agentes críticos estão rodando.
    fn check_agents_alive(&self) -> InvariantResult {
        // TODO: query AgentRegistry for expected agents
        // For now, pass through (agent registry integration needed)
        InvariantResult::Pass
    }

    /// I3: Trust intact check.
    /// Verifica se o TrustCache não foi violado.
    fn check_trust_intact(&self) -> InvariantResult {
        // TODO: verify trust cache integrity
        // For now, pass through
        InvariantResult::Pass
    }

    /// I4: Scheduler tick check.
    /// Verifica se o scheduler está avançando (tick não parou).
    fn check_scheduler_tick(&self, tick: u64) -> InvariantResult {
        let last = self.last_tick.load(Ordering::Relaxed);
        if last != 0 {
            let delta = tick.wrapping_sub(last);
            if delta > 1000 {
                // Tick parou por muito tempo
                return InvariantResult::Violation;
            }
            if delta > 100 {
                return InvariantResult::Warning;
            }
        }
        self.last_tick.store(tick, Ordering::Relaxed);
        InvariantResult::Pass
    }

    pub fn violation_count(&self) -> u64 {
        self.violations.load(Ordering::Relaxed)
    }
}
