//! ADR-0106 D1 — `decide_intent`: score por braço (não first-match-wins).
//!
//! Heurística tipada (`source=Heuristic`) até existir head neural próprio.
//! Abstenção → `Intent::Unknown` (conversacional roteia ao LLM, não HITL).

use crate::cortex::Intent;
use crate::decision::{
    AbstainReason, Decision, DecisionSource, Q8Dist, note_outcome, OutcomeKind,
};

pub const INTENT_N: usize = 15;

pub const INTENT_OPTIONS: [Intent; INTENT_N] = [
    Intent::AudioVolume,
    Intent::Greeting,
    Intent::SystemStatus,
    Intent::Echo,
    Intent::HardwareIdentify,
    Intent::HardwareInfo,
    Intent::TrustAllow,
    Intent::TrustDeny,
    Intent::Network,
    Intent::HttpFetch,
    Intent::Help,
    Intent::Conversation,
    Intent::Usage,
    Intent::Chat,
    Intent::Unknown,
];

/// θ default intent (Q8). Conservador até reliableza, mas
/// greeting claro (~150+) deve Auto — não abster em "oi".
const THETA_CONF: u8 = 110;
const THETA_MARGIN: u8 = 12;

fn hit(lower: &str, needles: &[&str]) -> u16 {
    let mut s = 0u16;
    for n in needles {
        if lower.contains(n) {
            s = s.saturating_add(3);
        }
    }
    s
}

/// Pesos por braço + escape Unknown. Corrige sombreamento "volume"≠áudio
/// quando há contexto de disco/ssd.
pub fn score_intent_weights(text: &str) -> [u16; INTENT_N] {
    let lower = text.to_ascii_lowercase();
    let mut w = [0u16; INTENT_N];

    // 0 AudioVolume — exige áudio/controle; penaliza disco
    w[0] = hit(
        &lower,
        &[
            "mute",
            "brilho",
            "brightness",
            "alto-falante",
            "speaker",
            "volume do",
            "aumenta o volume",
            "abaixa o volume",
            "som ",
        ],
    );
    if lower.contains("volume")
        && !lower.contains("disco")
        && !lower.contains("ssd")
        && !lower.contains("hdd")
        && !lower.contains("partição")
        && !lower.contains("particao")
    {
        w[0] = w[0].saturating_add(4);
    }

    // 1 Greeting
    w[1] = hit(
        &lower,
        &[
            "hello", "hey", "ola", "olá", "oi", "bom dia", "boa tarde", "boa noite",
        ],
    );
    if w[1] > 0 {
        w[1] = w[1].saturating_add(8);
    }

    // 2 SystemStatus
    w[2] = hit(&lower, &["status", "system info", "sysinfo"]);

    // 3 Echo
    w[3] = hit(&lower, &["echo", "reverse", "repeat"]);

    // 4 HardwareIdentify
    w[4] = hit(&lower, &["identify", "identifique", "hw id"]);
    if (lower.contains("hw") || lower.contains("hardware"))
        && (lower.contains("identify") || lower.contains("identifique") || lower == "hw")
    {
        w[4] = w[4].saturating_add(5);
    }

    // 5 HardwareInfo
    w[5] = hit(&lower, &["hardware", "hw ", "pci", "gpu info"]);
    if lower.contains("hardware") || lower.contains("hw") {
        w[5] = w[5].saturating_add(2);
    }
    // identify tem precedência
    if w[4] > 0 {
        w[5] = w[5].saturating_sub(w[4]);
    }

    // 6 TrustAllow
    w[6] = hit(&lower, &["trust allow"]);

    // 7 TrustDeny
    w[7] = hit(&lower, &["trust deny"]);

    // 8 Network
    w[8] = hit(&lower, &["ping", "net diag", "netdiag", "rede ", "network"]);
    if lower.contains("ping") || (lower.contains("net") && !lower.contains("internet")) {
        w[8] = w[8].saturating_add(3);
    }

    // 9 HttpFetch
    w[9] = hit(&lower, &["fetch", "http", "baixar", "download url"]);

    // 10 Help
    w[10] = hit(&lower, &["help", "ajuda", "?"]);

    // 11 Conversation
    w[11] = hit(&lower, &["conv", "history", "historico", "histórico"]);

    // 12 Usage
    w[12] = hit(&lower, &["usage", "metrics", "métricas", "metricas"]);

    // 13 Chat — base baixa; sobe se nada mais pontua
    w[13] = 1;
    if lower.len() > 8 {
        w[13] = 2;
    }

    // 14 Unknown — piso para OOD / empate
    w[14] = 2;
    if w.iter().take(13).all(|&x| x == 0) {
        w[14] = 8;
    }

    w
}

