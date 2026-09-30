//! Emotion Analysis (IDEA #315.6).
//! Classifica texto em 7 emoções usando heurística keyword-based (sem LLM pesado).
//! Alimenta o AffectVector para modular tom do JARVIS.
//!
//! AIOS na veia: emoção computacional que modula decisões de roteamento.

/// 7 emoções básicas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Emotion {
    Joy,        // alegria
    Sadness,    // tristeza
    Anger,      // raiva
    Fear,       // medo
    Surprise,   // surpresa
    Disgust,    // nojo/desgosto
    Neutral,    // neutro
    Sarcasm,    // sarcasmo
}

/// Resultado da análise emocional.
#[derive(Debug, Clone)]
pub struct EmotionResult {
    pub primary: Emotion,
    pub confidence: f32,
    /// Mapa de scores por emoção
    pub scores: [f32; 7],
    /// Valence resultante (-1.0 a 1.0)
    pub valence: f32,
    /// Arousal resultante (0.0 a 1.0)
    pub arousal: f32,
}

/// Analisador emocional baseado em keywords PT-BR + EN.
pub struct EmotionAnalyzer;

impl EmotionAnalyzer {
    /// Busca case-insensitive ASCII em buffer de bytes (sem alloc).
    fn contains_ci(hay: &[u8], needle: &[u8]) -> bool {
        if needle.is_empty() || hay.len() < needle.len() {
            return false;
        }
        hay.windows(needle.len())
            .any(|w| w.eq_ignore_ascii_case(needle))
    }

    /// Analisa o texto e retorna a emoção detectada.
    ///
    /// s429-lab: ALOC-FREE — o `text.to_lowercase()` antigo alocava String no
    /// bump heap e, sob pressão de memória (HeapAIOS pressure=1 pós-prova), o
    /// alloc devolvia NULL → deref → #PF em loop no AP (heap-fail cr2=0x28,
    /// ip em `analyze`). Heurística keyword NÃO precisa de alocação: copia
    /// até 512 bytes para buffer de stack e compara case-insensitive ASCII.
    /// Limitação honesta: keywords com acento em MAIÚSCULO no input (ex.
    /// "ÓTIMO") não casam — heurística, não pipeline canônico.
    pub fn analyze(text: &str) -> EmotionResult {
        let mut buf = [0u8; 512];
        let bytes = text.as_bytes();
        let n = bytes.len().min(512);
        buf[..n].copy_from_slice(&bytes[..n]);
        let lower = &buf[..n];
        let mut scores = [0.0f32; 7]; // Joy, Sadness, Anger, Fear, Surprise, Disgust, Neutral

        // Joy keywords
        for w in &["feliz", "alegre", "ótimo", "maravilha", "obrigado", "amei", "adoro",
                    "happy", "great", "wonderful", "thanks", "love", "amazing", "excellent"] {
            if Self::contains_ci(lower, w.as_bytes()) { scores[0] += 1.0; }
        }

        // Sadness keywords
        for w in &["triste", "chateado", "deprê", "sinto falta", "que pena",
                    "sad", "unhappy", "miss", "unfortunate", "sorry"] {
            if Self::contains_ci(lower, w.as_bytes()) { scores[1] += 1.0; }
        }

        // Anger keywords
        for w in &["raiva", "nervoso", "puto", "ódio", "inaceitável", "frustrado",
                    "angry", "mad", "hate", "unacceptable", "furious", "rage"] {
            if Self::contains_ci(lower, w.as_bytes()) { scores[2] += 1.0; }
        }

        // Fear keywords
        for w in &["medo", "assustado", "preocupado", "ansioso", "perigo",
                    "scared", "afraid", "worried", "anxious", "danger", "fear"] {
            if Self::contains_ci(lower, w.as_bytes()) { scores[3] += 1.0; }
        }

        // Surprise keywords
        for w in &["nossa", "caramba", "uau", "sério", "incrível", "não acredito",
                    "wow", "really", "amazing", "unbelievable", "surprise", "no way"] {
            if Self::contains_ci(lower, w.as_bytes()) { scores[4] += 1.0; }
        }

        // Disgust keywords
        for w in &["nojento", "repugnante", "que nojo", "horrível", "asqueroso",
                    "disgusting", "gross", "horrible", "awful", "yuck"] {
            if Self::contains_ci(lower, w.as_bytes()) { scores[5] += 1.0; }
        }

        // Neutral boost (default)
        scores[6] = 1.0;

        // Find primary emotion
        let total: f32 = scores.iter().sum();
        let (max_idx, &max_score) = scores.iter().enumerate()
            .max_by(|(_, a), (_, b)| a.total_cmp(b)).unwrap_or((6, &1.0));

        let primary = match max_idx {
            0 => Emotion::Joy, 1 => Emotion::Sadness, 2 => Emotion::Anger,
            3 => Emotion::Fear, 4 => Emotion::Surprise, 5 => Emotion::Disgust,
            _ => Emotion::Neutral,
        };

        // Compute valence and arousal from primary emotion
        let (valence, arousal) = match primary {
            Emotion::Joy => (0.8, 0.6),
            Emotion::Sadness => (-0.7, 0.2),
            Emotion::Anger => (-0.8, 0.9),
            Emotion::Fear => (-0.6, 0.8),
            Emotion::Surprise => (0.3, 0.9),
            Emotion::Disgust => (-0.5, 0.3),
            Emotion::Sarcasm => (0.2, 0.4),
            Emotion::Neutral => (0.0, 0.1),
        };

        EmotionResult {
            primary,
            confidence: if total > 0.0 { max_score / total } else { 1.0 },
            scores,
            valence,
            arousal,
        }
    }

    /// Aplica o resultado emocional a um AffectVector.
    pub fn apply_to_affect(result: &EmotionResult, affect: &mut crate::affect::AffectVector) {
        affect.valence = affect.valence * 0.7 + result.valence * 0.3;
        affect.arousal = affect.arousal * 0.7 + result.arousal * 0.3;
        if result.confidence > 0.5 {
            affect.uncertainty *= 0.8; // more confident
        }
    }
}
