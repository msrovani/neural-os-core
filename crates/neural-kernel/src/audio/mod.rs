//! Audio subsystem — JARVIS voice pipeline (Sprint Sound)
//!
//! JarvisAgent: persona que conversa com o usuario
//!   - USER_INTENT  → processa → LLM_REQUEST → LLM_RESPONSE → HERMES_RESPONSE + TTS
//! JarvisVoiceAgent: ouvidos + boca (somente I/O de audio)
//! AudioMixerAgent: volume, mixing
//! Skills: TtsSkill, SttSkill, AudioGetSettingsSkill, AudioSetVolumeSkill

pub mod frame;
pub mod ringbuf;
pub mod vad;
pub mod tts;
pub mod ser;
pub mod context;
pub mod neural;
pub mod piper;
pub mod voice;
pub mod skills;
pub mod settings;
pub mod mixer;
pub mod jarvis;
pub mod wakeword;
pub mod hda;
pub mod usb;
pub mod pipeline;
pub mod token;
pub mod codebook;

pub fn init_audio() {
    crate::audio::settings::init_audio_settings();
    crate::serial_println!("[AUDIO] Configuracoes de audio inicializadas");
}

pub const TOPIC_AUDIO_IN: &str = "AUDIO_IN";
pub const TOPIC_AUDIO_OUT: &str = "AUDIO_OUT";
pub const TOPIC_WAKEWORD: &str = "WAKEWORD";
pub const TOPIC_STT_TEXT: &str = "STT_TEXT";
pub const TOPIC_TTS_CMD: &str = "TTS_CMD";