pub type IntentDecision = Decision<Intent, INTENT_N>;

pub fn decide_intent(text: &str) -> IntentDecision {
    let (theta_c, theta_m, _trusted) =
        crate::decision::theta_for_site("intent.think", THETA_CONF, THETA_MARGIN);
    let (theta_c, theta_m) = crate::decision::pinned_theta()
        .map(|(a, r)| (a, r))
        .unwrap_or((theta_c, theta_m));

    let weights = score_intent_weights(text);
    let dist = Q8Dist::from_weights(&weights);
    let mut d = Decision::from_dist(
        &INTENT_OPTIONS,
        dist,
        DecisionSource::Heuristic,
        theta_c,
        theta_m,
    );

    // Se escolheu Unknown explicitamente com peso alto → abstenção conversacional
    if matches!(d.choice, Some(Intent::Unknown)) {
        d.choice = None;
        d.abstain = Some(AbstainReason::LowConfidence);
    }

    let name = d
        .choice
        .map(|i| i.skill_name())
        .unwrap_or(match d.abstain {
            Some(AbstainReason::Tie) => "abstain:tie",
            Some(AbstainReason::LowConfidence) => "abstain:low",
            Some(AbstainReason::NoOptions) => "abstain:none",
            Some(AbstainReason::ScorerAbsent) => "abstain:scorer",
            None => "unknown",
        });
    d.slog_fields("intent.think", name);

    if d.choice.is_none() {
        note_outcome("intent.think", OutcomeKind::Abstain);
    } else if d.confidence.0 >= 160 {
        // Weak label D4: só decisões claras
        if let Some(ex) = intent_to_expert(d.choice.unwrap()) {
            crate::decision::note_labeled_utterance(text, ex, false);
        }
    }

    d
}

/// Mapa Intent → índice Trinity (ADR-0083 ORDER). None = não rotula.
fn intent_to_expert(i: Intent) -> Option<u8> {
    match i {
        Intent::AudioVolume => Some(1), // hw_control
        Intent::HardwareIdentify | Intent::HardwareInfo => Some(2), // hw_identify
        Intent::TrustAllow | Intent::TrustDeny => Some(5), // security
        Intent::Greeting | Intent::Chat | Intent::Help | Intent::Conversation | Intent::Usage
        | Intent::Echo | Intent::SystemStatus | Intent::Network | Intent::HttpFetch => Some(0), // generator
        Intent::Unknown => None,
    }
}

/// Resolve Intent concreto: abstenção → Unknown (LLM path).
pub fn resolve_intent(d: &IntentDecision) -> Intent {
    d.choice.unwrap_or(Intent::Unknown)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greeting_picks_greeting() {
        let d = decide_intent("oi bom dia");
        assert_eq!(resolve_intent(&d), Intent::Greeting);
    }

    #[test]
    fn volume_disk_not_audio() {
        let d = decide_intent("por que o volume do disco encheu?");
        let i = resolve_intent(&d);
        assert!(
            i != Intent::AudioVolume,
            "disk volume shadowed to audio: {:?}",
            i
        );
    }

    #[test]
    fn volume_speaker_is_audio() {
        let d = decide_intent("aumenta o volume do alto-falante");
        assert_eq!(resolve_intent(&d), Intent::AudioVolume);
    }

    #[test]
    fn ood_abstains_or_unknown() {
        let d = decide_intent("xyzzy plugh 42");
        let i = resolve_intent(&d);
        assert!(
            matches!(i, Intent::Unknown | Intent::Chat),
            "got {:?}",
            i
        );
    }
}
