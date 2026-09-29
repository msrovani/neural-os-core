//! AUDIO_HEALTH — veredito GO/NO-GO do pipeline de áudio para a IA (AIOS).
//!
//! Premissa máxima (ADR-0088): a IA decide com dados medidos, não com ausência
//! de log. Este módulo publica no EventBus (`AUDIO_HEALTH`) um snapshot com
//! veredito por subsistema (playback / captura / sessão de voz) e razões
//! legíveis por máquina — o mesmo instrumento que o dev lê no serial, agora
//! consumível por Hermes/SelfHeal/LLM.
//!
//! Formato: JSON compacto (padrão MESH_HEALTH), ex.:
//! `{"playback":"GO","capture":"NO_GO","voice":"GO","reasons":["SD1_DMA_STUCK"]}`
//!
//! Regras (puras, host-testáveis):
//! - GO só com evidência POSITIVA (contador andando). Absence of failure ≠ GO.
//! - NO_GO nunca é mascarado: playback sem LPIB e captura com LPIB stale
//!   degradam o veredito mesmo que os contadores de erro estejam zerados.
//! - UNKNOWN para subsistema não inicializado (boot ainda não chegou lá).

use core::sync::atomic::{AtomicU64, Ordering};

/// Tópico no EventBus.
pub const TOPIC_AUDIO_HEALTH: &str = "AUDIO_HEALTH";

/// Veredito por subsistema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Evidência positiva de funcionamento (contador andando).
    Go,
    /// Sem evidência de progresso, ou erro ativo.
    NoGo,
    /// Subsistema não inicializado / não observável (ainda).
    Unknown,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Verdict::Go => "GO",
            Verdict::NoGo => "NO_GO",
            Verdict::Unknown => "UNKNOWN",
        }
    }
    pub fn from_label(s: &str) -> Self {
        match s {
            "GO" => Verdict::Go,
            "NO_GO" => Verdict::NoGo,
            _ => Verdict::Unknown,
        }
    }
}

/// Thresholds de progresso (contador de samples/entradas entre publicações).
pub const PLAY_PROGRESS_MIN: u64 = 1000; // ~62 ms @16 kHz mono em 1 s
pub const CAP_PROGRESS_MIN: u64 = 100; // ~2 entradas BDL (2×4 KB) em 1 s

/// Snapshot dos contadores medidos (chamado pelo dono do tick).
#[derive(Debug, Clone, Copy, Default)]
pub struct AudioHealthInputs {
    /// HDA driver pronto (init ok)?
    pub hda_ready: bool,
    /// Codec/enumeracao com pin+DAC (playback path existe)?
    pub playback_path: bool,
    /// Codec com pin+ADC (capture path existe)?
    pub capture_path: bool,
    /// Samples publicados no SD4 desde o último snapshot (LPIB andou).
    pub play_samples_delta: u64,
    /// Entradas SD0 drenadas desde o último snapshot (LPIB andou).
    pub cap_entries_delta: u64,
    /// Amostras de playback descartadas (back-pressure) no intervalo.
    pub play_dropped_delta: u64,
    /// Há demanda de playback agora (ring com dados OU estado Speaking)?
    /// Sem demanda + sem progresso = idle (UNKNOWN), NÃO NO_GO — evitar
    /// escalada falsa ao LLM quando o sistema está ocioso (boot real 8c).
    pub playback_demand: bool,
    /// Observações LPIB de captura implausíveis no intervalo.
    pub cap_lpib_stale_delta: u64,
    /// VOICE_STATE preso em ERROR (guard de SPEAKING disparou)?
    pub voice_error: bool,
    /// Agentes de voz no ar (jarvis_voice/wakeword/mixer)?
    pub voice_agents_up: bool,
    /// Modelo STT carregado (sem isso transcrição não existe)?
    pub stt_loaded: bool,
}

/// Veredito por subsistema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioHealthVerdicts {
    pub playback: Verdict,
    pub capture: Verdict,
    pub voice: Verdict,
}

