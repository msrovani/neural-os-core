//! Audio subsystem — espelho Jarbas (Sprint Sound / ADR-0045).
//! Truth runtime do bin = `neural-kernel/src/audio/*` (nao re-exportado aqui).
//! Contrato TOPIC_* + VAD/settings/wake Continuous alinhados ao monólito.

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
pub mod stt;

pub fn init_audio() {
    crate::audio::settings::init_audio_settings();
    k_nano::serial_println!("[AUDIO] Configuracoes de audio inicializadas");
}

pub const TOPIC_AUDIO_IN: &str = "AUDIO_IN";
pub const TOPIC_AUDIO_OUT: &str = "AUDIO_OUT";
pub const TOPIC_WAKEWORD: &str = "WAKEWORD";
pub const TOPIC_STT_TEXT: &str = "STT_TEXT";
pub const TOPIC_TTS_CMD: &str = "TTS_CMD";
