//! Contexto emocional — alimenta a LLM com o estado emocional do usuario.
//! A CortexAgent recebe isso como prefixo do prompt para ajustar tom/empatia.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;
use skill_registry::{Skill, McpManifest, OutputSchema};
use jarvis::jarvis::Emotion;
use jarvis::audio::voice::LAST_VOICE_EMOTION;
use jarvis::audio::vad::VAD_ENERGY;

/// Constroi o contexto emocional para injetar no prompt da LLM.
/// Formato: [Emotion: joy | Pitch: 220Hz | Energy: 4500 | Source: voice]
pub fn build_emotional_context(text_emotion: Option<Emotion>) -> String {
    let voice_emotion = LAST_VOICE_EMOTION.load(Ordering::Relaxed);
    let energy = VAD_ENERGY.load(Ordering::Relaxed);

    let voice_emo = match voice_emotion {
        0 => "joy", 1 => "sadness", 2 => "anger", 3 => "fear",
        4 => "surprise", 5 => "disgust", 6 => "neutral", 7 => "sarcasm",
        _ => "unknown",
    };

    let combined = match text_emotion {
        Some(te) => {
            let te_name = format_emotion(te);
            if te as u8 == voice_emotion || voice_emotion == 6 {
                te_name
            } else if voice_emotion != 6 {
                voice_emo // Voz sobrepoe texto se nao neutra
            } else {
                te_name
            }
        }
        None => voice_emo,
    };

    let source = if LAST_VOICE_EMOTION.load(Ordering::Relaxed) != 6 { "voice" } else { "text" };
    alloc::format!(
        "[Emotion: {} | Energy: {} | Source: {}]",
        combined, energy, source
    )
}

fn format_emotion(e: Emotion) -> &'static str {
    match e {
        Emotion::Joy => "joy", Emotion::Sadness => "sadness",
        Emotion::Anger => "anger", Emotion::Fear => "fear",
        Emotion::Surprise => "surprise", Emotion::Disgust => "disgust",
        Emotion::Neutral => "neutral", Emotion::Sarcasm => "sarcasm",
    }
}

pub struct EmotionalContextSkill;

impl Skill for EmotionalContextSkill {
    fn manifest(&self) -> McpManifest {
        McpManifest {
            name: String::from("emotional_context"),
            description: String::from("Retorna o contexto emocional atual do usuario para a LLM"),
            required_tokens: Vec::new(), preconditions: Vec::new(), context_links: Vec::new(),
            output_schema: OutputSchema::String, idempotent: true, contracts: Vec::new(),
        }
    }

    fn execute(&self, _input: &[u8]) -> Result<Vec<u8>, &'static str> {
        let ctx = build_emotional_context(None);
        Ok(ctx.into_bytes())
    }
}
