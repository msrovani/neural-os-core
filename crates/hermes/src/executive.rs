use alloc::vec::Vec;
use alloc::string::String;
use alloc::collections::BTreeMap;
use crate::affect::{AffectVector, AffectEvent, AffectRegulator};

// ─── LoopPhase: 7-stage Meta-Cognitive Loop ───────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopPhase {
    Observe, Think, Plan, Build, Execute, Verify, Learn,
}

impl LoopPhase {
    pub fn next(&self) -> LoopPhase {
        match self {
            LoopPhase::Observe => LoopPhase::Think,
            LoopPhase::Think => LoopPhase::Plan,
            LoopPhase::Plan => LoopPhase::Build,
            LoopPhase::Build => LoopPhase::Execute,
            LoopPhase::Execute => LoopPhase::Verify,
            LoopPhase::Verify => LoopPhase::Learn,
            LoopPhase::Learn => LoopPhase::Observe,
        }
    }

    pub fn rotation_deg(&self) -> u32 {
        (*self as u32) * 360 / 7
    }
}

// ─── Ego Layer: domain confidence with EMA ────────────────────────────

#[derive(Debug, Clone)]
pub struct DomainConfidence {
    pub confidence: f32,
    pub samples: u64,
    pub last_active: u64,
    pub avg_latency: f32,
    pub success_count: u64,
    pub failure_count: u64,
}

impl DomainConfidence {
    fn new(tick: u64) -> Self {
        DomainConfidence {
            confidence: 0.5,
            samples: 0,
            last_active: tick,
            avg_latency: 0.0,
            success_count: 0,
            failure_count: 0,
        }
    }
}

pub struct EgoLayer {
    pub domains: BTreeMap<String, DomainConfidence>,
    pub interactions: u64,
    ema_alpha: f32,
}

impl EgoLayer {
    pub fn new() -> Self {
        EgoLayer { domains: BTreeMap::new(), interactions: 0, ema_alpha: 0.9 }
    }

    pub fn record(&mut self, domain: &str, success: bool, confidence: f32, latency: f32, tick: u64) {
        self.interactions += 1;
        let entry = self.domains.entry(String::from(domain))
            .or_insert_with(|| DomainConfidence::new(tick));
        entry.last_active = tick;
        entry.samples += 1;
        // EMA confidence update
        let outcome = if success { 1.0 } else { 0.0 };
        entry.confidence = self.ema_alpha * entry.confidence + (1.0 - self.ema_alpha) * outcome;
        entry.avg_latency = self.ema_alpha * entry.avg_latency + (1.0 - self.ema_alpha) * latency;
        if success { entry.success_count += 1; } else { entry.failure_count += 1; }
    }

    pub fn can_answer(&self, domain: &str) -> bool {
        self.domains.get(domain).map_or(false, |d| d.confidence > 0.3)
    }

    pub fn domain_count(&self) -> usize { self.domains.len() }

    pub fn low_confidence_domains(&self, threshold: f32) -> Vec<String> {
        self.domains.iter()
            .filter(|(_, d)| d.samples > 2 && d.confidence < threshold)
            .map(|(name, _)| name.clone())
            .collect()
    }

    pub fn high_confidence_domains(&self, threshold: f32) -> Vec<String> {
        self.domains.iter()
            .filter(|(_, d)| d.confidence > threshold)
            .map(|(name, _)| name.clone())
            .collect()
    }

    pub fn status(&self) -> String {
        alloc::format!("[EGO] {} dominios, {} interacoes", self.domains.len(), self.interactions)
    }
}

// ─── PonderNet: adaptive pondering budget ────────────────────────────

pub struct PonderNet {
    pub halting_prob: f32,
    pub max_ponder_steps: u64,
    pub lambda: f32,
    pub step: u64,
    pub accumulated_budget: u64,
    pub total_ponder_cost: f32,
    threshold: f32,
}

impl PonderNet {
    pub fn new() -> Self {
        PonderNet {
            halting_prob: 0.0,
            max_ponder_steps: 10,
            lambda: 0.01,
            step: 0,
            accumulated_budget: 0,
            total_ponder_cost: 0.0,
            threshold: 0.9,
        }
    }