/// Razões NO_GO legíveis por máquina (estáveis — a IA aprende com elas).
pub const R_PLAYBACK_STUCK: &str = "SD_PLAY_DMA_STUCK";
pub const R_CAPTURE_STUCK: &str = "SD_CAPTURE_LPIB_STALE";
pub const R_DROPPED: &str = "PLAYBACK_DROPPED";
pub const R_NO_PATH_PLAY: &str = "PLAYBACK_PATH_ABSENT";
pub const R_NO_PATH_CAP: &str = "CAPTURE_PATH_ABSENT";
pub const R_HDA_NOT_READY: &str = "HDA_DRIVER_NOT_READY";
pub const R_VOICE_ERROR: &str = "VOICE_STATE_ERROR";
pub const R_VOICE_AGENTS_DOWN: &str = "VOICE_AGENTS_DOWN";
pub const R_STT_ABSENT: &str = "STT_MODEL_ABSENT";

/// Regra pura: contadores medidos → vereditos + razões (host-testável).
/// Retorna (vereditos, razões). Razões só descrevem degradação real.
pub fn audio_health(inputs: AudioHealthInputs) -> (AudioHealthVerdicts, alloc::vec::Vec<&'static str>) {
    let mut reasons = alloc::vec::Vec::new();

    // ── HDA driver ──────────────────────────────────────────────────────────
    if !inputs.hda_ready {
        reasons.push(R_HDA_NOT_READY);
        return (
            AudioHealthVerdicts {
                playback: Verdict::Unknown,
                capture: Verdict::Unknown,
                voice: Verdict::Unknown,
            },
            reasons,
        );
    }

    // ── Playback (SD4 output) ───────────────────────────────────────────────
    let playback = if !inputs.playback_path {
        reasons.push(R_NO_PATH_PLAY);
        Verdict::NoGo
    } else if inputs.play_samples_delta >= PLAY_PROGRESS_MIN {
        if inputs.play_dropped_delta > 0 {
            // Andou mas descartou: GO com nota (razão informativa).
            reasons.push(R_DROPPED);
        }
        Verdict::Go
    } else if !inputs.playback_demand {
        // Idle: nada enfileirado e não falando — sem evidência nesta janela.
        // (SESSION_415: idle ≠ defeito; NO_GO só com demanda sem progresso.)
        Verdict::Unknown
    } else {
        reasons.push(R_PLAYBACK_STUCK);
        Verdict::NoGo
    };

    // ── Captura (SD0 input) ─────────────────────────────────────────────────
    let capture = if !inputs.capture_path {
        reasons.push(R_NO_PATH_CAP);
        Verdict::NoGo
    } else if inputs.cap_lpib_stale_delta > 0 {
        reasons.push(R_CAPTURE_STUCK);
        Verdict::NoGo
    } else if inputs.cap_entries_delta >= CAP_PROGRESS_MIN {
        Verdict::Go
    } else {
        // Captura sem microfone ativo é estado NORMAL (open mic fechado) —
        // não é NO_GO do HW. UNKNOWN = sem evidência nesta janela.
        Verdict::Unknown
    };

    // ── Sessão de voz (software) ────────────────────────────────────────────
    let voice = if !inputs.voice_agents_up {
        reasons.push(R_VOICE_AGENTS_DOWN);
        Verdict::NoGo
    } else if inputs.voice_error {
        reasons.push(R_VOICE_ERROR);
        Verdict::NoGo
    } else if !inputs.stt_loaded {
        // Pipeline de voz vivo, mas sem STT: degraded esperado (hypervisor),
        // honestidade: é UNKNOWN para o ciclo completo, não NO_GO de software.
        reasons.push(R_STT_ABSENT);
        Verdict::Unknown
    } else {
        Verdict::Go
    };

    (AudioHealthVerdicts { playback, capture, voice }, reasons)
}

/// Serializa o snapshot (padrão MESH_HEALTH JSON compacto).
pub fn to_json(v: &AudioHealthVerdicts, reasons: &[&str]) -> alloc::string::String {
    let mut s = alloc::string::String::from("{\"playback\":\"");
    s.push_str(v.playback.label());
    s.push_str("\",\"capture\":\"");
    s.push_str(v.capture.label());
    s.push_str("\",\"voice\":\"");
    s.push_str(v.voice.label());
    s.push_str("\",\"reasons\":[");
    for (i, r) in reasons.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push('"');
        s.push_str(r);
        s.push('"');
    }
    s.push_str("]}");
    s
}

