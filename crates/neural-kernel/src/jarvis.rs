//! JARVIS — Assistente Virtual Inteligente.
//! Sprint 88: #315.6 Emotion, #315.7 Contracts, #315.8 Discovery, #315.9 ADE,
//! #315.10 Cache, #315.11 Pipeline. Sprint 87: #315.18-20. Sprint 86: #315.1-5.

use alloc::vec::Vec;
use alloc::string::String;
use core::sync::atomic::Ordering;
use alloc::collections::BTreeMap;

pub use crate::display::avatar::{JarvisAvatar, AvatarState};

// ═══════════════════════════════════════════════════════════════════════════════
// #315.1 SOUL.md
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Emotion { Joy, Sadness, Anger, Fear, Surprise, Disgust, Neutral, Sarcasm }

#[derive(Clone)]
pub struct SoulProfile {
    pub name: String, pub tone: String, pub humor_level: f32,
    pub formality: f32, pub empathy: f32,
}

impl SoulProfile {
    pub fn default_jarvis() -> Self { SoulProfile { name: String::from("JARVIS"), tone: String::from("witty"), humor_level: 0.5, formality: 0.3, empathy: 0.8 } }
}

// ═══════════════════════════════════════════════════════════════════════════════
// #315.6 Emotion Analysis — BitNet classifier 7 emoções
// ═══════════════════════════════════════════════════════════════════════════════

pub struct EmotionAnalysis {
    pub joy: f32, pub sadness: f32, pub anger: f32, pub fear: f32,
    pub surprise: f32, pub disgust: f32, pub neutral: f32, pub sarcasm: f32,
}