    /// Reset for a new inference cycle.
    pub fn reset(&mut self) {
        self.halting_prob = 0.0;
        self.step = 0;
        self.accumulated_budget = 0;
    }

    /// Advance one ponder step, return true if halting condition met.
    /// Halting probability increases sigmoidally with step count.
    pub fn ponder_step(&mut self, base_budget: u64) -> bool {
        self.step += 1;
        // Sigmoidal halting: p = 1 / (1 + exp(-k*(step - midpoint)))
        // midpoint = max_ponder_steps / 2, k = 0.8
        let midpoint = (self.max_ponder_steps as f32) * 0.5;
        let k = 0.8;
        self.halting_prob = 1.0 / (1.0 + libm::expf(-k * (self.step as f32 - midpoint)));
        self.accumulated_budget = self.accumulated_budget.saturating_add(base_budget);
        let cost = self.step as f32 * self.lambda;
        self.total_ponder_cost += cost;
        self.halting_prob >= self.threshold || self.step >= self.max_ponder_steps
    }

    /// How many more steps the supervisor recommends.
    pub fn recommended_steps(&self, uncertainty: f32) -> u64 {
        let remaining = self.max_ponder_steps.saturating_sub(self.step);
        if remaining == 0 { return 0; }
        let raw = remaining as f32 * (0.3 + uncertainty * 0.7);
        let steps = libm::ceilf(raw) as u64;
        steps.min(remaining).max(1)
    }
}

