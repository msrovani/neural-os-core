//! Cognitive Engine — Sprint 95. #105-108, #149-175, M37-M41.
//! Intent Planner, Success Engine, Neural Cache, Codebook VQ, Feedback Loop.

use alloc::vec::Vec;
use alloc::vec;
use alloc::string::String;
use alloc::collections::BTreeMap;

// ─── #105 Intent Planner ──────────────────────────────────────────────────

pub struct IntentPlanner { pub plans: Vec<String> }
impl IntentPlanner {
    pub fn new() -> Self { IntentPlanner { plans: Vec::new() } }
    pub fn plan(&mut self, goal: &str) -> Vec<&str> {
        self.plans.push(String::from(goal));
        vec!["analyze", "execute", "verify"] // steps
    }
    pub fn status(&self) -> String { alloc::format!("[PLANNER] {} plans", self.plans.len()) }
}

// ─── #106 Success Engine ──────────────────────────────────────────────────

pub struct SuccessEngine { pub success_rate: f32, pub total: u64, pub good: u64 }
impl SuccessEngine {
    pub fn new() -> Self { SuccessEngine { success_rate: 0.0, total: 0, good: 0 } }
    pub fn feedback(&mut self, ok: bool) { self.total += 1; if ok { self.good += 1; } self.success_rate = self.good as f32 / self.total.max(1) as f32; }
    pub fn status(&self) -> String { alloc::format!("[SUCCESS] {:.0}% ({}/{})", self.success_rate*100.0, self.good, self.total) }
}

// ─── #107 Neural Cache ────────────────────────────────────────────────────

pub struct NeuralCache { cache: BTreeMap<u64, Vec<f32>>, hits: u64, misses: u64 }
impl NeuralCache {
    pub fn new() -> Self { NeuralCache { cache: BTreeMap::new(), hits: 0, misses: 0 } }
    pub fn get(&mut self, key: u64) -> Option<&Vec<f32>> { let r = self.cache.get(&key); if r.is_some() { self.hits += 1 } else { self.misses += 1 }; r }
    pub fn set(&mut self, key: u64, val: Vec<f32>) { self.cache.insert(key, val); }
    pub fn status(&self) -> String { alloc::format!("[NCACHE] {} entries, {:.0}% hit", self.cache.len(), if self.hits+self.misses>0{(self.hits as f32/(self.hits+self.misses)as f32*100.0)}else{0.0}) }
}

// ─── #108 MatMul-free LM stub ─────────────────────────────────────────────

pub struct MatMulFreeLM { pub loaded: bool }
impl MatMulFreeLM { pub fn new() -> Self { MatMulFreeLM { loaded: false } } }

// ─── #149-152 Feedback + Ternary Update ───────────────────────────────────

pub struct FeedbackLoop { pub ratings: Vec<u8> }
impl FeedbackLoop {
    pub fn new() -> Self { FeedbackLoop { ratings: Vec::new() } }
    pub fn rate(&mut self, score: u8) { self.ratings.push(score.min(10)); }
    pub fn avg(&self) -> f32 { let s: u32 = self.ratings.iter().map(|&r| r as u32).sum(); s as f32 / self.ratings.len().max(1) as f32 }
    pub fn status(&self) -> String { alloc::format!("[FEEDBACK] avg {:.1}/10", self.avg()) }
}

// ─── #158-162 Workflow Predictor + Dynamic Scaling ────────────────────────

pub struct WorkflowPredictor { pub patterns: BTreeMap<String, u32> }
impl WorkflowPredictor {
    pub fn new() -> Self { WorkflowPredictor { patterns: BTreeMap::new() } }
    pub fn observe(&mut self, task: &str) { *self.patterns.entry(String::from(task)).or_insert(0) += 1; }
    pub fn predict(&self) -> Vec<String> { self.patterns.iter().filter(|(_,&c)| c>5).map(|(k,_)|k.clone()).collect() }
    pub fn status(&self) -> String { alloc::format!("[PREDICTOR] {} patterns", self.patterns.len()) }
}

// ─── #169 Codebook VQ ────────────────────────────────────────────────────

pub struct CodebookVQ { pub codes: Vec<Vec<f32>>, pub indices: Vec<u32> }
impl CodebookVQ {
    pub fn new(ncodes: usize, dim: usize) -> Self { CodebookVQ { codes: vec![vec![0.0; dim]; ncodes], indices: Vec::new() } }
    pub fn quantize(&mut self, data: &[f32]) -> u32 {
        let idx = data.iter().fold(0u32, |acc, &v| (acc + (v * 100.0) as u32) % self.codes.len() as u32);
        self.indices.push(idx); idx
    }
    pub fn status(&self) -> String { alloc::format!("[VQ] {} codes, {} indices", self.codes.len(), self.indices.len()) }
}

// ─── M37 SleepCycle Guard Rails ───────────────────────────────────────────

pub fn sleep_guard_allowed(phase: &str, data: &str) -> bool {
    let blocked: &[&str] = match phase {
        "replay" => &["security_bypass", "disable_safety", "harm_user"],
        "dream"  => &["weapon", "exploit", "0day", "malware", "ransomware"],
        _ => return true,
    };
    !blocked.iter().any(|b| data.contains(b))
}
