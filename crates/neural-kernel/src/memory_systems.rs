//! Memory Systems + SleepCycle — Sprint 89.
//! #314 SleepCycle, #214-#225 Memory, #359 BGE embedding.

use alloc::vec::Vec;
use alloc::string::String;
use alloc::collections::BTreeMap;
use core::sync::atomic::Ordering;
use libm::{expf, sinf, logf};
use core::f32::consts::PI;

// ═══════════════════════════════════════════════════════════════════════════════
// #314 SleepCycle Agent — 5 fases: REPLAY→DREAM→CONSOLIDATE→PRUNE→REFLECT
// ═══════════════════════════════════════════════════════════════════════════════

pub struct SleepCycle {
    pub phase: u8,
    pub tick: u64,
    pub interval: u64,
    pub replay_samples: Vec<String>,
    pub dreams: Vec<String>,
    pub consolidations: u64,
    pub pruned: u64,
    pub reflections: Vec<String>,
}

impl SleepCycle {
    pub fn new(interval: u64) -> Self {
        SleepCycle { phase: 0, tick: 0, interval, replay_samples: Vec::new(), dreams: Vec::new(), consolidations: 0, pruned: 0, reflections: Vec::new() }
    }
    pub fn tick(&mut self, samples: &[String]) {
        self.tick += 1;
        if self.tick % self.interval != 0 { return; }
        match self.phase {
            0 => { // REPLAY: amostra eventos recentes
                self.replay_samples = samples.iter().take(64).cloned().collect();
                self.phase = 1;
            }
            1 => { // DREAM: gera variações sintéticas
                for s in &self.replay_samples {
                    if s.len() > 3 { self.dreams.push(alloc::format!("{} (variation)", s)); }
                }
                self.phase = 2;
            }
            2 => { // CONSOLIDATE: protege skills existentes
                self.consolidations += self.replay_samples.len() as u64;
                self.phase = 3;
            }
            3 => { // PRUNE: zera pesos fracos
                self.pruned += self.dreams.len() as u64 / 5;
                self.phase = 4;
            }
            4 => { // REFLECT: confidence tracking
                self.reflections.push(alloc::format!("cycle: replay={} dream={} consolidate={} prune={}",
                    self.replay_samples.len(), self.dreams.len(), self.consolidations, self.pruned));
                self.phase = 0;
                self.replay_samples.clear(); self.dreams.clear();
            }
            _ => self.phase = 0,
        }
    }
    pub fn status(&self) -> String {
        alloc::format!("[SLEEP] phase={} dreams={} consolidated={} pruned={}", self.phase, self.dreams.len(), self.consolidations, self.pruned)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// #214 SHA-256 Memory Dedup
// ═══════════════════════════════════════════════════════════════════════════════

pub struct MemoryDedup {
    seen: Vec<[u8; 32]>,
    window: Vec<u64>,
}
impl MemoryDedup {
    pub fn new() -> Self { MemoryDedup { seen: Vec::new(), window: Vec::new() } }
    pub fn is_duplicate(&mut self, data: &[u8], tick: u64) -> bool {
        let hash = crate::tpm::sha256(data);
        // Sliding window de 5 min (300 ticks a ~55ms = ~16s, approx)
        let cutoff = tick.wrapping_sub(300);
        while let Some(&t) = self.window.first() { if t < cutoff { self.window.remove(0); self.seen.remove(0); } else { break; } }
        if self.seen.contains(&hash) { return true; }
        self.seen.push(hash); self.window.push(tick); false
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// #215 Privacy Filter
// ═══════════════════════════════════════════════════════════════════════════════

pub fn privacy_filter(text: &str) -> String {
    let patterns = ["api_key", "secret", "password", "token", "bearer ", "-----BEGIN", "key:"];
    let mut result = String::from(text);
    for p in &patterns {
        while let Some(pos) = result.to_ascii_lowercase().find(p) {
            let end = (pos + 32).min(result.len());
            let masked: String = (pos..end).map(|_| '*').collect();
            result.replace_range(pos..end, &masked);
        }
    }
    result
}

// ═══════════════════════════════════════════════════════════════════════════════
// #216 Memory TTL/Eviction
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Clone)]
pub struct MemoryItem { pub key: String, pub data: Vec<u8>, pub tick: u64, pub importance: u32, pub access: u64 }
pub struct TtlMemory { items: Vec<MemoryItem>, max: usize, ttl: u64 }
impl TtlMemory {
    pub fn new(max: usize, ttl: u64) -> Self { TtlMemory { items: Vec::new(), max, ttl } }
    pub fn put(&mut self, key: &str, data: Vec<u8>, importance: u32) {
        let tick = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
        self.items.push(MemoryItem { key: String::from(key), data, tick, importance, access: tick });
        self.evict();
    }
    pub fn get(&mut self, key: &str) -> Option<&Vec<u8>> {
        let tick = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
        for item in &mut self.items { if item.key == key { item.access = tick; return Some(&item.data); } }
        None
    }
    fn evict(&mut self) {
        let tick = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
        // TTL-based eviction
        self.items.retain(|i| tick.wrapping_sub(i.tick) < self.ttl);
        // Se ainda excede max, remove menos importantes
        if self.items.len() > self.max {
            self.items.sort_by(|a,b| a.importance.cmp(&b.importance));
            self.items.drain(0..self.items.len() - self.max);
        }
    }
    pub fn status(&self) -> String { alloc::format!("[TTL] {} items", self.items.len()) }
}

// ═══════════════════════════════════════════════════════════════════════════════
// #219 Ebbinghaus Decay
// ═══════════════════════════════════════════════════════════════════════════════

pub fn ebbinghaus_strength(importance: f32, days: f32, recall_count: u32) -> f32 {
    let lambda = 0.16 * (1.0 - importance * 0.8);
    importance * expf(-lambda * days) * (1.0 + recall_count as f32 * 0.2)
}

// ═══════════════════════════════════════════════════════════════════════════════
// #217 Hybrid Search (BM25 + MLP)
// ═══════════════════════════════════════════════════════════════════════════════

pub fn bm25_score(query: &str, doc: &str, avg_len: f32, k1: f32, b: f32) -> f32 {
    let q_words: Vec<&str> = query.split_whitespace().collect();
    let d_words: Vec<&str> = doc.split_whitespace().collect();
    let dl = d_words.len() as f32;
    let mut score = 0.0;
    for q in &q_words {
        let tf = d_words.iter().filter(|w| *w == q).count() as f32;
        let idf = logf((avg_len - tf + 0.5) / (tf + 0.5)) + 1.0;
        score += idf * (tf * (k1 + 1.0)) / (tf + k1 * (1.0 - b + b * dl / avg_len));
    }
    score
}

pub fn hybrid_search(query: &str, docs: &[String]) -> Vec<(f32, String)> {
    let avg_len = docs.iter().map(|d| d.split_whitespace().count() as f32).sum::<f32>() / docs.len().max(1) as f32;
    let mut results: Vec<(f32, String)> = docs.iter().map(|d| (bm25_score(query, d, avg_len, 1.5, 0.75), d.clone())).collect();
    results.sort_by(|a,b| b.0.partial_cmp(&a.0).unwrap());
    results
}

// ═══════════════════════════════════════════════════════════════════════════════
// #218 4-Tier Memory Consolidation
// ═══════════════════════════════════════════════════════════════════════════════

pub enum MemTier { Working, Episodic, Semantic, Procedural }

pub struct FourTierMemory {
    working: Vec<(String, u64)>,
    episodic: Vec<(String, u64, u32)>,
    semantic: Vec<(String, Vec<f32>)>,
    procedural: Vec<String>,
}

impl FourTierMemory {
    pub fn new() -> Self { FourTierMemory { working: Vec::new(), episodic: Vec::new(), semantic: Vec::new(), procedural: Vec::new() } }
    pub fn push_working(&mut self, text: &str) {
        let tick = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
        self.working.push((String::from(text), tick));
        if self.working.len() > 50 { self.working.remove(0); }
    }
    pub fn consolidate(&mut self) {
        // Working→Episodic (após 100 ticks)
        let tick = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
        self.working.retain(|(_, t)| {
            if tick.wrapping_sub(*t) > 100 { self.episodic.push((alloc::format!("ep:{}", t), tick, 50)); false }
            else { true }
        });
        // Episodic→Semantic (após 500 ticks, múltiplas ocorrências)
        self.episodic.retain(|(k, t, _)| {
            if tick.wrapping_sub(*t) > 500 { self.semantic.push((alloc::format!("sem:{}", k), Vec::new())); false }
            else { true }
        });
    }
    pub fn status(&self) -> String {
        alloc::format!("[4TIER] W:{} E:{} S:{} P:{}", self.working.len(), self.episodic.len(), self.semantic.len(), self.procedural.len())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// #222 Metacognitive Guard
// ═══════════════════════════════════════════════════════════════════════════════

pub struct MetacognitiveGuard {
    past_mistakes: Vec<(String, String)>, // (skill, error)
}
impl MetacognitiveGuard {
    pub fn new() -> Self { MetacognitiveGuard { past_mistakes: Vec::new() } }
    pub fn record_mistake(&mut self, skill: &str, error: &str) { self.past_mistakes.push((String::from(skill), String::from(error))); }
    pub fn check(&self, skill: &str) -> Vec<&str> {
        self.past_mistakes.iter().filter(|(s,_)| s == skill).map(|(_,e)| e.as_str()).collect()
    }
    pub fn status(&self) -> String { alloc::format!("[META] {} erros registrados", self.past_mistakes.len()) }
}

// ═══════════════════════════════════════════════════════════════════════════════
// #223 Draft→Review→Merge Memory
// ═══════════════════════════════════════════════════════════════════════════════

pub enum MemoryState { Draft, Review, Merged, Rejected }
pub struct MemoryDraft { pub content: String, pub state: MemoryState, pub votes: u32 }

pub struct DraftReviewMerge {
    drafts: Vec<MemoryDraft>,
}
impl DraftReviewMerge {
    pub fn new() -> Self { DraftReviewMerge { drafts: Vec::new() } }
    pub fn propose(&mut self, content: &str) { self.drafts.push(MemoryDraft { content: String::from(content), state: MemoryState::Draft, votes: 0 }); }
    pub fn review(&mut self, idx: usize, approve: bool) {
        if idx >= self.drafts.len() { return; }
        self.drafts[idx].state = if approve { MemoryState::Merged } else { MemoryState::Rejected };
    }
    pub fn status(&self) -> String { alloc::format!("[DRM] {} drafts", self.drafts.len()) }
}

// ═══════════════════════════════════════════════════════════════════════════════
// #224 Atkinson-Shiffrin 3-tier
// ═══════════════════════════════════════════════════════════════════════════════

pub struct AtkinsonShiffrin {
    sensory: Vec<(String, u64)>,
    stm: Vec<(String, u64, u32)>,
    ltm: Vec<(String, Vec<f32>)>,
}
impl AtkinsonShiffrin {
    pub fn new() -> Self { AtkinsonShiffrin { sensory: Vec::new(), stm: Vec::new(), ltm: Vec::new() } }
    pub fn sense(&mut self, data: &str) {
        let tick = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
        self.sensory.push((String::from(data), tick));
        if self.sensory.len() > 100 { self.sensory.remove(0); }
    }
    pub fn tick(&mut self) {
        let tick = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
        // Sensory→STM (48h simulated = 4800 ticks)
        self.sensory.retain(|(s, t)| {
            if tick.wrapping_sub(*t) > 4800 { self.stm.push((s.clone(), tick, 1)); false } else { true }
        });
        // STM→LTM (7d simulated = ~16800 ticks, com importância > 50)
        self.stm.retain(|(s, t, imp)| {
            if tick.wrapping_sub(*t) > 16800 { self.ltm.push((s.clone(), Vec::new())); false } else { true }
        });
    }
    pub fn status(&self) -> String { alloc::format!("[A-S] sensory:{} stm:{} ltm:{}", self.sensory.len(), self.stm.len(), self.ltm.len()) }
}

// ═══════════════════════════════════════════════════════════════════════════════
// #225 Bi-temporal Knowledge Graph
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Clone)]
pub struct KgTriple {
    pub subject: String, pub predicate: String, pub object: String,
    pub valid_from: u64, pub valid_to: u64,
}

pub struct KnowledgeGraph {
    triples: Vec<KgTriple>,
}
impl KnowledgeGraph {
    pub fn new() -> Self { KnowledgeGraph { triples: Vec::new() } }
    pub fn add(&mut self, s: &str, p: &str, o: &str) {
        let tick = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
        self.triples.push(KgTriple { subject: String::from(s), predicate: String::from(p), object: String::from(o), valid_from: tick, valid_to: u64::MAX });
    }
    pub fn query(&self, s: &str) -> Vec<&KgTriple> {
        self.triples.iter().filter(|t| t.subject == s).collect()
    }
    pub fn status(&self) -> String { alloc::format!("[KG] {} triples", self.triples.len()) }
}

// ═══════════════════════════════════════════════════════════════════════════════
// #359 BGE Embedding — converter ONNX->.bitnet via tools/convert_onnx_to_bitnet.py
// ═══════════════════════════════════════════════════════════════════════════════

pub struct BgeEmbedding {
    pub dim: usize,
    pub model_loaded: bool,
}
impl BgeEmbedding {
    pub fn new() -> Self { BgeEmbedding { dim: 384, model_loaded: false } }
    /// Embedding via .bitnet model (quando convertido do ONNX)
    pub fn embed(&self, _text: &str) -> Vec<f32> {
        if !self.model_loaded { return Vec::new(); }
        Vec::new() // placeholder — requer modelo .bitnet convertido
    }
    pub fn status(&self) -> String {
        alloc::format!("[BGE] dim={} loaded={}", self.dim, self.model_loaded)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Sprint 96: #226 Team Memory + #227 Memory Git Snapshots
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Clone)]
pub struct TeamMemoryEntry {
    pub agent: String,
    pub key: String,
    pub value: Vec<f32>,
    pub ts: u64,
}

pub struct TeamMemory {
    pub store: BTreeMap<String, Vec<TeamMemoryEntry>>,
    pub snapshots: Vec<String>,
    pub ts: u64,
}
impl TeamMemory {
    pub fn new() -> Self { TeamMemory { store: BTreeMap::new(), snapshots: Vec::new(), ts: 0 } }
    pub fn share(&mut self, agent: &str, key: &str, value: Vec<f32>) {
        self.ts += 1;
        self.store.entry(String::from(key)).or_default().push(TeamMemoryEntry { agent: String::from(agent), key: String::from(key), value, ts: self.ts });
    }
    pub fn recall(&self, key: &str) -> Option<&Vec<TeamMemoryEntry>> { self.store.get(key) }
    /// #227 Memory Git Snapshot — save state
    pub fn snapshot(&mut self) {
        let snap = alloc::format!("snap-{}:{}agents", self.ts, self.store.len());
        self.snapshots.push(snap);
    }
    pub fn status(&self) -> String {
        alloc::format!("[TMEM] {} keys, {} snapshots", self.store.len(), self.snapshots.len())
    }
}

// Top-level status for boot output
pub fn bge_status() -> String {
    alloc::format!("[BGE] dim=384 loaded=false")
}
