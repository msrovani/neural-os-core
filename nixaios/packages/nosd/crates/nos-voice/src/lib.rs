//! Voice pipeline — Whisper / Supertonic stubs (EngineAbsent até flake).

use thiserror::Error;
use tracing::warn;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum VoiceError {
    #[error("engine absent: {0}")]
    EngineAbsent(&'static str),
}

pub struct VoicePipeline;

impl VoicePipeline {
    pub fn new() -> Self {
        Self
    }

    pub fn wake_word_poll(&self, _pcm: &[i16]) -> bool {
        false
    }

    pub fn transcribe(&self, _pcm: &[i16]) -> Result<String, VoiceError> {
        warn!(target: "nosd", "whisper EngineAbsent");
        Err(VoiceError::EngineAbsent("whisper-tflite"))
    }

    pub fn synthesize(&self, _text: &str) -> Result<Vec<i16>, VoiceError> {
        warn!(target: "nosd", "supertonic EngineAbsent");
        Err(VoiceError::EngineAbsent("supertonic3"))
    }
}

impl Default for VoicePipeline {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_honest() {
        let v = VoicePipeline::new();
        assert!(matches!(
            v.transcribe(&[]),
            Err(VoiceError::EngineAbsent(_))
        ));
    }
}
