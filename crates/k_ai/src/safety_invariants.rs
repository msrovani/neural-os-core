//! Fail-Closed Safety Invariants (IDEA #315.18).
//! 4 invariantes checados a cada tick pelo SecurityAgent.
//! Se qualquer invariante falha, o sistema entra em modo fail-closed (shutdown ordenado).
//!
//! I1: Heap integrity — allocator não corrompido (smoke alloc; não é full heap audit)
//! I2: Agents alive — agents esperados estão rodando
//! I3: Trust intact — **proxy observe-only em k_ai** (TRUST_CACHE vive em hermes;
//!     Warning aqui ≠ "íntegro"; hermes SafetyAgent sobrescreve com entry_count)
//! I4: Scheduler tick — detecta salto grande entre checks (não "tick parado":
//!     se o tick congela, este checker também para de rodar)

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

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
        // Warning = degraded observe (AIOS honesty), não fail-closed.
        // Só Violation dispara fail-closed / contador.
        self.i1_heap != InvariantResult::Violation
            && self.i2_agents != InvariantResult::Violation
            && self.i3_trust != InvariantResult::Violation
            && self.i4_scheduler != InvariantResult::Violation
    }

    /// True se todos os invariantes estão Pass (sem Warning).
    pub fn all_green(&self) -> bool {
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
        // Read from the bin's SCHED_AGENT_COUNT (updated by sched_metrics_hook).
        let agent_count = crate::agent_stats::current_agent_count();
        // 0 = snapshot ainda não wired (pré-fleet) — Warning, não Violation fail-closed.
        if agent_count == 0 {
            k_nano::slog_kai!(
                "Safety",
                "warn",
                "I2: agent_count=0 (scheduler snapshot not yet updated)"
            );
            InvariantResult::Warning
        } else if agent_count < 5 {
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
    /// Honesty: k_ai não pode ler hermes::TRUST_CACHE (dep direction).
    /// Warning = "não verificado neste anel", NÃO Pass (= íntegro).
    /// Hermes SafetyAgent faz o check real (entry_count).
    fn check_trust_intact(&self) -> InvariantResult {
        static WARNED: AtomicBool = AtomicBool::new(false);
        if !WARNED.swap(true, Ordering::Relaxed) {
            k_nano::slog_kai!(
                "Safety",
                "warn",
                "I3 proxy: TrustCache delegated to hermes — returning Warning (not Pass)"
            );
        }
        InvariantResult::Warning
    }

    /// I4: Scheduler tick check.
    /// Detecta salto grande entre chamadas (lag/starvation do checker).
    /// NÃO detecta freeze absoluto: se TIMER_TICKS para, este código também para.
    fn check_scheduler_tick(&self, tick: u64) -> InvariantResult {
        let last = self.last_tick.load(Ordering::Relaxed);
        if last != 0 {
            let delta = tick.wrapping_sub(last);
            if delta > 1000 {
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
