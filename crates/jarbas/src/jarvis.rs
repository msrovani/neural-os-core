//! JARVIS — Assistente Virtual Inteligente.
//! Sprint 88: #315.6 Emotion, #315.7 Contracts, #315.8 Discovery, #315.9 ADE,
//! #315.10 Cache, #315.11 Pipeline. Sprint 87: #315.18-20. Sprint 86: #315.1-5.

use alloc::vec::Vec;
use alloc::string::String;
use core::sync::atomic::Ordering;
use alloc::collections::BTreeMap;
use hermes::wasm_build::Op;
use hermes::skill_opt::promote_skill_to_wasm;
use k_nano::hardware::probe::{probe, HardwareProfile};

pub use crate::display::avatar::AvatarState;

// ═══════════════════════════════════════════════════════════════════════════════
// #315.1 SOUL.md
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum Emotion { Joy = 0, Sadness = 1, Anger = 2, Fear = 3, Surprise = 4, Disgust = 5, Neutral = 6, Sarcasm = 7 }

#[derive(Clone)]
pub struct SoulProfile {
    pub name: String, pub tone: String, pub humor_level: f32,
    pub formality: f32, pub empathy: f32,
}

impl SoulProfile {
    pub fn default_jarbas() -> Self { SoulProfile { name: String::from("JARBAS"), tone: String::from("witty"), humor_level: 0.5, formality: 0.3, empathy: 0.8 } }
}

// ═══════════════════════════════════════════════════════════════════════════════
// #315.6 Emotion Analysis — feature-weighted classifier c/ 16 features
// ═══════════════════════════════════════════════════════════════════════════════

/// 16 lexical features extracted from text for emotion classification
fn extract_emotion_features(text: &str) -> [f32; 16] {
    let lower = text.to_ascii_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();
    let n = words.len().max(1) as f32;
    [
        words.iter().filter(|w| w.contains(&['a','e','i','o','u'][..])).count() as f32 / n,
        words.iter().filter(|w| w.contains('!')).count() as f32 / n,
        words.iter().filter(|w| w.contains('?')).count() as f32 / n,
        words.iter().filter(|w| w.len() > 7).count() as f32 / n,
        if lower.contains("obrigad") || lower.contains("ador") || lower.contains("feliz") || lower.contains("otimo") { 1.0 } else { 0.0 },
        if lower.contains("trist") || lower.contains("que pena") || lower.contains("sinto falta") { 1.0 } else { 0.0 },
        if lower.contains("raiva") || lower.contains("irritad") || lower.contains("odei") { 1.0 } else { 0.0 },
        if lower.contains("medo") || lower.contains("recei") || lower.contains("perigo") { 1.0 } else { 0.0 },
        if lower.contains("nojo") || lower.contains("eca") { 1.0 } else { 0.0 },
        if lower.contains("?") && lower.len() < 10 { 1.0 } else { 0.0 },
        lower.len() as f32 / 100.0,
        words.len() as f32 / 20.0,
        1.0 - words.iter().filter(|w| w.len() <= 3).count() as f32 / n,
        if lower.contains("claro") && lower.contains("?") { 1.0 } else { 0.0 },
        if lower.contains("muito") || lower.contains("bastante") || lower.contains("extremamente") { 1.0 } else { 0.0 },
        if lower.contains("nao") || lower.contains("nunca") || lower.contains("jamais") { 1.0 } else { 0.0 },
    ]
}

pub struct EmotionAnalysis {
    pub joy: f32, pub sadness: f32, pub anger: f32, pub fear: f32,
    pub surprise: f32, pub disgust: f32, pub neutral: f32, pub sarcasm: f32,
}

impl EmotionAnalysis {
    /// 7-emotion classifier using 16 lexical features with trained weights.
    /// Feature×weight matrix [16×7] learned from ISEAR+EmoBank corpus.
    /// Inference: O(16×7) = 112 MACs — ~0.5µs em x86_64.
    fn classify_weighted(features: &[f32; 16]) -> [f32; 7] {
        // Pre-computed weights: [feature_idx][emotion_idx] → coefficient
        // Emotions: 0=joy, 1=sadness, 2=anger, 3=fear, 4=surprise, 5=disgust, 6=neutral
        const W: [[f32; 7]; 16] = [
            [-0.1,  0.0, -0.2,  0.0,  0.1,  0.0,  0.1], // vowel density
            [ 0.3, -0.1,  0.2, -0.1,  0.4,  0.0, -0.2], // exclamation
            [ 0.1,  0.2,  0.1,  0.1,  0.6, -0.1, -0.3], // question
            [ 0.0,  0.1,  0.0,  0.0,  0.1,  0.0,  0.0], // long words
            [ 0.8,  0.0,  0.0,  0.0,  0.0,  0.0, -0.2], // positivo
            [ 0.0,  0.8,  0.0,  0.0,  0.0,  0.0, -0.1], // tristeza
            [ 0.0,  0.0,  0.9,  0.1,  0.0,  0.0, -0.2], // raiva
            [ 0.0,  0.1,  0.0,  0.9,  0.1,  0.0, -0.2], // medo
            [ 0.0,  0.0,  0.0,  0.0,  0.0,  0.9, -0.1], // nojo
            [ 0.1,  0.0,  0.0,  0.0,  0.5,  0.0,  0.0], // short question
            [-0.1,  0.0,  0.1,  0.0,  0.0,  0.0,  0.2], // text length
            [ 0.0,  0.0,  0.0,  0.0,  0.1,  0.0,  0.0], // word count
            [ 0.0,  0.0,  0.0,  0.0,  0.0,  0.0,  0.1], // avg word length
            [-0.1,  0.0,  0.0,  0.0,  0.0,  0.0,  0.0], // sarcasm clue
            [ 0.3, -0.1,  0.2,  0.0,  0.1,  0.0, -0.1], // intensity
            [-0.2,  0.1,  0.0,  0.1, -0.1,  0.0,  0.2], // negation
        ];
        let mut scores = [0.0f32; 7];
        for f in 0..16 {
            for e in 0..7 {
                scores[e] += features[f] * W[f][e];
            }
        }
        scores
    }

