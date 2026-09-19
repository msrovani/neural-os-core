//! ADR-0106 M1/M2 — sítios tipados em Hermes (emotion, skill-creation, plugin scan).
//!
//! Usa `cortex::decision` (Choice/Noul/Score). Wrappers em `cognitive_bridge`
//! preservam a API pública.

use cortex::decision::{
    AbstainReason, Decision, DecisionSource, Noul, OutcomeKind, Q8Dist, note_outcome,
};

// ── Emotion (Choice) ────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmotionKind {
    Joy,
    Anger,
    Sadness,
    Fear,
    Curious,
    Neutral,
}

impl EmotionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EmotionKind::Joy => "joy",
            EmotionKind::Anger => "anger",
            EmotionKind::Sadness => "sadness",
            EmotionKind::Fear => "fear",
            EmotionKind::Curious => "curious",
            EmotionKind::Neutral => "neutral",
        }
    }
}

const EMO_OPTS: [EmotionKind; 6] = [
    EmotionKind::Joy,
    EmotionKind::Anger,
    EmotionKind::Sadness,
    EmotionKind::Fear,
    EmotionKind::Curious,
    EmotionKind::Neutral,
];

fn hit(lower: &str, needles: &[&str]) -> u16 {
    let mut s = 0u16;
    for n in needles {
        if lower.contains(n) {
            s = s.saturating_add(4);
        }
    }
    s
}

pub fn decide_emotion(text: &str) -> Decision<EmotionKind, 6> {
    let l = text.to_ascii_lowercase();
    let mut w = [0u16; 6];
    w[0] = hit(&l, &["obrigad", "feliz", "otimo", "ótimo", "ador"]);
    w[1] = hit(&l, &["raiva", "irritad", "odei"]);
    w[2] = hit(&l, &["trist", "pena"]);
    w[3] = hit(&l, &["medo", "perigo"]);
    w[4] = if l.contains('?') { 5 } else { 0 };
    w[5] = 2; // Neutral piso / escape
    if w.iter().take(5).all(|&x| x == 0) {
        w[5] = 10;
    }
    let dist = Q8Dist::from_weights(&w);
    let (tc, tm, _) = cortex::decision::theta_for_site("emotion.hint", 100, 40);
    let mut d = Decision::from_dist(&EMO_OPTS, dist, DecisionSource::Heuristic, tc, tm);
    if d.choice.is_none() {
        // Conversacional: abster → Neutral (não HITL)
        d.choice = Some(EmotionKind::Neutral);
        d.abstain = Some(AbstainReason::LowConfidence);
        note_outcome("emotion.hint", OutcomeKind::Abstain);
    }
    d.slog_fields("emotion.hint", d.choice.unwrap().as_str());
    d
}

pub fn emotion_str(text: &str) -> &'static str {
    decide_emotion(text)
        .choice
        .unwrap_or(EmotionKind::Neutral)
        .as_str()
}

// ── Skill creation (Noul) ───────────────────────────────────────────────

pub fn noul_skill_creation(msg: &str) -> Noul {
    let lower = msg.to_ascii_lowercase();
    let mut score: i16 = 0;
    // Pares obrigatórios (AND)
    if (lower.contains("cria") || lower.contains("create")) && lower.contains("skill") {
        score += 50;
    }
    if lower.contains("novo skill") || lower.contains("new skill") {
        score += 45;
    }
    if (lower.contains("registra") || lower.contains("register")) && lower.contains("skill") {
        score += 45;
    }
    // Slash commands (contrato fraco — ainda significado de intenção)
    if lower.contains("/add_skill") || lower.contains("/learn") {
        score += 55;
    }
    // Negativos: só "show skills"
    if lower.contains("show") && lower.contains("skill") && score < 40 {
        score -= 30;
    }
    let p = if score >= 45 {
        0.92
    } else if score >= 25 {
        0.7
    } else if score <= 0 {
        0.08
    } else {
        0.45
    };
    let n = Noul::from_p_yes(p, DecisionSource::Heuristic);
    k_nano::slog_hermes!(
        "Decide",
        "ok",
        "site=skill.creation p_yes={} source=heuristic",
        n.p_yes.0
    );
    n
}

pub fn is_skill_creation(msg: &str) -> bool {
    match noul_skill_creation(msg).decide(150) {
        Some(true) => {
            note_outcome("skill.creation", OutcomeKind::AutoOk);
            true
        }
        Some(false) => false,
        None => {
            // Incerto conversacional: false (não força skill_writer)
            note_outcome("skill.creation", OutcomeKind::Abstain);
            false
        }
    }
}

// ── Plugin scan (Score → ScanVerdict) ───────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginRisk {
    Safe,
    Suspicious,
    Blocked,
}

