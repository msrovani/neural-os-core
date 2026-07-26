//! SOUL.md Personality Engine (IDEA #315.1).
//! Define a personalidade do JARVIS: tom, valores, comportamento adaptativo.
//!
//! Formato SOUL.md:
//! ```soul
//! name: Jarvis
//! tone: coach          # coach | tutor | tool | friend
//! formality: 0.7       # 0.0 (informal) a 1.0 (formal)
//! verbosity: 0.5       # 0.0 (conciso) a 1.0 (detalhado)
//! empathy: 0.8         # 0.0 (neutro) a 1.0 (empático)
//! creativity: 0.3      # 0.0 (literal) a 1.0 (criativo)
//! values:
//!   - privacy
//!   - honesty
//!   - helpfulness
//! restrictions:
//!   - never_generate_code_without_review
//!   - never_share_identity_key
//! ```

use alloc::string::String;
use alloc::vec::Vec;
use alloc::vec;
use alloc::collections::BTreeMap;
use crate::affect::AffectVector;

/// Tom de personalidade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Coach,   // Encorajador, didático
    Tutor,   // Paciente, explicativo
    Tool,    // Direto, funcional
    Friend,  // Casual, caloroso
}

impl Tone {
    pub fn from_str(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "coach" => Tone::Coach,
            "tutor" => Tone::Tutor,
            "tool" => Tone::Tool,
            "friend" => Tone::Friend,
            _ => Tone::Coach, // default
        }
    }
}

/// Engine de personalidade carregada do SOUL.md.
pub struct SoulEngine {
    pub name: String,
    pub tone: Tone,
    pub formality: f32,
    pub verbosity: f32,
    pub empathy: f32,
    pub creativity: f32,
    pub values: Vec<String>,
    pub restrictions: Vec<String>,
    /// Raw metadata extra
    pub metadata: BTreeMap<String, String>,
}

impl SoulEngine {
    /// Cria uma personalidade padrão (Coach).
    pub fn default() -> Self {
        Self {
            name: String::from("Jarvis"),
            tone: Tone::Coach,
            formality: 0.7,
            verbosity: 0.5,
            empathy: 0.8,
            creativity: 0.3,
            values: vec![
                String::from("privacy"),
                String::from("honesty"),
                String::from("helpfulness"),
            ],
            restrictions: vec![
                String::from("never_generate_code_without_review"),
            ],
            metadata: BTreeMap::new(),
        }
    }

    /// Parse um manifesto SOUL.md.
    pub fn parse(text: &str) -> Self {
        let mut soul = SoulEngine::default();
        let mut last_key = String::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with("```") {
                continue;
            }
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim();
                let value = value.trim();
                last_key = String::from(key);
                match key {
                    "name" => soul.name = String::from(value),
                    "tone" => soul.tone = Tone::from_str(value),
                    "formality" => soul.formality = value.parse().unwrap_or(0.7),
                    "verbosity" => soul.verbosity = value.parse().unwrap_or(0.5),
                    "empathy" => soul.empathy = value.parse().unwrap_or(0.8),
                    "creativity" => soul.creativity = value.parse().unwrap_or(0.3),
                    "values" | "restrictions" => { /* handled by - prefix lines */ }
                    _ => { soul.metadata.insert(String::from(key), String::from(value)); }
                }
            } else if line.starts_with("- ") && line.len() > 2 {
                let item = &line[2..];
                if last_key == "values" {
                    soul.values.push(String::from(item));
                } else {
                    soul.restrictions.push(String::from(item));
                }
            }
        }
        soul
    }

    /// Ajusta tom baseado no estado emocional do usuário (via affect valence).
    pub fn adjust_tone(&self, affect: &AffectVector) -> Tone {
        if affect.valence < -0.5 {
            Tone::Friend  // Usuário triste → amigável
        } else if self.tone == Tone::Tool && affect.valence < 0.0 {
            Tone::Coach   // Usuário frustrado → encorajador
        } else {
            self.tone
        }
    }

    /// Modula um AffectVector com base nos parâmetros de personalidade.
    /// Permite que a personalidade influencie o estado afetivo do sistema.
    pub fn modulate_affect(&self, affect: &AffectVector) -> AffectVector {
        AffectVector {
            valence: affect.valence * (0.5 + self.empathy * 0.5),
            arousal: affect.arousal * (0.5 + self.creativity * 0.5),
            dominance: affect.dominance * (0.5 + self.formality * 0.5),
            uncertainty: affect.uncertainty * (1.0 - self.empathy * 0.5).max(0.0),
            urgency: affect.urgency,
            fatigue: affect.fatigue * (1.0 - self.empathy * 0.3).max(0.0),
            curiosity: affect.curiosity * (0.5 + self.creativity * 0.5),
            coherence: affect.coherence,
        }
    }

    /// Prefixo de resposta baseado no tom.
    pub fn response_prefix(&self, tone: Tone) -> &'static str {
        match tone {
            Tone::Coach => "💡 ",
            Tone::Tutor => "📖 ",
            Tone::Tool => "",
            Tone::Friend => "😊 ",
        }
    }
}