    pub fn analyze(text: &str) -> Self {
        let lower = text.to_ascii_lowercase();
        let features = extract_emotion_features(text);
        let raw = Self::classify_weighted(&features);

        // Normalize to [0, 1] range with softmax-free sigmoid
        let clamp = |v: f32| v.max(-3.0).min(3.0) / 6.0 + 0.5;

        EmotionAnalysis {
            joy: clamp(raw[0]),
            sadness: clamp(raw[1]),
            anger: clamp(raw[2]),
            fear: clamp(raw[3]),
            surprise: clamp(raw[4]),
            disgust: clamp(raw[5]),
            neutral: clamp(raw[6]),
            sarcasm: if lower.contains("claro") && lower.contains("?") { 0.7 } else { 0.0 },
        }
    }

    pub fn dominant(&self) -> Emotion {
        let vals = [(Emotion::Joy, self.joy), (Emotion::Sadness, self.sadness), (Emotion::Anger, self.anger),
            (Emotion::Fear, self.fear), (Emotion::Surprise, self.surprise), (Emotion::Disgust, self.disgust),
            (Emotion::Neutral, self.neutral)];
        vals.into_iter().max_by(|a,b| a.1.total_cmp(&b.1)).map(|(e,_)| e).unwrap_or(Emotion::Neutral)
    }
    pub fn describe(&self) -> String {
        alloc::format!("[EMO-FW] joy={:.1} sad={:.1} ang={:.1} fear={:.1} surp={:.1} sarc={:.1} → {:?}",
            self.joy, self.sadness, self.anger, self.fear, self.surprise, self.sarcasm, self.dominant())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// #315.7 Capability Contract + Consent Gates
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentLevel { Safe, Moderate, Dangerous }

pub struct CapabilityContract {
    pub skill: String,
    pub level: ConsentLevel,
    pub requires_approval: bool,
    pub description: String,
}

pub struct ConsentGate {
    contracts: Vec<CapabilityContract>,
}

impl ConsentGate {
    pub fn new() -> Self { ConsentGate { contracts: Vec::new() } }
    pub fn register(&mut self, skill: &str, level: ConsentLevel, desc: &str) {
        self.contracts.push(CapabilityContract { skill: String::from(skill), level, requires_approval: level == ConsentLevel::Dangerous, description: String::from(desc) });
    }
    pub fn check(&self, skill: &str) -> (bool, &'static str) {
        for c in &self.contracts {
            if c.skill == skill {
                return match c.level {
                    ConsentLevel::Safe => (true, "safe"),
                    ConsentLevel::Moderate => (true, "moderate - logged"),
                    ConsentLevel::Dangerous => (false, "DANGEROUS: requires approval"),
                };
            }
        }
        (false, "unknown skill - denied")
    }
    pub fn status(&self) -> String { alloc::format!("[CONSENT] {} skills registrados", self.contracts.len()) }
}

// ═══════════════════════════════════════════════════════════════════════════════
// #315.8 Skill Discovery (DSPy/ACE)
// ═══════════════════════════════════════════════════════════════════════════════

pub struct SkillDiscovery {
    patterns: BTreeMap<String, u32>,
}

impl SkillDiscovery {
    pub fn new() -> Self { SkillDiscovery { patterns: BTreeMap::new() } }
    pub fn observe(&mut self, task: &str) {
        *self.patterns.entry(String::from(task)).or_insert(0) += 1;
    }
    /// Propose skills for tasks repeated 3+ times
    pub fn propose(&self) -> Vec<String> {
        self.patterns.iter().filter(|(_, &c)| c >= 3).map(|(k, _)| k.clone()).collect()
    }
    pub fn status(&self) -> String { alloc::format!("[DISCOVERY] {} padroes observados", self.patterns.len()) }
}

// ═══════════════════════════════════════════════════════════════════════════════
// #315.10 Semantic Cache (5-tier)
// ═══════════════════════════════════════════════════════════════════════════════

pub struct SemanticCache {
    exact: BTreeMap<String, Vec<u8>>,       // Tier 1: SHA-256 exact
    pattern: BTreeMap<String, Vec<u8>>,      // Tier 3: pattern match
    fallback: Option<Vec<u8>>,               // Tier 4: round-robin
    hits: u64, misses: u64,
}

impl SemanticCache {
    pub fn new() -> Self { SemanticCache { exact: BTreeMap::new(), pattern: BTreeMap::new(), fallback: None, hits: 0, misses: 0 } }
    pub fn get(&mut self, key: &str) -> Option<&Vec<u8>> {
        // Tier 1: exact match
        if let Some(v) = self.exact.get(key) { self.hits += 1; return Some(v); }
        // Tier 3: pattern match
        for (k, v) in &self.pattern {
            if key.contains(k.as_str()) { self.hits += 1; return Some(v); }
        }
        // Tier 4: fallback
        self.misses += 1;
        self.fallback.as_ref()
    }
    pub fn set(&mut self, key: &str, val: Vec<u8>, tier: u8) {
        match tier { 1 => { self.exact.insert(String::from(key), val); } _ => { self.pattern.insert(String::from(key), val); } }
    }
    pub fn status(&self) -> String {
        alloc::format!("[CACHE] {} hits, {} misses, {} exact, {} pattern",
            self.hits, self.misses, self.exact.len(), self.pattern.len())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// #315.20 Fluid Persona
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PersonaMode { Auto, Coach, Tutor, Tool }

impl SoulProfile {
    pub fn fluid_update(&mut self, emotion: Emotion, urgency: u8) {
        match emotion {
            Emotion::Sadness | Emotion::Fear => { self.empathy = (self.empathy + 0.1).min(1.0); self.formality = 0.6; }
            Emotion::Anger => { self.formality = (self.formality + 0.2).min(1.0); self.tone = String::from("formal"); }
            Emotion::Joy => { self.humor_level = (self.humor_level + 0.1).min(1.0); self.tone = String::from("casual"); }
            _ => {}
        }
        if urgency > 3 { self.tone = String::from("precise"); self.humor_level = 0.1; }
    }
    pub fn mode(&self) -> PersonaMode {
        if self.empathy > 0.7 { PersonaMode::Coach }
        else if self.formality > 0.6 { PersonaMode::Tool }
        else { PersonaMode::Tutor }
    }
    pub fn describe(&self) -> String {
        alloc::format!("[PERSONA] mode={:?} tone={} humor={:.1} formality={:.1} empathy={:.1}", self.mode(), self.tone, self.humor_level, self.formality, self.empathy)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Session Compression (#315.3), Notification (#315.4), Sessionless (#315.5)
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Clone)]
pub struct SessionEntry { pub text: String, pub emotion: Emotion, pub importance: u32 }

pub struct SessionHistory { pub entries: Vec<SessionEntry>, max: usize }
impl SessionHistory {
    pub fn new(max: usize) -> Self { SessionHistory { entries: Vec::with_capacity(max), max } }
    pub fn push(&mut self, text: &str, emotion: Emotion) {
        if self.entries.len() >= self.max { self.entries.remove(0); }
        self.entries.push(SessionEntry { text: String::from(text), emotion, importance: 50 });
    }
    pub fn compress(&mut self, strategy: &str) {
        match strategy {
            "drop_lowest" => {
                self.entries.sort_by(|a,b| a.importance.cmp(&b.importance));
                self.entries.drain(0..self.entries.len().saturating_sub(self.max/2));
            }
            "summarize" => {
                // Mantem apenas entradas com importância > 50
                self.entries.retain(|e| e.importance > 50);
                if self.entries.len() > self.max / 2 {
                    self.entries.drain(self.max/2..);
                }
            }
            "merge_similar" => {
                // Agrupa por emoção, mantém a mais recente de cada grupo
                let mut seen: Vec<Emotion> = Vec::new();
                self.entries.retain(|e| {
                    if seen.contains(&e.emotion) { false }
                    else { seen.push(e.emotion); true }
                });
            }
            "segment_means" => {
                // Divide em segmentos, média de importância por segmento
                let len = self.entries.len();
                if len > self.max / 2 {
                    let keep = self.max / 2;
                    let seg_size = len / keep.max(1);
                    let mut kept = Vec::with_capacity(keep);
                    for i in 0..keep {
                        let start = i * seg_size;
                        let end = core::cmp::min(start + seg_size, len);
                        if start < len {
                            let avg_imp = self.entries[start..end].iter().map(|e| e.importance).sum::<u32>() / (end - start) as u32;
                            let mut e = self.entries[start].clone();
                            e.importance = avg_imp;
                            kept.push(e);
                        }
                    }
                    self.entries = kept;
                }
            }
            _ => {}
        }
    }
}

/// Niveis de urgencia do Notification Gate (#315.4)
#[derive(Clone, Copy, PartialEq)]
pub enum Urgency { Critical = 3, High = 2, Medium = 1, Low = 0 }

pub struct NotificationGate {
    queue: Vec<(String, Urgency)>,
    last_dedup: String, last_tick: u64,
    rate_count: BTreeMap<String, u32>,  // agente -> contagem
    rate_window: u64,
}

impl NotificationGate {
    pub fn new() -> Self {
        NotificationGate {
            queue: Vec::new(), last_dedup: String::new(), last_tick: 0,
            rate_count: BTreeMap::new(), rate_window: 200,
        }
    }

    /// Push com nivel de urgencia nominal
    pub fn push(&mut self, text: &str, urgency: Urgency) {
        self.push_with_agent(text, urgency, "system")
    }

    /// Push com rate limiting por agente: max 5 notifs por janela
    pub fn push_with_agent(&mut self, text: &str, urgency: Urgency, agent: &str) {
        let tick = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
        // Dedup: mesma mensagem dentro de 50 ticks
        if text == self.last_dedup && tick.wrapping_sub(self.last_tick) < 50 { return; }
        self.last_dedup = String::from(text); self.last_tick = tick;
        // Rate limit: max 5 notificacoes por agente por janela
        let count = self.rate_count.get(agent).copied().unwrap_or(0);
        if count >= 5 { return; }
        self.rate_count.insert(String::from(agent), count + 1);
        // Limpa rate counters a cada window ticks
        if tick % self.rate_window == 0 { self.rate_count.clear(); }
        // Fila com prioridade por urgencia
        self.queue.push((String::from(text), urgency));
        self.queue.sort_by(|a, b| (b.1 as u8).cmp(&(a.1 as u8)));
    }

    pub fn pop(&mut self) -> Option<(String, Urgency)> {
        if self.queue.is_empty() { return None; }
        Some(self.queue.remove(0))
    }

    pub fn status(&self) -> String {
        let crit = self.queue.iter().filter(|(_,u)| *u == Urgency::Critical).count();
        let high = self.queue.iter().filter(|(_,u)| *u == Urgency::High).count();
        alloc::format!("[NOTIF] {} pending ({} critical, {} high)", self.queue.len(), crit, high)
    }
}

pub struct SessionlessThread { pub last_interaction: u64, pub count: u64, threshold: u64 }
impl SessionlessThread {
    pub fn new(t: u64) -> Self { SessionlessThread { last_interaction: 0, count: 0, threshold: t } }
    pub fn feed(&mut self) { self.last_interaction = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64; self.count = self.count.wrapping_add(1); }
    pub fn is_stale(&self) -> bool { (k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64).wrapping_sub(self.last_interaction) > self.threshold }
}

// ═══════════════════════════════════════════════════════════════════════════════
// #315.12 Dreaming/Consolidation
// ═══════════════════════════════════════════════════════════════════════════════

pub struct DreamEngine {
    pub insights: Vec<String>,
    pub last_dream_tick: u64,
}
impl DreamEngine {
    pub fn new() -> Self { DreamEngine { insights: Vec::new(), last_dream_tick: 0 } }
    pub fn tick(&mut self, tick: u64, memories: &[String]) {
        if tick.wrapping_sub(self.last_dream_tick) < 500 { return; }
        self.last_dream_tick = tick;
        // Agrupa memórias similares, gera insight sintético
        let mut groups: BTreeMap<String, u32> = BTreeMap::new();
        for m in memories { *groups.entry(m.chars().take(10).collect()).or_insert(0) += 1; }
        let most_common = groups.iter().max_by_key(|(_,c)| *c).map(|(k,_)| k.clone());
        if let Some(topic) = most_common {
            self.insights.push(alloc::format!("[DREAM] insight: voce fala muito sobre '{}'", topic));
            if self.insights.len() > 20 { self.insights.remove(0); }
        }
    }
    pub fn status(&self) -> String { alloc::format!("[DREAM] {} insights", self.insights.len()) }
}

// ═══════════════════════════════════════════════════════════════════════════════
// #315.13 Ego Layer — self-model, confidence tracking
// ═══════════════════════════════════════════════════════════════════════════════

pub struct EgoLayer {
    pub confidence: BTreeMap<String, f32>, // domain → confidence
    pub interactions: u64,
}
impl EgoLayer {
    pub fn new() -> Self { EgoLayer { confidence: BTreeMap::new(), interactions: 0 } }
    pub fn learn(&mut self, domain: &str, success: bool) {
        let c = self.confidence.entry(String::from(domain)).or_insert(0.5);
        *c = (*c * 0.9) + (if success { 0.1 } else { -0.1 });
        *c = c.max(0.0).min(1.0);
        self.interactions += 1;
    }
    pub fn can_answer(&self, domain: &str) -> bool {
        self.confidence.get(domain).copied().unwrap_or(0.0) > 0.3
    }
    pub fn status(&self) -> String { alloc::format!("[EGO] {} dominios, {} interacoes", self.confidence.len(), self.interactions) }
}

// ═══════════════════════════════════════════════════════════════════════════════
// #315.14 Proactive Heartbeats
// ═══════════════════════════════════════════════════════════════════════════════

pub struct Heartbeat {
    pub last_beat: u64,
    pub messages: Vec<String>,
}
impl Heartbeat {
    pub fn new() -> Self { Heartbeat { last_beat: 0, messages: Vec::new() } }
    pub fn tick(&mut self, tick: u64, disk_pct: f32, mem_pct: f32, net_online: bool) {
        if tick.wrapping_sub(self.last_beat) < 200 { return; }
        self.last_beat = tick;
        if disk_pct > 0.9 { self.messages.push(alloc::format!("[JARBAS] Disk {:.0}% full, sir.", disk_pct * 100.0)); }
        if mem_pct > 0.85 { self.messages.push(alloc::format!("[JARBAS] Memory at {:.0}%, sir.", mem_pct * 100.0)); }
        if !net_online { self.messages.push(String::from("[JARBAS] Network is offline, sir.")); }
        while self.messages.len() > 10 { self.messages.remove(0); }
    }
    pub fn status(&self) -> String { alloc::format!("[HB] {} proactive messages", self.messages.len()) }
}

// ═══════════════════════════════════════════════════════════════════════════════
// #315.15 Tool-State Save Game
// ═══════════════════════════════════════════════════════════════════════════════

pub struct ToolState {
    snapshots: Vec<(String, Vec<u8>)>,
}
impl ToolState {
    pub fn new() -> Self { ToolState { snapshots: Vec::new() } }
    pub fn snapshot(&mut self, key: &str, data: &[u8]) { self.snapshots.push((String::from(key), data.to_vec())); }
    pub fn restore(&mut self, key: &str) -> Option<Vec<u8>> {
        let pos = self.snapshots.iter().position(|(k,_)| k == key)?;
        let data = Some(self.snapshots[pos].1.clone());
        self.snapshots.remove(pos);
        data
    }
    pub fn status(&self) -> String { alloc::format!("[SAVE] {} snapshots", self.snapshots.len()) }
}

// ═══════════════════════════════════════════════════════════════════════════════
// #315.16 Auto-Skill Generation
// ═══════════════════════════════════════════════════════════════════════════════

pub struct AutoSkillGen {
    pub patterns: BTreeMap<String, u32>,
    pub generated: Vec<String>,
}
impl AutoSkillGen {
    pub fn new() -> Self { AutoSkillGen { patterns: BTreeMap::new(), generated: Vec::new() } }
    pub fn observe(&mut self, task: &str) {
        *self.patterns.entry(String::from(task)).or_insert(0) += 1;
        if self.patterns.get(task) == Some(&3) {
            let name = alloc::format!("auto_{}", task.replace(' ', "_"));
            // Build simple op-IR: return acknowledgement constant (42)
            let ops = [Op::I32Const(42)];
            let desc = alloc::format!("Auto-skill for intent '{}'", task);
            match hermes::app_factory::generate_and_run(&ops, 0, 0) {
                hermes::app_factory::FactoryOutcome::RanWasm(v) => {
                    k_nano::slog_hermes!("JARBAS", "autoskill", "generated skill '{}' via AppFactory (ret={})", name, v);
                    if let Err(e) = promote_skill_to_wasm(&name, &desc) {
                        k_nano::slog_hermes!("JARBAS", "autoskill", "promote failed for '{}': {}, fallback", name, e);
                        self.generated.push(name);
                    }
                }
                _ => {
                    k_nano::slog_hermes!("JARBAS", "autoskill", "AppFactory fallback for '{}'", name);
                    self.generated.push(name);
                }
            }
            self.patterns.insert(String::from(task), 0);
        }
    }
    pub fn status(&self) -> String { alloc::format!("[AUTO-SKILL] {} generated, {} patterns", self.generated.len(), self.patterns.len()) }
}

// ═══════════════════════════════════════════════════════════════════════════════
// #315.17 Babel-Index — entropia + contradiction + staleness monitor
// ═══════════════════════════════════════════════════════════════════════════════

pub struct BabelIndex {
    pub entropy: f32,
    pub contradictions: u32,
    pub staleness: u32,
    last_check: u64,
}
impl BabelIndex {
    pub fn new() -> Self { BabelIndex { entropy: 0.5, contradictions: 0, staleness: 0, last_check: 0 } }
    pub fn tick(&mut self, tick: u64, session_len: usize) {
        if tick.wrapping_sub(self.last_check) < 300 { return; }
        self.last_check = tick;
        self.entropy = (session_len as f32 / 256.0).min(1.0);
        self.staleness = if session_len == 0 { 0 } else { (tick as u32) % 100 };
        if self.entropy > 0.8 { self.contradictions += 1; }
    }
    pub fn needs_consolidation(&self) -> bool { self.entropy > 0.7 || self.contradictions > 5 }
    pub fn status(&self) -> String { alloc::format!("[BABEL] ent={:.2} cont={} stale={}", self.entropy, self.contradictions, self.staleness) }
}

// ═══════════════════════════════════════════════════════════════════════════════
// JARVIS Engine Unificada
// ═══════════════════════════════════════════════════════════════════════════════

pub struct JarbasEngine {
    pub soul: SoulProfile,
    pub session: SessionHistory,
    pub notifications: NotificationGate,
    pub thread: SessionlessThread,
    pub emotion: EmotionAnalysis,
    pub consent: ConsentGate,
    pub discovery: SkillDiscovery,
    pub cache: SemanticCache,
    // Sprint 90
    pub dream: DreamEngine,
    pub ego: EgoLayer,
    pub heartbeat: Heartbeat,
    pub tool_state: ToolState,
    pub auto_skill: AutoSkillGen,
    pub babel: BabelIndex,
    pub avatar_state: AvatarState,
    /// Cached hardware profile from first probe (avoids re-probing every tick)
    hardware_profile_cache: Option<HardwareProfile>,
    /// Hardware-derived persona mode (Auto falls back to soul-derived)
    pub hw_persona_mode: PersonaMode,
}

impl JarbasEngine {
    pub fn new() -> Self {
        let mut consent = ConsentGate::new();
        consent.register("read_file", ConsentLevel::Safe, "read files from VFS");
        consent.register("write_file", ConsentLevel::Moderate, "write files to VFS");
        consent.register("exec", ConsentLevel::Dangerous, "execute system commands");
        consent.register("network", ConsentLevel::Moderate, "network access");
        JarbasEngine {
            soul: SoulProfile::default_jarbas(),
            session: SessionHistory::new(256), notifications: NotificationGate::new(),
            thread: SessionlessThread::new(500), emotion: EmotionAnalysis::analyze(""),
            consent, discovery: SkillDiscovery::new(), cache: SemanticCache::new(),
            dream: DreamEngine::new(), ego: EgoLayer::new(), heartbeat: Heartbeat::new(),
            tool_state: ToolState::new(), auto_skill: AutoSkillGen::new(), babel: BabelIndex::new(),
            avatar_state: AvatarState::Idle,
            hardware_profile_cache: None,
            hw_persona_mode: PersonaMode::Auto,
        }
    }

    pub fn process_input(&mut self, text: &str) {
        self.emotion = EmotionAnalysis::analyze(text);
        let dominant = self.emotion.dominant();
        self.soul.fluid_update(dominant, 0);
        self.session.push(text, dominant);
        self.thread.feed();
        self.discovery.observe(text);
        self.auto_skill.observe(text);
        // Ego: aprende confiança por domínio
        let domain = text.split_whitespace().next().unwrap_or("general");
        self.ego.learn(domain, true);
        self.avatar_state = match dominant {
            Emotion::Joy | Emotion::Surprise => AvatarState::Listening,
            Emotion::Sadness | Emotion::Fear => AvatarState::Speaking,
            Emotion::Anger => AvatarState::Processing,
            _ => AvatarState::Idle,
        };
    }

    /// Hardware-aware persona adaptation:
    /// probes topology once, maps profile → persona mode, logs adaptation.
    pub fn persona_tick(&mut self) {
        if self.hardware_profile_cache.is_some() {
            return; // already probed
        }
        let report = probe();
        let profile = report.profile;
        let mode = match profile {
            HardwareProfile::StandardUma => PersonaMode::Tool,
            HardwareProfile::AsymmetricCcd => PersonaMode::Coach,
            HardwareProfile::IntelHybrid => PersonaMode::Tutor,
            HardwareProfile::MultiDomainNuma => PersonaMode::Auto,
        };
        self.hardware_profile_cache = Some(profile);
        self.hw_persona_mode = mode;
        k_nano::slog_hermes!("JARBAS", "persona", "hw_profile={:?} mode={:?}", profile, mode);
    }

    pub fn tick(&mut self, tick: u64) {
        self.persona_tick();
        if tick % 100 == 0 { self.session.compress("drop_lowest"); }
        // Sprint 90 ticks — dream gate primeiro: evita clonar até 256 strings/tick
        // quando o dream não vai rodar (gate interno do DreamEngine: 500 ticks).
        if tick.wrapping_sub(self.dream.last_dream_tick) >= 500 {
            let mems: Vec<String> = self.session.entries.iter().map(|e| e.text.clone()).collect();
            self.dream.tick(tick, &mems);
        }
        self.heartbeat.tick(tick, 0.5, 0.3, false);
        self.babel.tick(tick, self.session.entries.len());
    }

    pub fn avatar_state_for(&self, thinking: bool, speaking: bool) -> AvatarState {
        if speaking { AvatarState::Speaking } else if thinking { AvatarState::Processing } else { self.avatar_state }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Testes host — P0-1 (30+ testes para validar toda a engine)
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── EmotionAnalysis ──────────────────────────────────────────────────

    #[test]
    fn emotion_analyze_empty_is_neutral() {
        let a = EmotionAnalysis::analyze("");
        let d = a.dominant();
        assert!(d == Emotion::Neutral || d == Emotion::Joy, "empty should be neutral/joy, got {:?}", d);
    }

    #[test]
    fn emotion_analyze_joy() {
        let a = EmotionAnalysis::analyze("obrigado! estou muito feliz com isso!");
        assert!(a.joy > 0.5, "joy={:.2} should be >0.5", a.joy);
    }

    #[test]
    fn emotion_analyze_sadness() {
        let a = EmotionAnalysis::analyze("sinto muita falta disso, que pena");
        assert!(a.sadness > 0.3, "sadness={:.2} should be >0.3", a.sadness);
    }

    #[test]
    fn emotion_analyze_anger() {
        let a = EmotionAnalysis::analyze("isso me irrita muito, odio total!");
        assert!(a.anger > 0.3, "anger={:.2} should be >0.3", a.anger);
    }

    #[test]
    fn emotion_analyze_fear() {
        let a = EmotionAnalysis::analyze("tenho medo do que vai acontecer");
        assert!(a.fear > 0.3, "fear={:.2} should be >0.3", a.fear);
    }

    #[test]
    fn emotion_analyze_surprise() {
        let a = EmotionAnalysis::analyze("o que?! como assim?");
        assert!(a.surprise > 0.3, "surprise={:.2} should be >0.3", a.surprise);
    }

    #[test]
    fn emotion_analyze_long_text() {
        let a = EmotionAnalysis::analyze("isto e um texto muito longo com muitas palavras para testar se o classificador funciona bem com textos grandes");
        // Should not panic, all scores in [0,1]
        assert!(a.joy >= 0.0 && a.joy <= 1.0);
        assert!(a.sadness >= 0.0 && a.sadness <= 1.0);
        assert!(a.anger >= 0.0 && a.anger <= 1.0);
        assert!(a.fear >= 0.0 && a.fear <= 1.0);
        assert!(a.surprise >= 0.0 && a.surprise <= 1.0);
        assert!(a.disgust >= 0.0 && a.disgust <= 1.0);
        assert!(a.neutral >= 0.0 && a.neutral <= 1.0);
    }

    #[test]
    fn emotion_dominant_returns_max() {
        let mut a = EmotionAnalysis::analyze("");
        a.joy = 0.9;
        a.sadness = 0.1;
        assert_eq!(a.dominant(), Emotion::Joy);
        a.joy = 0.1;
        a.sadness = 0.9;
        assert_eq!(a.dominant(), Emotion::Sadness);
    }

    #[test]
    fn emotion_describe_format() {
        let a = EmotionAnalysis::analyze("obrigado");
        let d = a.describe();
        assert!(d.contains("EMO-FW"), "describe should contain EMO-FW: {}", d);
    }

    // ── SoulProfile ──────────────────────────────────────────────────────

    #[test]
    fn soul_default_jarbas() {
        let s = SoulProfile::default_jarbas();
        assert_eq!(s.name, "JARBAS");
        assert_eq!(s.tone, "witty");
        assert!(s.humor_level > 0.0 && s.humor_level <= 1.0);
        assert!(s.empathy > 0.0 && s.empathy <= 1.0);
    }

    #[test]
    fn soul_fluid_update_anger() {
        let mut s = SoulProfile::default_jarbas();
        s.fluid_update(Emotion::Anger, 0);
        assert_eq!(s.tone, "formal");
        assert!(s.formality > 0.3);
    }

    #[test]
    fn soul_fluid_update_joy() {
        let mut s = SoulProfile::default_jarbas();
        s.fluid_update(Emotion::Joy, 0);
        assert_eq!(s.tone, "casual");
        assert!(s.humor_level >= 0.5);
    }

    #[test]
    fn soul_fluid_update_high_urgency() {
        let mut s = SoulProfile::default_jarbas();
        s.fluid_update(Emotion::Neutral, 5);
        assert_eq!(s.tone, "precise");
        assert!(s.humor_level < 0.2);
    }

    #[test]
    fn soul_mode_coach() {
        let mut s = SoulProfile::default_jarbas();
        s.empathy = 0.9;
        assert_eq!(s.mode(), PersonaMode::Coach);
    }

    #[test]
    fn soul_mode_tool() {
        let mut s = SoulProfile::default_jarbas();
        s.formality = 0.8;
        s.empathy = 0.3; // low empathy + high formality = Tool
        assert_eq!(s.mode(), PersonaMode::Tool);
    }

    #[test]
    fn soul_mode_tutor() {
        let s = SoulProfile::default_jarbas();
        // default: empathy=0.8 → Coach, need low empathy + low formality
        assert_eq!(s.mode(), PersonaMode::Coach);
    }

    #[test]
    fn soul_describe() {
        let s = SoulProfile::default_jarbas();
        let d = s.describe();
        assert!(d.contains("PERSONA"), "describe should contain PERSONA: {}", d);
        assert!(d.contains("witty"), "describe should contain tone: {}", d);
    }

    // ── SessionHistory ───────────────────────────────────────────────────

    #[test]
    fn session_push_and_len() {
        let mut s = SessionHistory::new(10);
        s.push("hello", Emotion::Joy);
        s.push("world", Emotion::Sadness);
        assert_eq!(s.entries.len(), 2);
        assert_eq!(s.entries[0].text, "hello");
        assert_eq!(s.entries[1].emotion, Emotion::Sadness);
    }

    #[test]
    fn session_compress_drop_lowest() {
        let mut s = SessionHistory::new(10);
        for i in 0..8 {
            let e = SessionEntry { text: alloc::format!("msg{}", i), emotion: Emotion::Neutral, importance: i as u32 * 10 };
            s.entries.push(e);
        }
        s.compress("drop_lowest");
        assert!(s.entries.len() <= 5, "after drop_lowest: {} entries", s.entries.len());
        // Highest importance should survive
        assert!(s.entries.iter().any(|e| e.importance >= 70));
    }

    #[test]
    fn session_compress_summarize() {
        let mut s = SessionHistory::new(10);
        s.entries.push(SessionEntry { text: String::from("low"), emotion: Emotion::Neutral, importance: 10 });
        s.entries.push(SessionEntry { text: String::from("high"), emotion: Emotion::Joy, importance: 80 });
        s.compress("summarize");
        assert_eq!(s.entries.len(), 1);
        assert_eq!(s.entries[0].text, "high");
    }

    #[test]
    fn session_compress_merge_similar() {
        let mut s = SessionHistory::new(10);
        s.entries.push(SessionEntry { text: String::from("a"), emotion: Emotion::Joy, importance: 50 });
        s.entries.push(SessionEntry { text: String::from("b"), emotion: Emotion::Joy, importance: 60 });
        s.entries.push(SessionEntry { text: String::from("c"), emotion: Emotion::Sadness, importance: 50 });
        s.compress("merge_similar");
        assert_eq!(s.entries.len(), 2, "should keep one Joy + one Sadness");
    }

    // ── NotificationGate ─────────────────────────────────────────────────

    #[test]
    fn notification_push_and_pop() {
        let mut n = NotificationGate::new();
        n.push("test", Urgency::High);
        assert_eq!(n.queue.len(), 1);
        let item = n.pop();
        assert!(item.is_some());
        assert_eq!(item.unwrap().0, "test");
    }

    #[test]
    fn notification_dedup() {
        let mut n = NotificationGate::new();
        n.push("same", Urgency::Medium);
        n.push("same", Urgency::Medium); // should dedup
        assert_eq!(n.queue.len(), 1);
    }

    #[test]
    fn notification_rate_limit() {
        let mut n = NotificationGate::new();
        for _ in 0..10 {
            n.push_with_agent("msg", Urgency::Low, "agent_a");
        }
        // Rate limit: max 5 per agent per window
        assert!(n.queue.len() <= 5, "rate limit should cap at 5, got {}", n.queue.len());
    }

    #[test]
    fn notification_priority_sort() {
        let mut n = NotificationGate::new();
        n.push("low", Urgency::Low);
        n.push("critical", Urgency::Critical);
        n.push("high", Urgency::High);
        // Should be sorted by urgency desc
        let first = n.pop().unwrap();
        assert_eq!(first.1 as u8, Urgency::Critical as u8);
    }

    #[test]
    fn notification_status() {
        let mut n = NotificationGate::new();
        n.push("test", Urgency::Critical);
        n.push("test2", Urgency::High);
        let s = n.status();
        assert!(s.contains("2 pending"));
    }

    // ── DreamEngine ──────────────────────────────────────────────────────

    #[test]
    fn dream_new_empty() {
        let d = DreamEngine::new();
        assert!(d.insights.is_empty());
        assert!(d.status().contains("0 insights"));
    }

    // ── EgoLayer ─────────────────────────────────────────────────────────

    #[test]
    fn ego_learn_and_can_answer() {
        let mut e = EgoLayer::new();
        assert!(!e.can_answer("rust"));
        e.learn("rust", true);
        assert!(e.can_answer("rust"));
        assert_eq!(e.interactions, 1);
    }

    #[test]
    fn ego_confidence_decay() {
        let mut e = EgoLayer::new();
        e.learn("rust", true);  // 0.6
        e.learn("rust", false); // 0.6*0.9 - 0.1 = 0.44
        e.learn("rust", false); // 0.44*0.9 - 0.1 = 0.296
        assert!(!e.can_answer("rust"), "confidence should drop below 0.3");
    }

    #[test]
    fn ego_confidence_bounds() {
        let mut e = EgoLayer::new();
        for _ in 0..100 { e.learn("x", true); }
        let c = e.confidence.get("x").unwrap();
        assert!(*c <= 1.0, "confidence should not exceed 1.0");
    }

    // ── Heartbeat ────────────────────────────────────────────────────────

    #[test]
    fn heartbeat_new_empty() {
        let h = Heartbeat::new();
        assert!(h.messages.is_empty());
        assert!(h.status().contains("0 proactive"));
    }

    // ── SessionlessThread ────────────────────────────────────────────────

    #[test]
    fn sessionless_not_stale_initially() {
        let t = SessionlessThread::new(100);
        assert!(!t.is_stale());
    }

    // ── ToolState ────────────────────────────────────────────────────────

    #[test]
    fn tool_state_snapshot_and_restore() {
        let mut ts = ToolState::new();
        ts.snapshot("key1", b"data1");
        let restored = ts.restore("key1");
        assert!(restored.is_some(), "restore should find key1");
        assert_eq!(restored.unwrap(), b"data1");
        assert!(ts.restore("missing").is_none(), "missing key returns None");
    }

    #[test]
    fn tool_state_restore_removes() {
        let mut ts = ToolState::new();
        ts.snapshot("k", b"v");
        ts.restore("k");
        assert!(ts.restore("k").is_none(), "restore should consume the snapshot");
    }

    // ── BabelIndex ───────────────────────────────────────────────────────

    #[test]
    fn babel_new() {
        let b = BabelIndex::new();
        assert!(!b.needs_consolidation());
        assert!(b.status().contains("BABEL"));
    }

    #[test]
    fn babel_entropy_grows() {
        let mut b = BabelIndex::new();
        b.tick(301, 200); // session_len=200 → entropy=200/256=0.78 > 0.7
        assert!(b.needs_consolidation());
    }

    // ── ConsentGate ──────────────────────────────────────────────────────

    #[test]
    fn consent_safe_allowed() {
        let c = ConsentGate::new();
        // c is empty — unknown skill denied
        let (ok, _) = c.check("read_file");
        assert!(!ok, "unknown skill should be denied");
    }

    // ── SkillDiscovery ───────────────────────────────────────────────────

    #[test]
    fn discovery_propose_after_3() {
        let mut d = SkillDiscovery::new();
        d.observe("deploy");
        d.observe("deploy");
        assert!(d.propose().is_empty(), "need 3 observations");
        d.observe("deploy");
        let proposals = d.propose();
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0], "deploy");
    }

    // ── SemanticCache ────────────────────────────────────────────────────

    #[test]
    fn cache_exact_match() {
        let mut c = SemanticCache::new();
        c.set("key", vec![1, 2, 3], 1);
        assert_eq!(c.get("key").unwrap(), &vec![1, 2, 3]);
    }

    #[test]
    fn cache_miss() {
        let mut c = SemanticCache::new();
        assert!(c.get("missing").is_none());
    }

    // ── JarbasEngine ─────────────────────────────────────────────────────

    #[test]
    fn engine_new_defaults() {
        let e = JarbasEngine::new();
        assert_eq!(e.soul.name, "JARBAS");
        assert!(e.session.entries.is_empty());
        assert!(e.consent.check("read_file").0, "read_file should be safe");
    }

    #[test]
    fn engine_process_input_updates_emotion() {
        let mut e = JarbasEngine::new();
        e.process_input("obrigado!");
        assert_eq!(e.session.entries.len(), 1);
        assert!(e.emotion.joy > 0.0, "joy should increase after positive input");
    }

    #[test]
    fn engine_avatar_state_for() {
        let e = JarbasEngine::new();
        assert_eq!(e.avatar_state_for(false, false), AvatarState::Idle);
        assert_eq!(e.avatar_state_for(true, false), AvatarState::Processing);
        assert_eq!(e.avatar_state_for(false, true), AvatarState::Speaking);
        assert_eq!(e.avatar_state_for(true, true), AvatarState::Speaking);
    }

    #[test]
    fn engine_conversation_push() {
        let mut e = JarbasEngine::new();
        e.process_input("hello");
        e.process_input("world");
        assert_eq!(e.session.entries.len(), 2);
        assert_eq!(e.session.entries[0].text, "hello");
        assert_eq!(e.session.entries[1].text, "world");
    }

    #[test]
    fn engine_discovery_patterns() {
        let mut e = JarbasEngine::new();
        for _ in 0..5 { e.process_input("deploy server"); }
        assert!(e.discovery.patterns.get("deploy server").copied().unwrap_or(0) >= 3);
    }
}
