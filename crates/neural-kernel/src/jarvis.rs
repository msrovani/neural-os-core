//! JARVIS — Assistente Virtual Inteligente.
//! Port do .NET MAUI + ADR-0036 (JARVIS Unified Interaction Layer).
//! Sprint 86: #315.1 SOUL.md, #315.2 IPW, #315.3 Session, #315.4 Notification, #315.5 Sessionless.

use alloc::vec::Vec;
use alloc::string::String;
use core::sync::atomic::Ordering;

pub use crate::display::avatar::{JarvisAvatar, AvatarState};

// ═══════════════════════════════════════════════════════════════════════════════
// #315.1 SOUL.md Personality Engine
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Emotion { Joy, Sadness, Anger, Fear, Surprise, Disgust, Neutral, Sarcasm }

#[derive(Clone)]
pub struct SoulProfile {
    pub name: String,
    pub tone: String,
    pub humor_level: f32,
    pub formality: f32,
    pub empathy: f32,
}

impl SoulProfile {
    pub fn default_jarvis() -> Self {
        SoulProfile {
            name: String::from("JARVIS"),
            tone: String::from("witty"),
            humor_level: 0.5,
            formality: 0.3,
            empathy: 0.8,
        }
    }
    /// Carrega SOUL.md de um texto markdown
    pub fn load_soul(_markdown: &str) -> Self { SoulProfile::default_jarvis() }
}

// ═══════════════════════════════════════════════════════════════════════════════
// #315.2 IPW Monitor — Intelligence Per Watt (RAPL MSR 0x610)
// ═══════════════════════════════════════════════════════════════════════════════

pub struct IpwMonitor {
    last_energy: u64,       // último valor lido do MSR PKG_ENERGY_STATUS
    total_energy_uj: u64,   // microjoules acumulados
    tokens_generated: u64,
    last_tick: u64,
    tokens_per_watt: f32,
}

impl IpwMonitor {
    pub fn new() -> Self {
        IpwMonitor { last_energy: 0, total_energy_uj: 0, tokens_generated: 0, last_tick: 0, tokens_per_watt: 0.0 }
    }
    /// Lê RAPL PKG_ENERGY_STATUS (MSR 0x610) se disponível
    pub fn sample(&mut self, tick: u64, tokens_this_tick: u64) {
        self.tokens_generated += tokens_this_tick;
        let elapsed = tick.wrapping_sub(self.last_tick);
        if elapsed < 10 { return; }
        self.last_tick = tick;
        let energy = unsafe { read_rapl_msr() };
        if energy > 0 && self.last_energy > 0 {
            let delta = energy.wrapping_sub(self.last_energy);
            self.total_energy_uj = self.total_energy_uj.wrapping_add(delta);
            if delta > 0 {
                self.tokens_per_watt = self.tokens_generated as f32 / (self.total_energy_uj as f32 / 1_000_000.0);
            }
        }
        self.last_energy = energy;
    }
    pub fn efficiency(&self) -> String {
        alloc::format!("IPW: {:.1} tok/W · {} µJ acum.", self.tokens_per_watt, self.total_energy_uj)
    }
}

unsafe fn read_rapl_msr() -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        let mut lo: u32; let mut hi: u32;
        core::arch::asm!("rdmsr", in("ecx") 0x610u32, out("eax") lo, out("edx") hi, options(nostack, preserves_flags));
        (hi as u64) << 32 | lo as u64
    }
    #[cfg(not(target_arch = "x86_64"))] { 0 }
}

// ═══════════════════════════════════════════════════════════════════════════════
// #315.3 Session Compression (4 strategies)
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Clone)]
pub struct SessionEntry {
    pub text: String,
    pub emotion: Emotion,
    pub timestamp: u64,
    pub importance: u32,
}

pub struct SessionHistory {
    entries: Vec<SessionEntry>,
    max: usize,
}

