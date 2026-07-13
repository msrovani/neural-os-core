//! Speech Emotion Recognition (SER) — detecta emoção na voz do usuário.
//!
//! ## Crates existentes (alternativas ao heuristico):
//! - `loqa-voice-dsp` (v0.5.0) — pitch, formants, spectral features
//! - `sensevoice` (v0.1.0) — bindings Rust SenseVoice (ASR+emoção+diarização)
//! - `sensevoice-rs` (v0.1.7) — SenseVoiceSmall Rust puro
//!
//! ## Modelos ONNX (via sherpa-onnx futuramente):
//! - huggingface.co/onnx-community/wav2vec2-emotion-recognition-ONNX
//! - huggingface.co/audeering/wav2vec2-large-robust-12-ft-emotion-msp-dim
//!
//! ## Como funciona (heurístico atual):
//! 1. Extrai acustic features: pitch (autocorrelação), energy (RMS), ZCR, centroid
//! 2. Classifica por regras heuristicas baseadas na literatura SER
//! 3. Combina com analise textual (EmotionAnalysis) via combine_emotions()
//! 4. Alimenta JarvisEngine.emotion para resposta emocional adequada

use alloc::vec::Vec;
use alloc::string::String;
use skill_registry::{Skill, McpManifest, OutputSchema};
use jarvis::jarvis::Emotion;
use libm::{sqrtf};

pub struct VoiceFeatures {
    pub pitch_hz: f32,
    pub energy_rms: f32,
    pub zcr: f32,
    pub spectral_centroid: f32,
}

pub fn extract_features(pcm: &[i16]) -> VoiceFeatures {
    if pcm.is_empty() {
        return VoiceFeatures { pitch_hz: 0.0, energy_rms: 0.0, zcr: 0.0, spectral_centroid: 0.0 };
    }
    let n = pcm.len() as f32;
    let mut energy_sum = 0.0f32;
    let mut zcr_count = 0.0f32;
    for i in 0..pcm.len() {
        let s = pcm[i] as f32;
        energy_sum += s * s;
        if i > 0 && (pcm[i] as i32).signum() != (pcm[i - 1] as i32).signum() {
            zcr_count += 1.0;
        }
    }
    let energy_rms = sqrtf(energy_sum / n);
    let zcr = zcr_count / n;
    let pitch_hz = estimate_pitch(pcm, 16000);

    let mut spec_sum = 0.0f32;
    let mut spec_weight = 0.0f32;
    for i in 1..pcm.len().min(1024) {
        let diff = (pcm[i] as f32 - pcm[i - 1] as f32).abs();
        spec_sum += diff * i as f32;
        spec_weight += diff;
    }
    let spectral_centroid = if spec_weight > 0.0 { spec_sum / spec_weight } else { 0.0 };

    VoiceFeatures { pitch_hz, energy_rms, zcr, spectral_centroid }
}

fn estimate_pitch(pcm: &[i16], sample_rate: u32) -> f32 {
    let min_lag = (sample_rate / 400).max(2) as usize;
    let max_lag = ((sample_rate / 40) as usize).min(pcm.len() / 2);
    if max_lag <= min_lag || pcm.len() < max_lag * 2 { return 0.0; }

    let mut best_corr = 0.0f32;
    let mut best_lag = 0usize;
    for lag in min_lag..max_lag {
        let mut corr = 0.0f32;
        for i in 0..(pcm.len() - lag).min(1024) {
            corr += (pcm[i] as f32) * (pcm[i + lag] as f32);
        }
        if corr > best_corr { best_corr = corr; best_lag = lag; }
    }
    if best_lag > 0 { sample_rate as f32 / best_lag as f32 } else { 0.0 }
}

pub fn classify_emotion(features: &VoiceFeatures) -> Emotion {
    let p = features.pitch_hz;
    let e = features.energy_rms;
    let z = features.zcr;
    let s = features.spectral_centroid;

    if p > 280.0 && e > 5000.0 && z > 0.15 { Emotion::Joy }
    else if p > 250.0 && e > 8000.0 && z > 0.20 { Emotion::Anger }
    else if p > 260.0 && e < 3000.0 && z > 0.18 { Emotion::Fear }
    else if p > 300.0 && s > 800.0 { Emotion::Surprise }
    else if p < 140.0 && e < 2000.0 && z < 0.08 { Emotion::Sadness }
    else if p < 160.0 && e < 1000.0 && z < 0.05 { Emotion::Disgust }
    else if p > 180.0 && e > 4000.0 && s > 600.0 && z > 0.12 { Emotion::Sarcasm }
    else { Emotion::Neutral }
}

pub fn combine_emotions(text_emotion: Emotion, voice_emotion: Emotion) -> Emotion {
    if text_emotion == voice_emotion { return text_emotion; }
    if voice_emotion == Emotion::Neutral { return text_emotion; }
    if text_emotion == Emotion::Neutral { return voice_emotion; }
    voice_emotion
}

pub struct VoiceEmotionSkill;

impl Skill for VoiceEmotionSkill {
    fn manifest(&self) -> McpManifest {
        McpManifest {
            name: String::from("voice_emotion"),
            description: String::from("Retorna emoção detectada na voz do usuario via SER"),
            required_tokens: Vec::new(), preconditions: Vec::new(), context_links: Vec::new(),
            output_schema: OutputSchema::String, idempotent: false, contracts: Vec::new(),
        }
    }

    fn execute(&self, _input: &[u8]) -> Result<Vec<u8>, &'static str> {
        let emotion = jarvis::audio::voice::LAST_VOICE_EMOTION.load(core::sync::atomic::Ordering::Relaxed);
        let name = match emotion {
            0 => "joy", 1 => "sadness", 2 => "anger", 3 => "fear",
            4 => "surprise", 5 => "disgust", 6 => "neutral", 7 => "sarcasm",
            _ => "unknown",
        };
        Ok(alloc::format!("{}", name).into_bytes())
    }
}