// ─── Supervisor Verdict ───────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum SupervisorVerdict {
    Proceed,
    /// Proceed with a specific budget ceiling.
    ProceedWithBudget(u64),
    /// Needs N more pondering steps (PonderNet).
    Ponder(u64),
    /// Delay until a future tick.
    Delay { reason: &'static str, until_tick: u64 },
    Preempt { reason: &'static str },
    Escalate { reason: &'static str },
    /// Trigger BitNetTrainer for a low-confidence domain.
    Train { domain: String, reason: &'static str },
    /// Trigger SkillOpt promotion for an evolving skill.
    PromoteSkill { skill_name: String },
}

// ─── EntropyMonitor ──────────────────────────────────────────────────

pub struct EntropyMonitor {
    pub contradiction_rate: f32,
    pub staleness_index: f32,
    total_contradictions: u64,
    total_ticks: u64,
    recent_contradictions: Vec<u64>,
}

impl EntropyMonitor {
    pub fn new() -> Self {
        EntropyMonitor {
            contradiction_rate: 0.0,
            staleness_index: 0.0,
            total_contradictions: 0,
            total_ticks: 0,
            recent_contradictions: Vec::new(),
        }
    }

    pub fn record_contradiction(&mut self, tick: u64) {
        self.total_contradictions += 1;
        self.recent_contradictions.push(tick);
        self.recent_contradictions.retain(|&t| tick - t < 100);
        self.contradiction_rate = self.recent_contradictions.len() as f32 / 100.0;
    }

    pub fn tick(&mut self) {
        self.total_ticks += 1;
        self.staleness_index = if self.total_ticks > 0 {
            (self.total_ticks - self.total_contradictions) as f32 / self.total_ticks as f32
        } else {
            0.0
        };
    }
}

// ─── DomainBoundary (legacy compat) ──────────────────────────────────


/// Legacy: maintained for backward compat. New code should use EgoLayer directly.
pub struct DomainBoundary {
    pub domain: String,
    pub confidence: f32,
    pub samples: u64,
}

// ─── ExecutiveSupervisor v2 — Meta-Cognitive Supervisor ──────────────

pub struct ExecutiveSupervisor {
    pub phase: LoopPhase,
    pub ego: EgoLayer,
    pub ponder: PonderNet,
    pub entropy_monitor: EntropyMonitor,
    pub affect: AffectRegulator,
    pub collapse_warning: bool,
    pub max_poll_cycles: u64,
    pub grace_cycles: u64,
    // Legacy compat: updated from ego layer on read
    pub confidence_by_domain: BTreeMap<String, f32>,
    pub domain_boundaries: Vec<DomainBoundary>,
    pub inference_budget_dynamic: u64,
    tick: u64,
    phase_counter: u64,
}

impl ExecutiveSupervisor {
    pub fn new() -> Self {
        ExecutiveSupervisor {
            phase: LoopPhase::Observe,
            ego: EgoLayer::new(),
            ponder: PonderNet::new(),
            confidence_by_domain: BTreeMap::new(),
            domain_boundaries: Vec::new(),
            entropy_monitor: EntropyMonitor::new(),
            collapse_warning: false,
            inference_budget_dynamic: 10,
            max_poll_cycles: 5,
            grace_cycles: 2,
            affect: AffectRegulator::new(),
            tick: 0,
            phase_counter: 0,
        }
    }

    // ─── Ego Layer API ─────────────────────────────────────────────

    pub fn record_result(&mut self, domain: &str, success: bool, confidence: f32) {
        let latency = 0.0; // caller can pass latency via extended API
        self.ego.record(domain, success, confidence, latency, self.tick);
        // Sync legacy fields
        if let Some(d) = self.ego.domains.get(domain) {
            self.confidence_by_domain.insert(String::from(domain), d.confidence);
        }
        if let Some(b) = self.domain_boundaries.iter_mut().find(|b| b.domain == domain) {
            if let Some(d) = self.ego.domains.get(domain) {
                b.confidence = d.confidence;
                b.samples = d.samples;
            }
        } else {
            if let Some(d) = self.ego.domains.get(domain) {
                self.domain_boundaries.push(DomainBoundary {
                    domain: String::from(domain),
                    confidence: d.confidence,
                    samples: d.samples,
                });
            }
        }
        let event = if success {
            AffectEvent::Success(confidence)
        } else {
            AffectEvent::Error(1.0 - confidence)
        };
        self.affect.incorporate(event);
    }

    /// Extended record with latency tracking.
    pub fn record_result_full(&mut self, domain: &str, success: bool, confidence: f32, latency: f32) {
        self.ego.record(domain, success, confidence, latency, self.tick);
        if let Some(d) = self.ego.domains.get(domain) {
            self.confidence_by_domain.insert(String::from(domain), d.confidence);
        }
        let event = if success {
            AffectEvent::Success(confidence)
        } else {
            AffectEvent::Error(1.0 - confidence)
        };
        self.affect.incorporate(event);
    }

    // ─── PonderNet API ─────────────────────────────────────────────

    /// Reset PonderNet for a new inference cycle.
    pub fn reset_ponder(&mut self) {
        self.ponder.reset();
    }

    /// Perform one ponder step. Returns true if halting condition met.
    pub fn ponder_step(&mut self, base_budget: u64) -> bool {
        self.ponder.ponder_step(base_budget)
    }

    /// How many more steps PonderNet recommends given current uncertainty.
    pub fn recommended_ponder_steps(&self) -> u64 {
        self.ponder.recommended_steps(self.affect.affect.uncertainty)
    }

    // ─── SkillOpt Integration ──────────────────────────────────────

    /// Check if any evolving skill is ready for WASM promotion.
    /// Returns PromoteSkill verdict if a candidate is found.
    pub fn check_skill_promotion(&self) -> Option<SupervisorVerdict> {
        // Lightweight check: delegate to skill_opt which tracks evolving skills
        let map = crate::skill_opt::EVOLVING.lock();
        for (name, skill) in map.iter() {
            if skill.runs >= 3 && skill.success_rate >= 0.7 {
                return Some(SupervisorVerdict::PromoteSkill { skill_name: name.clone() });
            }
        }
        None
    }

    // ─── BitNetTrainer Integration ─────────────────────────────────

    /// Trigger training for domains below confidence threshold.
    pub fn check_training_needed(&self, threshold: f32) -> Vec<SupervisorVerdict> {
        self.ego.low_confidence_domains(threshold).into_iter()
            .map(|domain| SupervisorVerdict::Train {
                domain,
                reason: "low_confidence",
            })
            .collect()
    }

    // ─── Core Loop ─────────────────────────────────────────────────

    pub fn tick_observe(&mut self, route_budget: u64) -> SupervisorVerdict {
        self.phase = LoopPhase::Think;
        self.tick += 1;
        self.phase_counter += 1;
        self.affect.decay();

        // 1. Entropy / contradiction check
        let verdict = self.contradiction_detect();
        if verdict != SupervisorVerdict::Proceed { return verdict; }
        if self.entropy_monitor.contradiction_rate > 0.2 {
            return SupervisorVerdict::Delay {
                reason: "high contradiction rate",
                until_tick: self.tick + 10,
            };
        }

        // 2. PonderNet: check if pondering is needed
        if self.affect.affect.uncertainty > 0.4 {
            let steps = self.recommended_ponder_steps();
            if steps > 1 {
                return SupervisorVerdict::Ponder(steps);
            }
        }

        // 3. Training check for low-confidence domains
        let low = self.ego.low_confidence_domains(0.3);
        if !low.is_empty() {
            // Trigger training for the lowest confidence domain
            if let Some(domain) = low.first() {
                return SupervisorVerdict::Train {
                    domain: domain.clone(),
                    reason: "low_confidence",
                };
            }
        }

        // 4. Budget allocation
        self.inference_budget(route_budget)
    }

    /// Full tick with phase advance.
    pub fn tick_supervise(&mut self, route_budget: u64) -> SupervisorVerdict {
        let verdict = self.tick_observe(route_budget);
        self.phase = self.phase.next();
        verdict
    }

    // ─── Internal ──────────────────────────────────────────────────

    fn contradiction_detect(&mut self) -> SupervisorVerdict {
        self.entropy_monitor.tick();
        if self.entropy_monitor.contradiction_rate > 0.2 {
            self.collapse_warning = true;
            self.affect.incorporate(AffectEvent::Error(0.5));
            return SupervisorVerdict::Delay {
                reason: "contradiction_rate > 0.2",
                until_tick: self.tick + 5,
            };
        }
        let low_domains: Vec<String> = self.confidence_by_domain.iter()
            .filter(|(_, &c)| c < 0.3)
            .map(|(d, _)| d.clone())
            .collect();
        if !low_domains.is_empty() {
            self.collapse_warning = true;
            return SupervisorVerdict::Escalate {
                reason: "low domain confidence",
            };
        }
        self.collapse_warning = false;
        SupervisorVerdict::Proceed
    }

    fn inference_budget(&mut self, base_budget: u64) -> SupervisorVerdict {
        let affect = &self.affect.affect;
        let mut budget = base_budget;
        if affect.uncertainty > 0.6 { budget = budget.saturating_mul(2); }
        if affect.urgency > 0.7 { budget = budget.saturating_div(2).max(1); }
        if affect.fatigue > 0.8 { budget = budget.saturating_div(2).max(1); }
        if affect.curiosity > 0.7 { budget = budget.saturating_mul(2); }
        self.inference_budget_dynamic = budget;
        if budget > self.max_poll_cycles {
            SupervisorVerdict::ProceedWithBudget(budget)
        } else {
            SupervisorVerdict::Proceed
        }
    }

    // ─── Status ────────────────────────────────────────────────────

    pub fn status(&self) -> String {
        let phase_name = match self.phase {
            LoopPhase::Observe => "Observe",
            LoopPhase::Think => "Think",
            LoopPhase::Plan => "Plan",
            LoopPhase::Build => "Build",
            LoopPhase::Execute => "Execute",
            LoopPhase::Verify => "Verify",
            LoopPhase::Learn => "Learn",
        };
        alloc::format!(
            "[SUPERVISOR] Phase={} Tick={} Domains={} PondStep={} Affect=({:.2},{:.2},{:.2})",
            phase_name, self.tick, self.ego.domain_count(),
            self.ponder.step,
            self.affect.affect.valence,
            self.affect.affect.arousal,
            self.affect.affect.dominance,
        )
    }
}
