//! Audio subsystem — JARVIS voice pipeline (Sprint Sound / ADR-0045)
//!
//! Mic (HDA|UAC) → AUDIO_IN → AudioInputAgent → MicFrameRing (SPSC) + VAD_TRANSITION
//! WakeWordAgent / JarbasVoiceAgent: drenam rings (não EventBus AUDIO_FRAME)
//! HermesAgent: USER_INTENT → LLM → HERMES_RESPONSE
//! JarbasAgent: HERMES_RESPONSE / INFER_TTS_PARTIAL → Piper|formant → PLAYBACK_RING
//! AudioMixerAgent: PLAYBACK_RING → HDA/UAC (pacing por free frames)
//! Skills: TtsSkill, SttSkill, AudioGetSettingsSkill, AudioSetVolumeSkill

pub mod frame;
pub mod ringbuf;
pub mod mic_ring;
pub mod vad;
pub mod tts;
pub mod ser;
pub mod context;
pub mod piper;
pub mod voice;
pub mod capture;
pub mod skills;
pub mod settings;
pub mod mixer;
pub mod jarvis;
pub mod wakeword;
pub mod usb;
pub mod token;
pub mod codebook;
pub mod stt;

pub fn init_audio() {
    crate::audio::settings::init_audio_settings();
    k_nano::slog_bin!("Audio", "ok", "Configuracoes de audio inicializadas");
}

pub const TOPIC_AUDIO_IN: &str = "AUDIO_IN";
pub const TOPIC_AUDIO_OUT: &str = "AUDIO_OUT";
pub const TOPIC_WAKEWORD: &str = "WAKEWORD";
pub const TOPIC_STT_TEXT: &str = "STT_TEXT";
/// Transcrição de baixa confiança (honesta) — Display/chat consomem.
pub const TOPIC_STT_UNCERTAIN: &str = "STT_UNCERTAIN";
pub const TOPIC_TTS_CMD: &str = "TTS_CMD";
/// Frames de 320 amostras @16 kHz mono — legado de tópico; hot path = `mic_ring`.
/// Mantido p/ docs/compat; capture **não** publica mais neste tópico (IDEA #562).
pub const TOPIC_AUDIO_FRAME: &str = "AUDIO_FRAME";
/// Transição de VAD única do sistema (payload `start|end`).
pub const TOPIC_VAD_TRANSITION: &str = "VAD_TRANSITION";
/// Estado da sessão de voz (payload = `VoiceState as u8` + rótulo).
pub const TOPIC_VOICE_STATE: &str = "VOICE_STATE";
