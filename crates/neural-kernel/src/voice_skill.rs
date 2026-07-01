//! Voice skill — Hermes fala com o usuario via MCP externo ou terminal.
//! Leve: apenas texto-para-display (sem TTS pesado, sem Whisper).

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use spin::Mutex;

/// Voz sintetizada (display ou futura integracao MCP)
pub struct VoiceOutput {
    pub text: String,
    pub profile: String,
    pub timestamp: u64,
}

static LAST_SPEECH: Mutex<Option<VoiceOutput>> = Mutex::new(None);

/// Hermes fala algo. Se MCP conectado, envia para voicebox.
/// Se nao, mostra no display como [Hermes diz: ...]
pub fn speak(text: &str, profile: &str) {
    let tick = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
    *LAST_SPEECH.lock() = Some(VoiceOutput {
        text: String::from(text),
        profile: String::from(profile),
        timestamp: tick as u64,
    });
    // Mostra no display
    let msg = alloc::format!("[Hermes diz: {}]", text);
    let _ = crate::EVENT_BUS.publish(crate::Event {
        id: tick as u64,
        topic: String::from(crate::hermes::TOPIC_HERMES_RESPONSE),
        payload: msg.into_bytes(),
        token: crate::CapabilityToken::Legacy(1),
    });
}

/// Recupera ultima fala
pub fn last_speech() -> Option<VoiceOutput> {
    LAST_SPEECH.lock().take()
}

/// Lista de vozes disponiveis (presets Kokoro-style)
pub const VOICES: &[&str] = &[
    "default", "Morgan", "Scarlett", "Echo",
    "Nova", "Fable", "Onyx", "Shimmer",
];

pub fn voice_exists(name: &str) -> bool {
    VOICES.iter().any(|v| v.eq_ignore_ascii_case(name))
}
