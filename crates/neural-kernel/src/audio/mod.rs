//! Audio subsystem — JARVIS voice pipeline (Sprint Sound)
//!
//! ## Arquitetura Agent/Skill-First
//!
//! JarvisVoiceAgent (ouvidos + boca):
//!   - TOPIC_AUDIO_IN  ← HdaAudioAgent/USB (PCM chunks)
//!       → wake word (Rustpotter stub)
//!       → texto (sherpa-onnx STT stub)
//!       → USER_INTENT → HermesAgent (delibera) → Cortex (processa)
//!   - HERMES_RESPONSE ← HermesAgent
//!       → texto → TTS (sherpa-onnx stub)
//!       → TOPIC_AUDIO_OUT → HdaAudioAgent (PCM playback)
//!
//! HdaAudioAgent  → DriverAgent (Oneshot): init Intel HDA controller
//! UsbAudioAgent  → DriverAgent (Oneshot): init USB Audio Class
//! TtsSkill       → Skill: text→audio PCM via sherpa-onnx PocketTTS
//! SttSkill       → Skill: audio PCM→text via sherpa-onnx Whisper

pub mod frame;
pub mod ringbuf;
pub mod voice;
pub mod skills;
pub mod wakeword;
pub mod hda;
pub mod usb;

pub const TOPIC_AUDIO_IN: &str = "AUDIO_IN";
pub const TOPIC_AUDIO_OUT: &str = "AUDIO_OUT";
pub const TOPIC_WAKEWORD: &str = "WAKEWORD";
pub const TOPIC_STT_TEXT: &str = "STT_TEXT";
pub const TOPIC_TTS_CMD: &str = "TTS_CMD";
