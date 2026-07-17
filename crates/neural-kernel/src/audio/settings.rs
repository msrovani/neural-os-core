//! Configurações compartilhadas do stack de voz (ADR-0045 / Sprint Sound).
//! Thresholds, timeouts e skills de volume/voz.

use alloc::vec::Vec;
use alloc::string::String;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use skill_registry::{Skill, McpManifest, OutputSchema};

pub static AUDIO_VOLUME: AtomicU8 = AtomicU8::new(80);
pub static VOICE_CLONE_ENABLED: AtomicBool = AtomicBool::new(false);
pub static WAKEWORD_SENSITIVITY: AtomicU8 = AtomicU8::new(5);
pub static CURRENT_VOICE: spin::Mutex<Option<String>> = spin::Mutex::new(None);

/// Janela de escuta pós-WAKEWORD em ticks do agente (~1 tick ≈ 1 frame scheduler).
pub static WAKE_LISTEN_TICKS: AtomicU32 = AtomicU32::new(800);
/// Threshold VAD base (RMS). Noise-floor adaptativo ajusta em cima disso.
pub static VAD_THRESHOLD: AtomicU32 = AtomicU32::new(300);
/// Score mínimo do MLP wakeword (0–100 → /100.0).
pub static WAKE_ML_THRESHOLD: AtomicU8 = AtomicU8::new(50);
/// Cooldown wakeword em ticks após detecção.
pub static WAKE_COOLDOWN_TICKS: AtomicU32 = AtomicU32::new(100);
/// Amostras mínimas de emoção (SER) antes de classificar.
pub static SER_MIN_SAMPLES: AtomicU32 = AtomicU32::new(1600);

pub fn init_audio_settings() {
    *CURRENT_VOICE.lock() = Some(String::from("jarvis (Piper neural-lite)"));
}

pub fn wake_listen_ticks() -> u32 {
    WAKE_LISTEN_TICKS.load(Ordering::Relaxed)
}

pub fn vad_threshold() -> f32 {
    VAD_THRESHOLD.load(Ordering::Relaxed) as f32
}

pub fn wake_ml_threshold() -> f32 {
    WAKE_ML_THRESHOLD.load(Ordering::Relaxed) as f32 / 100.0
}

pub fn wake_cooldown_ticks() -> u32 {
    WAKE_COOLDOWN_TICKS.load(Ordering::Relaxed)
}

pub fn ser_min_samples() -> usize {
    SER_MIN_SAMPLES.load(Ordering::Relaxed) as usize
}

/// Bypass do gate wake: e2e clima e boot skinny (feature weather-e2e).
pub fn wake_gate_bypassed() -> bool {
    crate::demo_flags::RUN_WEATHER_E2E_SKINNY
}

pub struct AudioGetSettingsSkill;

impl Skill for AudioGetSettingsSkill {
    fn manifest(&self) -> McpManifest {
        McpManifest {
            name: String::from("audio_get_settings"),
            description: String::from("Retorna configuracoes de audio atuais"),
            required_tokens: Vec::new(),
            preconditions: Vec::new(),
            context_links: Vec::new(),
            output_schema: OutputSchema::String,
            idempotent: true,
            contracts: Vec::new(),
        }
    }

    fn execute(&self, _input: &[u8]) -> Result<Vec<u8>, &'static str> {
        let voice = CURRENT_VOICE.lock();
        let voice_name = voice.as_deref().unwrap_or("default");
        let info = alloc::format!(
            "Volume: {}%\nVoice: {}\nWake Word: Jarvis\nSensitivity: {}\nWakeListenTicks: {}\nVadThreshold: {}\nWakeMl: {}\nVoice Clone: {}\nWakeGateBypass: {}",
            AUDIO_VOLUME.load(Ordering::Relaxed),
            voice_name,
            WAKEWORD_SENSITIVITY.load(Ordering::Relaxed),
            wake_listen_ticks(),
            vad_threshold() as u32,
            WAKE_ML_THRESHOLD.load(Ordering::Relaxed),
            if VOICE_CLONE_ENABLED.load(Ordering::Relaxed) { "on" } else { "off" },
            wake_gate_bypassed(),
        );
        Ok(info.into_bytes())
    }
}

pub struct AudioSetVolumeSkill;

impl Skill for AudioSetVolumeSkill {
    fn manifest(&self) -> McpManifest {
        McpManifest {
            name: String::from("audio_set_volume"),
            description: String::from("Define volume do audio (0-100)"),
            required_tokens: Vec::new(),
            preconditions: Vec::new(),
            context_links: Vec::new(),
            output_schema: OutputSchema::String,
            idempotent: false,
            contracts: Vec::new(),
        }
    }

    fn execute(&self, input: &[u8]) -> Result<Vec<u8>, &'static str> {
        let s = core::str::from_utf8(input).unwrap_or("80");
        let vol: u8 = s.trim().parse().unwrap_or(80).min(100);
        AUDIO_VOLUME.store(vol, Ordering::Relaxed);
        Ok(alloc::format!("Volume definido para {}%", vol).into_bytes())
    }
}

pub struct AudioToggleVoiceCloneSkill;

impl Skill for AudioToggleVoiceCloneSkill {
    fn manifest(&self) -> McpManifest {
        McpManifest {
            name: String::from("audio_toggle_voice_clone"),
            description: String::from("Ativa/desativa clonagem de voz"),
            required_tokens: Vec::new(),
            preconditions: Vec::new(),
            context_links: Vec::new(),
            output_schema: OutputSchema::String,
            idempotent: false,
            contracts: Vec::new(),
        }
    }

    fn execute(&self, _input: &[u8]) -> Result<Vec<u8>, &'static str> {
        let current = VOICE_CLONE_ENABLED.load(Ordering::Relaxed);
        VOICE_CLONE_ENABLED.store(!current, Ordering::Relaxed);
        let status = if !current { "ativada" } else { "desativada" };
        Ok(alloc::format!("Clonagem de voz {}", status).into_bytes())
    }
}