// ── Estado do publicador (dono: JarbasVoiceAgent tick) ──────────────────────
static LAST_PLAY_PUBLISHED: AtomicU64 = AtomicU64::new(0);
static LAST_CAP_DRAINED: AtomicU64 = AtomicU64::new(0);
static LAST_PLAY_DROPPED: AtomicU64 = AtomicU64::new(0);
static LAST_CAP_LPIB_STALE: AtomicU64 = AtomicU64::new(0);
static LAST_PUBLISH_TICK: AtomicU64 = AtomicU64::new(0);
/// Último JSON publicado (para HUD consumir sem re-parse do EventBus).
pub static LAST_AUDIO_HEALTH_JSON: spin::Mutex<Option<alloc::string::String>> =
    spin::Mutex::new(None);

/// Período de publicação: 1 s (100 ticks @100 Hz) — igual telemetria 1 Hz.
pub const PUBLISH_PERIOD_TICKS: u64 = 100;

/// Publica AUDIO_HEALTH no EventBus se passou o período. Chamado no tick do
/// agente de voz. Sem alocação fora do publish (1×/s).
pub fn publish_audio_health(inputs: &AudioHealthInputs, tick: u64) {
    let last = LAST_PUBLISH_TICK.load(Ordering::Relaxed);
    if last != 0 && tick.saturating_sub(last) < PUBLISH_PERIOD_TICKS {
        return;
    }
    LAST_PUBLISH_TICK.store(tick, Ordering::Relaxed);

    let play_delta = k_nano::audio::hda::PLAY_SAMPLES_WRITTEN
        .load(Ordering::Relaxed)
        .saturating_sub(LAST_PLAY_PUBLISHED.swap(
            k_nano::audio::hda::PLAY_SAMPLES_WRITTEN.load(Ordering::Relaxed),
            Ordering::Relaxed,
        ));
    let cap_delta = k_nano::audio::hda::CAP_ENTRIES_DRAINED
        .load(Ordering::Relaxed)
        .saturating_sub(LAST_CAP_DRAINED.swap(
            k_nano::audio::hda::CAP_ENTRIES_DRAINED.load(Ordering::Relaxed),
            Ordering::Relaxed,
        ));
    let drop_delta = k_nano::audio::hda::PLAY_SAMPLES_DROPPED
        .load(Ordering::Relaxed)
        .saturating_sub(LAST_PLAY_DROPPED.swap(
            k_nano::audio::hda::PLAY_SAMPLES_DROPPED.load(Ordering::Relaxed),
            Ordering::Relaxed,
        ));
    let stale_delta = k_nano::audio::hda::CAP_LPIB_STALE
        .load(Ordering::Relaxed)
        .saturating_sub(LAST_CAP_LPIB_STALE.swap(
            k_nano::audio::hda::CAP_LPIB_STALE.load(Ordering::Relaxed),
            Ordering::Relaxed,
        ));

    let mut inp = *inputs;
    inp.play_samples_delta = play_delta;
    inp.cap_entries_delta = cap_delta;
    inp.play_dropped_delta = drop_delta;
    inp.cap_lpib_stale_delta = stale_delta;

    let (verdicts, reasons) = audio_health(inp);
    let json = to_json(&verdicts, &reasons);

    let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
        id: 0,
        topic: alloc::string::String::from(TOPIC_AUDIO_HEALTH),
        payload: json.clone().into_bytes(),
        token: event_bus::CapabilityToken::Legacy(1),
    });
    *LAST_AUDIO_HEALTH_JSON.lock() = Some(json);

    // slog ok/warn (ADR-0092: dmesg mostra DEGRADAÇÃO; UNKNOWN não é degração
    // — boot real 8c/6c: capture/voice UNKNOWN eterno em hypervisor virava
    // warn eterno = ruído que esconde warn real. JSON carrega a nuance UNKNOWN.)
    let no_nogo = verdicts.playback != Verdict::NoGo
        && verdicts.voice != Verdict::NoGo
        && verdicts.capture != Verdict::NoGo;
    let sev = if no_nogo { "ok" } else { "warn" };
    k_nano::slog_jarbas!(
        "AudioHealth",
        sev,
        "AUDIO_HEALTH playback={} capture={} voice={} reasons={}",
        verdicts.playback.label(),
        verdicts.capture.label(),
        verdicts.voice.label(),
        reasons.len()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready() -> AudioHealthInputs {
        AudioHealthInputs {
            hda_ready: true,
            playback_path: true,
            capture_path: true,
            voice_agents_up: true,
            stt_loaded: true,
            ..Default::default()
        }
    }

    #[test]
    fn tudo_andando_e_go() {
        let mut inp = ready();
        inp.play_samples_delta = 10_000;
        inp.cap_entries_delta = CAP_PROGRESS_MIN;
        let (v, r) = audio_health(inp);
        assert_eq!(v.playback, Verdict::Go);
        assert_eq!(v.capture, Verdict::Go);
        assert_eq!(v.voice, Verdict::Go);
        assert!(r.is_empty());
    }

    #[test]
    fn dma_preso_e_no_go_com_razao() {
        // Este é o caso real detectado no log: LPIB congelado = 0 samples/s
        // COM demanda (ring cheio / falando) = defeito real.
        let mut inp = ready();
        inp.play_samples_delta = 0;
        inp.playback_demand = true;
        let (v, r) = audio_health(inp);
        assert_eq!(v.playback, Verdict::NoGo);
        assert!(r.contains(&R_PLAYBACK_STUCK));
    }

    #[test]
    fn playback_idle_e_unknown_nao_no_go() {
        // SESSION_415: sistema ocioso (nada tocando) não é defeito — sem a
        // correção, 30 s de idle disparam escalada falsa ao LLM (visto no
        // boot real: NO_GO em T+646 antes do 1º TTS).
        let mut inp = ready();
        inp.play_samples_delta = 0;
        inp.playback_demand = false;
        let (v, r) = audio_health(inp);
        assert_eq!(v.playback, Verdict::Unknown);
        assert!(!r.contains(&R_PLAYBACK_STUCK));
    }

    #[test]
    fn sem_evidencia_nao_e_go() {
        // Capture path ok mas nenhuma entrada drenada: UNKNOWN (sem dado),
        // nunca GO — absence of failure não é evidência de funcionamento.
        let mut inp = ready();
        inp.cap_entries_delta = 0;
        let (v, _) = audio_health(inp);
        assert_eq!(v.capture, Verdict::Unknown);
    }

    #[test]
    fn lpib_stale_e_no_go() {
        let mut inp = ready();
        inp.cap_entries_delta = CAP_PROGRESS_MIN;
        inp.cap_lpib_stale_delta = 3;
        let (v, r) = audio_health(inp);
        assert_eq!(v.capture, Verdict::NoGo);
        assert!(r.contains(&R_CAPTURE_STUCK));
    }

    #[test]
    fn stt_ausente_e_unknown_nao_no_go() {
        // Hypervisor sem STT: degradação honesta, não falha de software.
        let mut inp = ready();
        inp.stt_loaded = false;
        inp.play_samples_delta = 10_000;
        inp.cap_entries_delta = CAP_PROGRESS_MIN;
        let (v, r) = audio_health(inp);
        assert_eq!(v.voice, Verdict::Unknown);
        assert!(r.contains(&R_STT_ABSENT));
    }

    #[test]
    fn hda_nao_pronto_tudo_unknown() {
        let inp = AudioHealthInputs { hda_ready: false, ..Default::default() };
        let (v, r) = audio_health(inp);
        assert_eq!(v.playback, Verdict::Unknown);
        assert_eq!(v.capture, Verdict::Unknown);
        assert_eq!(v.voice, Verdict::Unknown);
        assert!(r.contains(&R_HDA_NOT_READY));
    }

    #[test]
    fn json_formato_mesh_health() {
        let v = AudioHealthVerdicts {
            playback: Verdict::Go,
            capture: Verdict::NoGo,
            voice: Verdict::Unknown,
        };
        let j = to_json(&v, &[R_CAPTURE_STUCK]);
        assert_eq!(
            j.as_str(),
            "{\"playback\":\"GO\",\"capture\":\"NO_GO\",\"voice\":\"UNKNOWN\",\"reasons\":[\"SD_CAPTURE_LPIB_STALE\"]}"
        );
    }
}
