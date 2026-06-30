//! UserProfile — perfis de uso que otimizam alocacao de recursos, MHI,
//! scheduler e display baseado no comportamento do usuario.
//!
//! Cada perfil ajusta:
//! - ResourceWeights: CPU vs GPU vs IO priority
//! - MHI tier suggestion: quais tiers usar para cada tipo de alocacao
//! - Agent activity: quais agentes rodam em foreground
//! - Display theme: paleta de cores
//! - Power profile: performance vs balanced vs powersave

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU8, Ordering};

// ---------------------------------------------------------------------------
// Perfis disponiveis
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UserProfile {
    /// Jogos: max GPU, min latencia, VRAM优先
    Gamer,
    /// Engenharia/Arquitetura: CPU multi-core, RAM grande, IO moderado
    Engineer,
    /// Estudo: baixo consumo, CPU moderado, display limpo
    Student,
    /// Escritorio: IO medio, rede constante, multitarefa
    Office,
    /// Navegacao: rede leve, CPU baixo, GPU minimo
    Browsing,
    /// Multimedia: GPU media, IO streaming, audio priority
    Multimedia,
}

const PROFILE_COUNT: usize = 6;

pub const ALL_PROFILES: [UserProfile; PROFILE_COUNT] = [
    UserProfile::Gamer,
    UserProfile::Engineer,
    UserProfile::Student,
    UserProfile::Office,
    UserProfile::Browsing,
    UserProfile::Multimedia,
];

impl UserProfile {
    pub fn name(&self) -> &'static str {
        match self {
            UserProfile::Gamer => "Gamer",
            UserProfile::Engineer => "Engineer",
            UserProfile::Student => "Student",
            UserProfile::Office => "Office",
            UserProfile::Browsing => "Browsing",
            UserProfile::Multimedia => "Multimedia",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            UserProfile::Gamer => "🎮",
            UserProfile::Engineer => "⚙️",
            UserProfile::Student => "📚",
            UserProfile::Office => "💼",
            UserProfile::Browsing => "🌐",
            UserProfile::Multimedia => "🎬",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            UserProfile::Gamer => "Max GPU, baixa latencia, VRAM优先",
            UserProfile::Engineer => "CPU multi-core, RAM grande, compilacao rapida",
            UserProfile::Student => "Baixo consumo, foco, sem distracoes",
            UserProfile::Office => "Multitarefa, IO balanceado, rede estavel",
            UserProfile::Browsing => "Leve, eficiente, seguro",
            UserProfile::Multimedia => "Streaming, audio, GPU midia",
        }
    }

    /// Pesos relativos para escalonamento (CPU, GPU, IO)
    pub fn resource_weights(&self) -> (f32, f32, f32) {
        match self {
            UserProfile::Gamer => (0.3, 0.6, 0.1),
            UserProfile::Engineer => (0.5, 0.2, 0.3),
            UserProfile::Student => (0.4, 0.1, 0.5),
            UserProfile::Office => (0.3, 0.1, 0.6),
            UserProfile::Browsing => (0.2, 0.1, 0.7),
            UserProfile::Multimedia => (0.2, 0.4, 0.4),
        }
    }

    /// Sugestao de tier MHI para cada tipo de alocacao
    pub fn mhi_tier(&self, access_hot: bool, size_bytes: u64) -> &'static str {
        match self {
            UserProfile::Gamer => {
                if access_hot { "VRAM" }
                else if size_bytes > 1024 * 1024 { "NVMe" }
                else { "DRAM" }
            }
            UserProfile::Engineer => {
                if access_hot { "DRAM" }
                else if size_bytes > 10 * 1024 * 1024 { "NVMe" }
                else { "DRAM" }
            }
            _ => {
                if access_hot && size_bytes < 1024 * 1024 { "DRAM" }
                else { "NVMe" }
            }
        }
    }

    /// Paleta de cores para o display
    pub fn theme_colors(&self) -> ((u8, u8, u8), (u8, u8, u8), (u8, u8, u8)) {
        match self {
            UserProfile::Gamer => ((5, 5, 15), (255, 50, 50), (50, 255, 50)),
            UserProfile::Engineer => ((10, 15, 25), (80, 180, 255), (200, 200, 220)),
            UserProfile::Student => ((15, 20, 15), (100, 220, 100), (200, 220, 200)),
            UserProfile::Office => ((20, 20, 25), (100, 150, 220), (200, 200, 210)),
            UserProfile::Browsing => ((18, 18, 22), (80, 160, 220), (190, 190, 200)),
            UserProfile::Multimedia => ((10, 10, 20), (220, 80, 180), (180, 220, 100)),
        }
    }

    /// Perfil de energia
    pub fn power_profile(&self) -> &'static str {
        match self {
            UserProfile::Gamer => "performance",
            UserProfile::Engineer => "balanced",
            UserProfile::Student => "powersave",
            UserProfile::Office => "balanced",
            UserProfile::Browsing => "powersave",
            UserProfile::Multimedia => "balanced",
        }
    }
}

// ---------------------------------------------------------------------------
// ProfileManager — singleton atomico
// ---------------------------------------------------------------------------

static CURRENT_PROFILE: AtomicU8 = AtomicU8::new(1); // default: Engineer

pub struct ProfileManager;

impl ProfileManager {
    pub fn get() -> UserProfile {
        ALL_PROFILES[CURRENT_PROFILE.load(Ordering::Relaxed) as usize]
    }

    pub fn set(profile: UserProfile) {
        for (i, p) in ALL_PROFILES.iter().enumerate() {
            if *p == profile {
                CURRENT_PROFILE.store(i as u8, Ordering::Relaxed);
                crate::serial_println!("[PROFILE] Perfil alterado para: {} {}", profile.icon(), profile.name());
                return;
            }
        }
    }

    pub fn set_from_name(name: &str) {
        for p in &ALL_PROFILES {
            if p.name().eq_ignore_ascii_case(name) {
                Self::set(*p);
                return;
            }
        }
    }

    pub fn list() -> Vec<(UserProfile, &'static str)> {
        ALL_PROFILES.iter().map(|p| (*p, p.description())).collect()
    }

    pub fn detect_from_usage(skill_counts: &[(String, u64)]) -> UserProfile {
        let mut profile_scores = [0i64; PROFILE_COUNT];

        for (skill, count) in skill_counts {
            let c = *count as i64;
            match skill.as_str() {
                s if s.contains("render") || s.contains("game") || s.contains("fps") => profile_scores[0] += c * 3,
                s if s.contains("compile") || s.contains("build") || s.contains("analyze") => profile_scores[1] += c * 2,
                s if s.contains("study") || s.contains("read") || s.contains("learn") => profile_scores[2] += c,
                s if s.contains("edit") || s.contains("write") || s.contains("calc") => profile_scores[3] += c,
                s if s.contains("browse") || s.contains("search") || s.contains("web") => profile_scores[4] += c,
                s if s.contains("play") || s.contains("stream") || s.contains("audio") => profile_scores[5] += c * 2,
                _ => {}
            }
        }

        let mut best = 1; // default Engineer
        let mut best_score = 0i64;
        for (i, score) in profile_scores.iter().enumerate() {
            if *score > best_score {
                best_score = *score;
                best = i;
            }
        }
        ALL_PROFILES[best]
    }
}