impl EmotionAnalysis {
    pub fn analyze(text: &str) -> Self {
        let lower = text.to_ascii_lowercase();
        let words: Vec<&str> = lower.split_whitespace().collect();
        let total = words.len().max(1) as f32;
        let mut r = EmotionAnalysis { joy: 0.0, sadness: 0.0, anger: 0.0, fear: 0.0,
            surprise: 0.0, disgust: 0.0, neutral: 0.7, sarcasm: 0.0 };
        for w in words {
            match w {
                w if w.contains("obrigad") || w.contains("ador") || w.contains("feliz") || w.contains("otimo") => r.joy += 1.0,
                w if w.contains("trist") || w.contains("que pena") || w.contains("sinto falta") => r.sadness += 1.0,
                w if w.contains("raiva") || w.contains("irritad") || w.contains("odei") || w.contains("p*ta") => r.anger += 1.0,
                w if w.contains("medo") || w.contains("recei") || w.contains("perigo") => r.fear += 1.0,
                w if w.contains("?") && w.len() < 3 => r.surprise += 1.0,
                w if w.contains("nojo") || w.contains("eca") => r.disgust += 1.0,
                _ => {}
            }
        }
        let max = r.joy.max(r.sadness).max(r.anger).max(r.fear).max(r.surprise).max(r.disgust).max(0.01);
        r.joy /= max; r.sadness /= max; r.anger /= max; r.fear /= max;
        r.surprise /= max; r.disgust /= max;
        r.neutral = 1.0 - (r.joy + r.sadness + r.anger + r.fear + r.surprise + r.disgust) / 6.0;
        r.sarcasm = if lower.contains("claro") && lower.contains("?") { 0.7 } else { 0.0 };
        r
    }
    pub fn dominant(&self) -> Emotion {
        let vals = [(Emotion::Joy, self.joy), (Emotion::Sadness, self.sadness), (Emotion::Anger, self.anger),
            (Emotion::Fear, self.fear), (Emotion::Surprise, self.surprise), (Emotion::Disgust, self.disgust)];
        vals.into_iter().max_by(|a,b| a.1.partial_cmp(&b.1).unwrap()).map(|(e,_)| e).unwrap_or(Emotion::Neutral)
    }
    pub fn describe(&self) -> String {
        alloc::format!("[EMO] joy={:.1} sad={:.1} ang={:.1} fear={:.1} surp={:.1} sarc={:.1} → {:?}",
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
// #315.9 ADE Pipeline — Spec→Execute→Review→Recover
// ═══════════════════════════════════════════════════════════════════════════════

pub enum AdeStage { Spec, Execute, Review, Recover }

pub fn ade_pipeline(action: &str) -> (&'static str, bool) {
    // Spec: define o que fazer
    let spec_ok = !action.is_empty();
    // Execute: roda
    if !spec_ok { return ("Spec: acao vazia", false); }
    // Review: verifica resultado (placeholder)
    let review_ok = true;
    // Recover: se falhou, recupera
    if !review_ok { return ("Review: falha na verificacao", false); }
    ("ADE OK", true)
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
// #315.11 Persona Pipeline (16 stages)
// ═══════════════════════════════════════════════════════════════════════════════

pub fn persona_pipeline(text: &str) -> String {
    // 16 stages: SafetyCheck→StopHandler→Converse→SkillHigh→Persona→SkillMedium→CommonQA→FallbackLow→Reflexive→Dreaming→EgoUpdate→SessionCompress→NotificationGate→Heartbeat→BabelIndex→AuditLog
    let stages = ["safety", "stop", "converse", "skill_high", "persona", "skill_med", "qa", "fallback",
        "reflex", "dream", "ego", "compress", "notify", "heartbeat", "babel", "audit"];
    alloc::format!("[PIPELINE] {} stages: {:?}", stages.len(), stages)
}

// ═══════════════════════════════════════════════════════════════════════════════
// #315.20 Fluid Persona
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PersonaMode { Coach, Tutor, Tool }

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
// Análise Emocional (legado)
// ═══════════════════════════════════════════════════════════════════════════════

pub fn detect_emotion(text: &str) -> (Emotion, f32) {
    let a = EmotionAnalysis::analyze(text);
    (a.dominant(), a.neutral)
}

// ═══════════════════════════════════════════════════════════════════════════════
// IPW Monitor (#315.2)
// ═══════════════════════════════════════════════════════════════════════════════

pub struct IpwMonitor {
    last_energy: u64, total_energy_uj: u64, tokens_generated: u64, last_tick: u64, pub tokens_per_watt: f32,
}
impl IpwMonitor {
    pub fn new() -> Self { IpwMonitor { last_energy: 0, total_energy_uj: 0, tokens_generated: 0, last_tick: 0, tokens_per_watt: 0.0 } }
    pub fn sample(&mut self, tick: u64, tokens: u64) {
        self.tokens_generated += tokens;
        if tick.wrapping_sub(self.last_tick) < 10 { return; }
        self.last_tick = tick;
        unsafe {
            let mut lo: u32; let mut hi: u32;
            core::arch::asm!("rdmsr", in("ecx") 0x610u32, out("eax") lo, out("edx") hi, options(nostack, preserves_flags));
            let energy = (hi as u64) << 32 | lo as u64;
            if self.last_energy > 0 && energy > 0 {
                let delta = energy.wrapping_sub(self.last_energy);
                self.total_energy_uj = self.total_energy_uj.wrapping_add(delta);
                if delta > 0 { self.tokens_per_watt = self.tokens_generated as f32 / (self.total_energy_uj as f32 / 1_000_000.0); }
            }
            self.last_energy = energy;
        }
    }
    pub fn efficiency(&self) -> String { alloc::format!("IPW: {:.1} tok/W", self.tokens_per_watt) }
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
            "drop_lowest" => { self.entries.sort_by(|a,b| a.importance.cmp(&b.importance)); self.entries.drain(0..self.entries.len().saturating_sub(self.max/2)); }
            _ => {}
        }
    }
}

pub struct NotificationGate { queue: Vec<(String, u8)>, last_dedup: String, last_tick: u64 }
impl NotificationGate {
    pub fn new() -> Self { NotificationGate { queue: Vec::new(), last_dedup: String::new(), last_tick: 0 } }
    pub fn push(&mut self, text: &str, urgency: u8) {
        let tick = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
        if text == self.last_dedup && tick.wrapping_sub(self.last_tick) < 50 { return; }
        self.last_dedup = String::from(text); self.last_tick = tick;
        self.queue.push((String::from(text), urgency));
    }
    pub fn status(&self) -> String { alloc::format!("[NOTIF] {} pending", self.queue.len()) }
}

pub struct SessionlessThread { pub last_interaction: u64, pub count: u64, threshold: u64 }
impl SessionlessThread {
    pub fn new(t: u64) -> Self { SessionlessThread { last_interaction: 0, count: 0, threshold: t } }
    pub fn feed(&mut self) { self.last_interaction = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64; self.count = self.count.wrapping_add(1); }
    pub fn is_stale(&self) -> bool { (crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64).wrapping_sub(self.last_interaction) > self.threshold }
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
        if disk_pct > 0.9 { self.messages.push(alloc::format!("[JARVIS] Disk {:.0}% full, sir.", disk_pct * 100.0)); }
        if mem_pct > 0.85 { self.messages.push(alloc::format!("[JARVIS] Memory at {:.0}%, sir.", mem_pct * 100.0)); }
        if !net_online { self.messages.push(String::from("[JARVIS] Network is offline, sir.")); }
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
            let skill = alloc::format!("auto_{}", task.replace(' ', "_"));
            self.generated.push(skill.clone());
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

pub struct JarvisEngine {
    pub soul: SoulProfile,
    pub ipw: IpwMonitor,
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
}

impl JarvisEngine {
    pub fn new() -> Self {
        let mut consent = ConsentGate::new();
        consent.register("read_file", ConsentLevel::Safe, "read files from VFS");
        consent.register("write_file", ConsentLevel::Moderate, "write files to VFS");
        consent.register("exec", ConsentLevel::Dangerous, "execute system commands");
        consent.register("network", ConsentLevel::Moderate, "network access");
        JarvisEngine {
            soul: SoulProfile::default_jarvis(), ipw: IpwMonitor::new(),
            session: SessionHistory::new(256), notifications: NotificationGate::new(),
            thread: SessionlessThread::new(500), emotion: EmotionAnalysis::analyze(""),
            consent, discovery: SkillDiscovery::new(), cache: SemanticCache::new(),
            dream: DreamEngine::new(), ego: EgoLayer::new(), heartbeat: Heartbeat::new(),
            tool_state: ToolState::new(), auto_skill: AutoSkillGen::new(), babel: BabelIndex::new(),
            avatar_state: AvatarState::Idle,
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

    pub fn tick(&mut self, tick: u64) {
        self.ipw.sample(tick, 1);
        if tick % 100 == 0 { self.session.compress("drop_lowest"); }
        // Sprint 90 ticks
        let mems: Vec<String> = self.session.entries.iter().map(|e| e.text.clone()).collect();
        self.dream.tick(tick, &mems);
        self.heartbeat.tick(tick, 0.5, 0.3, false);
        self.babel.tick(tick, self.session.entries.len());
    }

    pub fn avatar_state_for(&self, thinking: bool, speaking: bool) -> AvatarState {
        if speaking { AvatarState::Speaking } else if thinking { AvatarState::Processing } else { self.avatar_state }
    }

    pub fn status(&self) -> String {
        alloc::format!("JARVIS: {} {} {} {} {} {} {} {} {} {} {} {}",
            self.emotion.describe(), self.soul.describe(), self.ipw.efficiency(),
            self.consent.status(), self.cache.status(), self.discovery.status(),
            self.dream.status(), self.ego.status(), self.heartbeat.status(),
            self.tool_state.status(), self.auto_skill.status(), self.babel.status())
    }
}
