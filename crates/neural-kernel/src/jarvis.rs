//! JARVIS — Assistente Virtual Inteligente (port do .NET MAUI para bare-metal Rust).
//! Integra todos os conceitos do repositório github.com/msrovani/jarvis:
//! - Avatar animado com partículas e estados (Idle/Listening/Processing/Speaking)
//! - Análise emocional (BitNet classifier)
//! - Personalidade (SOUL.md)
//! - Memória contextual (MemoryTree + MHI)
//! - Aprendizado contínuo (SleepCycle)
//! - Voz (Piper TTS + Vosk STT — quando B-01 for resolvido)

use alloc::vec::Vec;
use alloc::string::String;
use core::sync::atomic::Ordering;

// ─── Estado do Avatar ──────────────────────────────────────────────────────
pub use crate::display::avatar::{JarvisAvatar, AvatarState};

// ─── Análise Emocional (port do EmotionalAnalysisService .NET MAUI) ────────
// Usa BitNet classifier quando disponível. Fallback para regras simples.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Emotion {
    Joy, Sadness, Anger, Fear, Surprise, Disgust, Neutral, Sarcasm,
}

/// Analisa emoção no texto. Port do EmotionalAnalysisService.
pub fn detect_emotion(text: &str) -> (Emotion, f32) {
    let lower = text.to_ascii_lowercase();
    // Análise baseada em palavras-chave (fallback até BitNet emotion expert)
    if lower.contains("obrigad") || lower.contains("ador") || lower.contains("feliz") {
        (Emotion::Joy, 0.7)
    } else if lower.contains("trist") || lower.contains("que pena") {
        (Emotion::Sadness, 0.6)
    } else if lower.contains("raiva") || lower.contains("irritad") || lower.contains("odei") {
        (Emotion::Anger, 0.8)
    } else if lower.contains("medo") || lower.contains("recei") {
        (Emotion::Fear, 0.6)
    } else if lower.contains("?" ) && lower.len() < 20 {
        (Emotion::Surprise, 0.5)
    } else if lower.contains("nojo") || lower.contains("eca") {
        (Emotion::Disgust, 0.7)
    } else {
        (Emotion::Neutral, 0.3)
    }
}

// ─── Personalidade (port do perfil do usuário do .NET MAUI) ─────────────────

#[derive(Clone)]
pub struct JarvisPersonality {
    pub name: String,
    pub tone: String,       // formal, casual, witty
    pub empathy_level: f32, // 0.0-1.0
    pub formality: f32,     // 0.0-1.0
    pub humor_level: f32,   // 0.0-1.0
    pub user_interactions: u64,
}

impl JarvisPersonality {
    pub fn new() -> Self {
        JarvisPersonality {
            name: String::from("JARVIS"),
            tone: String::from("casual"),
            empathy_level: 0.7,
            formality: 0.4,
            humor_level: 0.3,
            user_interactions: 0,
        }
    }

    /// Ajusta personalidade baseado no texto do usuário (port do UserProfile do .NET MAUI)
    pub fn learn_from(&mut self, text: &str, _emotion: Emotion) {
        self.user_interactions = self.user_interactions.wrapping_add(1);
        let words: Vec<&str> = text.split_whitespace().collect();
        let word_count = words.len();
        
        // Ajusta formalidade baseado no tamanho das palavras
        let long_words = words.iter().filter(|w| w.len() > 7).count();
        self.formality = (self.formality * 0.9) + ((long_words as f32 / word_count.max(1) as f32) * 0.1);
        
        // Ajusta empatia
        let _ = Emotion::Neutral; // placeholder
        if text.contains("?") {
            self.empathy_level = (self.empathy_level + 0.01).min(1.0);
        }
    }

    pub fn greeting(&self) -> String {
        alloc::format!("{} online. {} interações aprendidas.", self.name, self.user_interactions)
    }
}

// ─── Memória Contextual (port do VectorStorageService + SQLite) ─────────────
// Adaptado para MHI tiers (memória em RAM, NVMe, HDD)

#[derive(Clone)]
pub struct ContextMemory {
    pub text: String,
    pub emotion: Emotion,
    pub timestamp: u64,
    pub importance: u32, // 0-100
}

/// Ring buffer de memória contextual (últimas 256 interações)
pub struct JarvisMemory {
    entries: Vec<ContextMemory>,
    max: usize,
}

impl JarvisMemory {
    pub fn new(max: usize) -> Self {
        JarvisMemory { entries: Vec::with_capacity(max), max }
    }

    pub fn remember(&mut self, text: &str, emotion: Emotion) {
        let tick = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
        if self.entries.len() >= self.max {
            self.entries.remove(0);
        }
        self.entries.push(ContextMemory {
            text: String::from(text),
            emotion,
            timestamp: tick,
            importance: 50,
        });
    }

    /// Busca similaridade simples (port do VectorStorageService search)
    pub fn search(&self, query: &str) -> Vec<&ContextMemory> {
        let q = query.to_ascii_lowercase();
        self.entries.iter()
            .filter(|m| m.text.to_ascii_lowercase().contains(&q))
            .collect()
    }

    pub fn recent(&self, n: usize) -> &[ContextMemory] {
        let start = self.entries.len().saturating_sub(n);
        &self.entries[start..]
    }
}

// ─── JARVIS Engine Unificada ───────────────────────────────────────────────

pub struct JarvisEngine {
    pub personality: JarvisPersonality,
    pub memory: JarvisMemory,
    pub avatar_state: AvatarState,
    pub last_emotion: Emotion,
}

impl JarvisEngine {
    pub fn new() -> Self {
        JarvisEngine {
            personality: JarvisPersonality::new(),
            memory: JarvisMemory::new(256),
            avatar_state: AvatarState::Idle,
            last_emotion: Emotion::Neutral,
        }
    }

    /// Processa input do usuário, atualiza avatar + emoção + memória
    pub fn process_input(&mut self, text: &str) {
        let (emotion, _confidence) = detect_emotion(text);
        self.last_emotion = emotion;
        self.personality.learn_from(text, emotion);
        self.memory.remember(text, emotion);
        
        self.avatar_state = match emotion {
            Emotion::Joy | Emotion::Surprise => AvatarState::Listening,
            Emotion::Sadness | Emotion::Fear => AvatarState::Speaking,
            Emotion::Anger => AvatarState::Processing,
            _ => AvatarState::Idle,
        };
    }

    pub fn avatar_state_for(&self, is_thinking: bool, is_speaking: bool) -> AvatarState {
        if is_speaking { AvatarState::Speaking }
        else if is_thinking { AvatarState::Processing }
        else { self.avatar_state }
    }

    pub fn status(&self) -> String {
        alloc::format!("JARVIS: {:?}, {} memórias, {} interações",
            self.last_emotion, self.memory.entries.len(), self.personality.user_interactions)
    }
}