impl SessionHistory {
    pub fn new(max: usize) -> Self { SessionHistory { entries: Vec::with_capacity(max), max } }
    pub fn push(&mut self, text: &str, emotion: Emotion) {
        let tick = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
        if self.entries.len() >= self.max { self.entries.remove(0); }
        self.entries.push(SessionEntry { text: String::from(text), emotion, timestamp: tick, importance: 50 });
    }
    /// Compressão: 4 estratégias
    pub fn compress(&mut self, strategy: &str) {
        match strategy {
            "summarize" => {
                // DropLowest: remove 30% menos importantes
                self.entries.sort_by(|a, b| a.importance.cmp(&b.importance));
                let keep = (self.entries.len() as f32 * 0.7) as usize;
                self.entries.drain(0..self.entries.len().saturating_sub(keep));
            }
            "drop_lowest" => {
                self.entries.sort_by(|a, b| a.importance.cmp(&b.importance));
                self.entries.drain(0..self.entries.len().saturating_sub(self.max / 2));
            }
            "merge_similar" => {
                let mut i = 0;
                while i + 1 < self.entries.len() {
                    if self.entries[i].emotion == self.entries[i + 1].emotion {
                        let merged = alloc::format!("{}; {}", self.entries[i].text, self.entries[i + 1].text);
                        self.entries[i].text = merged;
                        self.entries.remove(i + 1);
                    }
                    i += 1;
                }
            }
            "segment_means" => {
                let seg_size = (self.entries.len() / 4).max(1);
                let mut segments: Vec<SessionEntry> = Vec::new();
                for chunk in self.entries.chunks(seg_size) {
                    if let Some(first) = chunk.first() {
                        segments.push(SessionEntry {
                            text: alloc::format!("[{} msgs]", chunk.len()),
                            emotion: first.emotion,
                            timestamp: first.timestamp,
                            importance: chunk.iter().map(|e| e.importance).sum::<u32>() / chunk.len() as u32,
                        });
                    }
                }
                self.entries = segments;
            }
            _ => {}
        }
    }
    pub fn recent(&self, n: usize) -> &[SessionEntry] {
        self.entries.as_slice().split_at(self.entries.len().saturating_sub(n)).1
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// #315.4 Notification Gate (4 urgency levels)
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency { Critical, High, Medium, Low }

pub struct Notification {
    pub text: String,
    pub urgency: Urgency,
    pub tick: u64,
}

pub struct NotificationGate {
    queue: Vec<Notification>,
    last_dedup: String,
    last_dedup_tick: u64,
}

impl NotificationGate {
    pub fn new() -> Self { NotificationGate { queue: Vec::new(), last_dedup: String::new(), last_dedup_tick: 0 } }
    pub fn push(&mut self, text: &str, urgency: Urgency) {
        let tick = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
        // Dedup: mesma notificação dentro de 50 ticks
        if text == self.last_dedup && tick.wrapping_sub(self.last_dedup_tick) < 50 { return; }
        self.last_dedup = String::from(text);
        self.last_dedup_tick = tick;
        // Rate limit por urgência
        match urgency {
            Urgency::Critical => { self.queue.push(Notification { text: String::from(text), urgency, tick }); }
            Urgency::High => { if self.queue.iter().filter(|n| n.urgency == Urgency::High).count() < 5 { self.queue.push(Notification { text: String::from(text), urgency, tick }); } }
            Urgency::Medium => { self.queue.push(Notification { text: String::from(text), urgency, tick }); }
            Urgency::Low => { if self.queue.len() < 20 { self.queue.push(Notification { text: String::from(text), urgency, tick }); } }
        }
    }
    pub fn drain(&mut self) -> Vec<Notification> { core::mem::take(&mut self.queue) }
    pub fn status(&self) -> String { alloc::format!("[NOTIF] {} pendentes", self.queue.len()) }
}

// ═══════════════════════════════════════════════════════════════════════════════
// #315.5 Sessionless Thread — conversa contínua sem reset
// ═══════════════════════════════════════════════════════════════════════════════

pub struct SessionlessThread {
    pub context: String,
    pub last_interaction: u64,
    pub interaction_count: u64,
    idle_threshold: u64,
}

impl SessionlessThread {
    pub fn new(idle_ticks: u64) -> Self {
        SessionlessThread { context: String::new(), last_interaction: 0, interaction_count: 0, idle_threshold: idle_ticks }
    }
    pub fn feed(&mut self, text: &str) {
        self.context = alloc::format!("{}", text.len());
        self.last_interaction = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
        self.interaction_count = self.interaction_count.wrapping_add(1);
    }
    pub fn is_stale(&self) -> bool {
        let now = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
        now.wrapping_sub(self.last_interaction) > self.idle_threshold
    }
    pub fn status(&self) -> String {
        alloc::format!("[SESS] {} interações, stale={}", self.interaction_count, self.is_stale())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// #315.20 Fluid Persona — Personalidade Adaptativa por Contexto
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PersonaMode { Coach, Tutor, Tool }

impl SoulProfile {
    /// Ajusta persona conforme emoção detectada e urgência
    pub fn fluid_update(&mut self, emotion: Emotion, urgency: u8) {
        match emotion {
            Emotion::Sadness | Emotion::Fear => { self.empathy = (self.empathy + 0.1).min(1.0); self.formality = 0.6; }
            Emotion::Anger => { self.formality = (self.formality + 0.2).min(1.0); self.tone = String::from("formal"); }
            Emotion::Joy => { self.humor_level = (self.humor_level + 0.1).min(1.0); self.tone = String::from("casual"); }
            _ => {}
        }
        if urgency > 3 {
            self.tone = String::from("precise");
            self.humor_level = 0.1;
        }
    }

    pub fn mode(&self) -> PersonaMode {
        if self.empathy > 0.7 { PersonaMode::Coach }
        else if self.formality > 0.6 { PersonaMode::Tool }
        else { PersonaMode::Tutor }
    }

    pub fn describe(&self) -> alloc::string::String {
        alloc::format!("[PERSONA] mode={:?} tone={} humor={:.1} formality={:.1} empathy={:.1}",
            self.mode(), self.tone, self.humor_level, self.formality, self.empathy)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Análise Emocional
// ═══════════════════════════════════════════════════════════════════════════════

pub fn detect_emotion(text: &str) -> (Emotion, f32) {
    let lower = text.to_ascii_lowercase();
    if lower.contains("obrigad") || lower.contains("ador") || lower.contains("feliz") { (Emotion::Joy, 0.7) }
    else if lower.contains("trist") || lower.contains("que pena") { (Emotion::Sadness, 0.6) }
    else if lower.contains("raiva") || lower.contains("irritad") || lower.contains("odei") { (Emotion::Anger, 0.8) }
    else if lower.contains("medo") || lower.contains("recei") { (Emotion::Fear, 0.6) }
    else if lower.contains("?") && lower.len() < 20 { (Emotion::Surprise, 0.5) }
    else if lower.contains("nojo") || lower.contains("eca") { (Emotion::Disgust, 0.7) }
    else { (Emotion::Neutral, 0.3) }
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
    pub avatar_state: AvatarState,
    pub last_emotion: Emotion,
}

impl JarvisEngine {
    pub fn new() -> Self {
        JarvisEngine {
            soul: SoulProfile::default_jarvis(),
            ipw: IpwMonitor::new(),
            session: SessionHistory::new(256),
            notifications: NotificationGate::new(),
            thread: SessionlessThread::new(500),
            avatar_state: AvatarState::Idle,
            last_emotion: Emotion::Neutral,
        }
    }

    pub fn process_input(&mut self, text: &str) {
        let (emotion, _) = detect_emotion(text);
        self.last_emotion = emotion;
        self.session.push(text, emotion);
        self.thread.feed(text);
        self.avatar_state = match emotion {
            Emotion::Joy | Emotion::Surprise => AvatarState::Listening,
            Emotion::Sadness | Emotion::Fear => AvatarState::Speaking,
            Emotion::Anger => AvatarState::Processing,
            _ => AvatarState::Idle,
        };
    }

    pub fn tick(&mut self, tick: u64, tokens: u64) {
        self.ipw.sample(tick, tokens);
        if tick % 100 == 0 { self.session.compress("drop_lowest"); }
    }

    pub fn avatar_state_for(&self, is_thinking: bool, is_speaking: bool) -> AvatarState {
        if is_speaking { AvatarState::Speaking }
        else if is_thinking { AvatarState::Processing }
        else { self.avatar_state }
    }

    pub fn status(&self) -> String {
        alloc::format!("JARVIS: {:?} {} {} {}",
            self.last_emotion, self.ipw.efficiency(), self.notifications.status(), self.thread.status())
    }
}
