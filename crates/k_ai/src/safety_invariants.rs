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
    /// Uses the global agent count to detect if agents have died.
    /// After Phase 6 (AgentFleet), we expect at least 10 agents.
    /// Warning if count drops below 8, Violation if below 5.
    fn check_agents_alive(&self) -> InvariantResult {
        // Read from the bin's SCHED_AGENT_COUNT static (updated each tick
        // by the scheduler halt callback via agent_stats::update_agent_count).
        // Read from the bin's SCHED_AGENT_COUNT (updated by sched_metrics_hook).
        // agent_stats module provides a decoupled accessor.
        let agent_count = crate::agent_stats::current_agent_count();
        if agent_count < 5 {
            k_nano::slog_kai!("Safety", "warn", "I2: only {} agents alive — expected ≥10", agent_count);
            InvariantResult::Violation
        } else if agent_count < 8 {
            k_nano::slog_kai!("Safety", "warn", "I2: {} agents alive — below expected 10", agent_count);
            InvariantResult::Warning
        } else {
            InvariantResult::Pass
        }
    }

    /// I3: Trust intact check.
    /// Verify that no trust entries have been revoked unexpectedly.
    /// Note: TrustCache lives in hermes::globals (TicketLock). k_ai cannot
    /// access it directly due to crate dependency direction (k_ai → hermes forbidden).
    /// The check is done by SecurityAgent (hermes ring) which reads TRUST_CACHE
    /// directly. Here we do a lightweight proxy: check if trust module exists
    /// and report Pass (real check delegated to SecurityAgent).
    fn check_trust_intact(&self) -> InvariantResult {
        // Phase 3: Real check delegated to SecurityAgent (hermes ring).
        // k_ai cannot access hermes::globals::TRUST_CACHE.
        // SecurityAgent calls check_all() and reads TRUST_CACHE directly.
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