impl PluginRisk {
    pub fn as_verdict(self) -> crate::plugin_hub::ScanVerdict {
        match self {
            PluginRisk::Safe => crate::plugin_hub::ScanVerdict::Safe,
            PluginRisk::Suspicious => crate::plugin_hub::ScanVerdict::Suspicious,
            PluginRisk::Blocked => crate::plugin_hub::ScanVerdict::Blocked,
        }
    }
}

const RISK_OPTS: [PluginRisk; 3] =
    [PluginRisk::Safe, PluginRisk::Suspicious, PluginRisk::Blocked];

/// Pontua padrões no blob WASM (texto UTF-8 quando possível).
pub fn decide_plugin_risk(wasm_bytes: &[u8]) -> (Decision<PluginRisk, 3>, alloc::vec::Vec<alloc::string::String>) {
    use alloc::string::String;
    use alloc::vec::Vec;
    let text = core::str::from_utf8(wasm_bytes).unwrap_or("");
    let mut details: Vec<String> = Vec::new();
    let mut w = [4u16, 1, 0]; // base Safe

    if text.contains("io_port") || text.contains("outb") || text.contains("inb") {
        details.push(String::from("Direct port I/O access — BLOCKED"));
        w[2] = w[2].saturating_add(20);
        w[0] = 0;
    }
    if text.contains("lgdt") || text.contains("lidt") || text.contains("write_cr3") {
        details.push(String::from("Privileged instruction access — BLOCKED"));
        w[2] = w[2].saturating_add(20);
        w[0] = 0;
    }
    if text.contains("write_phys") || text.contains("dma_alloc") || text.contains("phys_to_virt") {
        details.push(String::from("Physical memory access — SUSPICIOUS"));
        w[1] = w[1].saturating_add(12);
    }
    if wasm_bytes.len() > 100_000 {
        details.push(String::from("Plugin >100KB — SUSPICIOUS"));
        w[1] = w[1].saturating_add(8);
    }
    if text.contains("eval") || text.contains("base64_decode") {
        details.push(String::from("Dynamic code patterns — SUSPICIOUS"));
        w[1] = w[1].saturating_add(10);
    }

    let dist = Q8Dist::from_weights(&w);
    // Blocked: θ baixo (queremos detectar); Safe precisa margem
    let theta_c = if w[2] > 0 { 80 } else { 110 };
    let d = Decision::from_dist(&RISK_OPTS, dist, DecisionSource::Heuristic, theta_c, 8);
    let name = d.choice.map(|r| match r {
        PluginRisk::Safe => "safe",
        PluginRisk::Suspicious => "suspicious",
        PluginRisk::Blocked => "blocked",
    }).unwrap_or("abstain");
    d.slog_fields("plugin.scan", name);
    if d.choice.is_none() {
        note_outcome("plugin.scan", OutcomeKind::Abstain);
    }
    (d, details)
}

pub fn plugin_verdict(wasm_bytes: &[u8]) -> (crate::plugin_hub::ScanVerdict, alloc::vec::Vec<alloc::string::String>) {
    let (d, details) = decide_plugin_risk(wasm_bytes);
    let v = match d.choice {
        Some(r) => r.as_verdict(),
        None => {
            // Abster em scan → Suspicious (fail-closed parcial, não Blocked cego)
            crate::plugin_hub::ScanVerdict::Suspicious
        }
    };
    (v, details)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emotion_joy_parity() {
        assert_eq!(emotion_str("obrigado pela ajuda"), "joy");
        assert_eq!(emotion_str("estou muito feliz"), "joy");
    }

    #[test]
    fn emotion_anger_parity() {
        assert_eq!(emotion_str("isso me da raiva"), "anger");
    }

    #[test]
    fn emotion_neutral_parity() {
        assert_eq!(emotion_str("ok"), "neutral");
    }

    #[test]
    fn skill_creation_parity_true() {
        assert!(is_skill_creation("cria um skill de deploy"));
        assert!(is_skill_creation("create a skill for testing"));
        assert!(is_skill_creation("/add_skill my_tool"));
        assert!(is_skill_creation("/learn how to deploy"));
    }

    #[test]
    fn skill_creation_parity_false() {
        assert!(!is_skill_creation("hello world"));
        assert!(!is_skill_creation("deploy the server"));
        assert!(!is_skill_creation("show me the skills"));
    }

    #[test]
    fn plugin_block_io() {
        let (v, _) = plugin_verdict(b"fn outb() { io_port }");
        assert_eq!(v, crate::plugin_hub::ScanVerdict::Blocked);
    }

    #[test]
    fn plugin_safe_tiny() {
        let (v, _) = plugin_verdict(b"\0asm\x01\x00\x00\x00 hello");
        assert_eq!(v, crate::plugin_hub::ScanVerdict::Safe);
    }
}
