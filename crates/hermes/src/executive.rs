use alloc::vec::Vec;
use alloc::string::String;
use alloc::collections::BTreeMap;
use crate::affect::{AffectVector, AffectEvent, AffectRegulator};

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

#[derive(Debug, Clone, PartialEq)]
pub enum SupervisorVerdict {
    Proceed,
    ProceedWithBudget(u64),
    Delay { reason: &'static str, until_tick: u64 },
    Preempt { reason: &'static str },
    Escalate { reason: &'static str },
}

pub struct DomainBoundary {
    pub domain: String,
    pub confidence: f32,
    pub samples: u64,
}

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

pub struct ExecutiveSupervisor {
    pub phase: LoopPhase,
    pub confidence_by_domain: BTreeMap<String, f32>,
    pub domain_boundaries: Vec<DomainBoundary>,
    pub entropy_monitor: EntropyMonitor,
    pub collapse_warning: bool,
    pub inference_budget_dynamic: u64,
    pub stop_threshold: f32,
    pub max_poll_cycles: u64,
    pub grace_cycles: u64,
    pub affect: AffectRegulator,
    tick: u64,
    phase_counter: u64,
}

impl ExecutiveSupervisor {
    pub fn new() -> Self {
        ExecutiveSupervisor {
            phase: LoopPhase::Observe,
            confidence_by_domain: BTreeMap::new(),
            domain_boundaries: Vec::new(),
            entropy_monitor: EntropyMonitor::new(),
            collapse_warning: false,
            inference_budget_dynamic: 10,
            stop_threshold: 0.8,
            max_poll_cycles: 5,
            grace_cycles: 2,
            affect: AffectRegulator::new(),
            tick: 0,
            phase_counter: 0,
        }
    }

    pub fn tick_observe(&mut self, route_budget: u64) -> SupervisorVerdict {
        self.phase = LoopPhase::Think;
        self.tick += 1;
        self.phase_counter += 1;
        self.affect.decay();

        let verdict = self.contradiction_detect();
        if verdict != SupervisorVerdict::Proceed { return verdict; }
        if self.entropy_monitor.contradiction_rate > 0.2 {
            return SupervisorVerdict::Delay {
                reason: "high contradiction rate",
                until_tick: self.tick + 10,
            };
        }
        self.inference_budget(route_budget)
    }

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

    pub fn record_result(&mut self, domain: &str, success: bool, confidence: f32) {
        let entry = self.confidence_by_domain.entry(domain.into()).or_insert(0.5);
        let delta = if success { 0.1 } else { -0.1 };
        *entry = (*entry + delta).clamp(0.0, 1.0);

        let event = if success {
            AffectEvent::Success(confidence)
        } else {
            AffectEvent::Error(1.0 - confidence)
        };
        self.affect.incorporate(event);

        if let Some(b) = self.domain_boundaries.iter_mut().find(|b| b.domain == domain) {
            b.confidence = *entry;
            b.samples += 1;
        } else {
            self.domain_boundaries.push(DomainBoundary {
                domain: domain.into(),
                confidence: *entry,
                samples: 1,
            });
        }
    }

    pub fn tick_supervise(&mut self, route_budget: u64) -> SupervisorVerdict {
        let verdict = self.tick_observe(route_budget);
        self.phase = self.phase.next();
        verdict
    }
}
