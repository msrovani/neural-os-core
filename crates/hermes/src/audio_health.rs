//! AUDIO_HEALTH consumer — escala NO_GO persistente ao LLM (Hermes → LLM).
//!
//! O jarbas publica `AUDIO_HEALTH` (JSON padrão MESH_HEALTH) 1×/s com o
//! veredito por subsistema (playback/capture/voice) e razões legíveis por
//! máquina. Este agente é o CONSUMIDOR — tópico só está "feito" quando tem
//! consumidor (lição STT_UNCERTAIN, SESSION_352).
//!
//! Política de escala (lição SESSION_410 — anti-loop de feedback):
//! - `UNKNOWN`/`GO` nunca escalam (UNKNOWN ≠ falha — é estado esperado em
//!   hypervisor, ex. STT ausente).
//! - `NO_GO` só escala após janela de persistência (`NO_GO_PERSIST_TICKS`):
//!   uma única janela ruim pode ser startup/ruído; persistente é defeito.
//! - Escala **1× por incidente** (dedupe por razão): enquanto a mesma razão
//!   continua NO_GO, não re-escala. Incidente fechado só quando o subsistema
//!   volta a evidência positiva (GO) — razão "nova" reabre incidente novo.
//! - A decisão de escalar é daqui; o LLM nunca recebe o eco da própria
//!   decisão como se fosse saúde nova.

use core::sync::atomic::{AtomicU64, Ordering};

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use event_bus::{CapabilityToken, Event, Receiver};

use k_nano::EVENT_BUS;

/// Tópico publicado pelo jarbas::audio::health (literal — hermes não depende
/// de jarbas; mesmo padrão de "HEALTH_ISSUE"/"SECURITY_ALERT").
pub const TOPIC_AUDIO_HEALTH: &str = "AUDIO_HEALTH";

/// Escala via USER_INTENT (mesmo caminho do runtime_observe::ingest_health_issue).
/// HermesAgent processa USER_INTENT → LLM diagnostica.
const TOPIC_USER_INTENT: &str = crate::hermes::TOPIC_USER_INTENT;

/// Verdict/EscalationState/NO_GO_PERSIST_MIN são ÚNICOS em `k_nano::sys_health`
/// (SESSION_415) — política de escala única, compartilhada com SYS_HEALTH.
pub use k_nano::sys_health::{EscalationState, NO_GO_PERSIST_MIN, Verdict};

/// Estado de escalation por subsistema (lógica pura — host-testável).
/// STT é o pior: mute total no áudio do usuário. Playback: sem resposta falada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Subsystem {
    /// Sem resposta falada (TTS/playback preso) — impacto direto na conversa.
    Playback,
    /// Sem transcrição/entrada de voz (dados de áudio não chegam) — mute total.
    Capture,
}

impl Subsystem {
    fn label(self) -> &'static str {
        match self {
            Subsystem::Playback => "playback",
            Subsystem::Capture => "capture",
        }
    }
}

/// Verdict/razões extraídos do payload (parsing puro — host-testável).
/// Formato: `{"playback":"GO","capture":"NO_GO","voice":"UNKNOWN","reasons":[...]}`.
pub struct AudioHealthSnapshot {
    pub playback: Verdict,
    pub capture: Verdict,
}

/// (Verdict agora é o único de k_nano::sys_health — inclui from_label.)

/// Parse mínimo do JSON do jarbas (sem serde; padrão MESH_HEALTH fixo).
pub fn parse_snapshot(json: &str) -> Option<AudioHealthSnapshot> {
    let get_field = |key: &str| -> Option<Verdict> {
        // `"key":"VALUE"` — procura a chave exata com aspas.
        let needle = alloc::format!("\"{}\":\"", key);
        let start = json.find(&needle)? + needle.len();
        let rest = &json[start..];
        let end = rest.find('"')?;
        Some(Verdict::from_label(&rest[..end]))
    };
    let playback = get_field("playback")?;
    let capture = get_field("capture")?;
    Some(AudioHealthSnapshot { playback, capture })
}

/// Prompt de diagnóstico autônomo (inclui o JSON + razões — a IA decide com
/// dados medidos, premissa máxima ADR-0088).
pub fn escalation_prompt(subsystem: Subsystem, reason: &str, json: &str) -> alloc::string::String {
    alloc::format!(
        "AUDIO_HEALTH persistente: subsistema de áudio {} em NO_GO (razão: {}). \
Diagnostique a causa raiz e proponha correção. Snapshot: {}",
        subsystem.label(),
        reason,
        json
    )
}

// ── Contadores lock-free (observabilidade — HUD/diagnóstico lê sem lock) ───
pub static AUDIO_HEALTH_EVENTS_RECV: AtomicU64 = AtomicU64::new(0);
pub static AUDIO_HEALTH_ESCALATIONS: AtomicU64 = AtomicU64::new(0);

const AUDIO_MANIFEST: AgentManifest = AgentManifest {
    name: "audio_health_agent",
    kind: AgentKind::System,
    schedule: ScheduleKind::EventDriven,
    auto_start: true,
    persist: true,
};

/// Consumidor do AUDIO_HEALTH (EventDriven — só acorda com evento pendente,
/// 1 evento/s = custo ~zero no tick do scheduler).
pub struct AudioHealthAgent {
    receiver: Receiver,
    playback: EscalationState,
    capture: EscalationState,
}

impl Default for AudioHealthAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioHealthAgent {
    pub fn new() -> Self {
        AudioHealthAgent {
            receiver: EVENT_BUS.subscribe(TOPIC_AUDIO_HEALTH),
            playback: EscalationState::default(),
            capture: EscalationState::default(),
        }
    }

    /// Processa um snapshot: atualiza janelas e escala ao LLM quando due.
    /// (separado do tick para testes — publica via USER_INTENT real)
    fn handle_snapshot(&mut self, json: &str) {
        let Some(snap) = parse_snapshot(json) else {
            return;
        };

        // Razão do streak atual (a razão NO_GO de hoje; se múltiplas, a primeira).
        let playback_reason = first_nogo_reason(json);
        if let Some(reason) = self.playback.observe(snap.playback, playback_reason) {
            self.escalate(Subsystem::Playback, reason, json);
        }
        if let Some(reason) = self.capture.observe(snap.capture, playback_reason_for_cap(json)) {
            self.escalate(Subsystem::Capture, reason, json);
        }
    }

    fn escalate(&mut self, subsystem: Subsystem, reason: &'static str, json: &str) {
        AUDIO_HEALTH_ESCALATIONS.fetch_add(1, Ordering::Relaxed);
        let prompt = escalation_prompt(subsystem, reason, json);
        k_nano::slog_hermes!(
            "AudioHealth",
            "ok",
            "escalate LLM {} persist reason={} escalations={}",
            subsystem.label(),
            reason,
            AUDIO_HEALTH_ESCALATIONS.load(Ordering::Relaxed)
        );
        let _ = EVENT_BUS.publish(Event {
            id: 0,
            topic: alloc::string::String::from(TOPIC_USER_INTENT),
            payload: prompt.into_bytes(),
            token: CapabilityToken::Legacy(1),
        });
    }
}

/// Primeira razão NO_GO que afeta playback (reasons inclui playback e voice).
fn first_nogo_reason(json: &str) -> Option<&'static str> {
    reasons_of(json).into_iter().find(|r| audio_reason_affects(r, Subsystem::Playback))
}

fn playback_reason_for_cap(json: &str) -> Option<&'static str> {
    reasons_of(json).into_iter().find(|r| audio_reason_affects(r, Subsystem::Capture))
}

/// Razões estáveis do jarbas::audio::health mapeadas por subsistema.
/// Retorna 'static str para caber no EscalationState — razões desconhecidas
/// viram None (não escalam com reason fabricado).
fn audio_reason_affects(reason: &str, sub: Subsystem) -> bool {
    let playback_reasons = [
        "SD_PLAY_DMA_STUCK",
        "PLAYBACK_DROPPED",
        "PLAYBACK_PATH_ABSENT",
        "HDA_DRIVER_NOT_READY",
        "VOICE_STATE_ERROR",
        "VOICE_AGENTS_DOWN",
    ];
    let capture_reasons = [
        "SD_CAPTURE_LPIB_STALE",
        "CAPTURE_PATH_ABSENT",
        "HDA_DRIVER_NOT_READY",
        "VOICE_AGENTS_DOWN",
    ];
    match sub {
        Subsystem::Playback => playback_reasons.contains(&reason),
        Subsystem::Capture => capture_reasons.contains(&reason),
    }
}

/// Extrai a primeira razão do JSON `"reasons":["X",...]` como &'static str.
/// Razões do jarbas são estáticas (const &str) — mapeia as conhecidas;
/// desconhecidas → None (não fabricar reason que a IA aprenderia errado).
/// Razões do JSON de áudio (pub — sys_health consolida no veredito de máquina).
pub fn reasons_of(json: &str) -> alloc::vec::Vec<&'static str> {
    const KNOWN: [&str; 9] = [
        "SD_PLAY_DMA_STUCK",
        "SD_CAPTURE_LPIB_STALE",
        "PLAYBACK_DROPPED",
        "PLAYBACK_PATH_ABSENT",
        "CAPTURE_PATH_ABSENT",
        "HDA_DRIVER_NOT_READY",
        "VOICE_STATE_ERROR",
        "VOICE_AGENTS_DOWN",
        "STT_MODEL_ABSENT",
    ];
    let Some(start) = json.find("\"reasons\":[") else {
        return alloc::vec::Vec::new();
    };
    let rest = &json[start + "\"reasons\":[".len()..];
    let Some(end) = rest.find(']') else {
        return alloc::vec::Vec::new();
    };
    rest[..end]
        .split(',')
        .filter_map(|item| {
            let item = item.trim().trim_matches('"');
            KNOWN.iter().copied().find(|k| *k == item)
        })
        .collect()
}

impl Agent for AudioHealthAgent {
    fn manifest(&self) -> &AgentManifest {
        &AUDIO_MANIFEST
    }

    fn has_pending(&self) -> bool {
        self.receiver.has_pending()
    }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        while let Some(ev) = self.receiver.try_receive() {
            AUDIO_HEALTH_EVENTS_RECV.fetch_add(1, Ordering::Relaxed);
            if let Ok(text) = core::str::from_utf8(&ev.payload) {
                self.handle_snapshot(text);
            }
        }
        AgentTickResult::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON_PLAYBACK_NOGO: &str =
        "{\"playback\":\"NO_GO\",\"capture\":\"GO\",\"voice\":\"GO\",\"reasons\":[\"SD_PLAY_DMA_STUCK\"]}";
    const JSON_ALL_GO: &str =
        "{\"playback\":\"GO\",\"capture\":\"GO\",\"voice\":\"GO\",\"reasons\":[]}";
    const JSON_CAPTURE_NOGO: &str =
        "{\"playback\":\"GO\",\"capture\":\"NO_GO\",\"voice\":\"UNKNOWN\",\"reasons\":[\"SD_CAPTURE_LPIB_STALE\"]}";

    #[test]
    fn parse_json_minimo() {
        let s = parse_snapshot(JSON_PLAYBACK_NOGO).unwrap();
        assert_eq!(s.playback, Verdict::NoGo);
        assert_eq!(s.capture, Verdict::Go);

        let s = parse_snapshot(JSON_CAPTURE_NOGO).unwrap();
        assert_eq!(s.playback, Verdict::Go);
        assert_eq!(s.capture, Verdict::NoGo);
    }

    #[test]
    fn no_go_isolado_nao_escala() {
        // 1 amostra ruim não é defeito persistente (startup/ruído).
        let mut st = EscalationState::default();
        assert!(st.observe(Verdict::NoGo, Some("SD_PLAY_DMA_STUCK")).is_none());
    }

    #[test]
    fn no_go_persistente_escala_uma_vez() {
        let mut st = EscalationState::default();
        let mut fired = None;
        for i in 0..NO_GO_PERSIST_MIN {
            fired = st.observe(Verdict::NoGo, Some("SD_PLAY_DMA_STUCK"));
            if i < NO_GO_PERSIST_MIN - 1 {
                assert!(fired.is_none());
            }
        }
        assert_eq!(fired, Some("SD_PLAY_DMA_STUCK"));
        // Continua NO_GO com a mesma razão: NÃO re-escala (dedupe por incidente).
        for _ in 0..NO_GO_PERSIST_MIN * 2 {
            assert!(st.observe(Verdict::NoGo, Some("SD_PLAY_DMA_STUCK")).is_none());
        }
    }

    #[test]
    fn go_fecha_incidente_e_razao_nova_reabre() {
        let mut st = EscalationState::default();
        for _ in 0..NO_GO_PERSIST_MIN {
            st.observe(Verdict::NoGo, Some("SD_PLAY_DMA_STUCK"));
        }
        assert!(st.observe(Verdict::NoGo, Some("SD_PLAY_DMA_STUCK")).is_none()); // já escalou
        // GO fecha o incidente.
        assert!(st.observe(Verdict::Go, None).is_none());
        // Mesma razão após GO = incidente NOVO → janela cheia de novo.
        for _ in 0..NO_GO_PERSIST_MIN - 1 {
            assert!(st.observe(Verdict::NoGo, Some("SD_PLAY_DMA_STUCK")).is_none());
        }
        assert!(st.observe(Verdict::NoGo, Some("SD_PLAY_DMA_STUCK")).is_some());
    }

    #[test]
    fn unknown_nao_conta_como_falha() {
        // UNKNOWN (open-mic fechado, STT ausente) é estado esperado — sem streak.
        let mut st = EscalationState::default();
        for _ in 0..NO_GO_PERSIST_MIN * 2 {
            assert!(st.observe(Verdict::Unknown, None).is_none());
        }
    }

    #[test]
    fn troca_de_razao_reinicia_janela() {
        let mut st = EscalationState::default();
        for _ in 0..NO_GO_PERSIST_MIN - 1 {
            st.observe(Verdict::NoGo, Some("SD_PLAY_DMA_STUCK"));
        }
        // Razão nova: janela recomeça — não escala com streak da outra razão.
        assert!(st.observe(Verdict::NoGo, Some("PLAYBACK_DROPPED")).is_none());
        for _ in 0..NO_GO_PERSIST_MIN - 2 {
            assert!(st.observe(Verdict::NoGo, Some("PLAYBACK_DROPPED")).is_none());
        }
        assert_eq!(st.observe(Verdict::NoGo, Some("PLAYBACK_DROPPED")), Some("PLAYBACK_DROPPED"));
    }

    #[test]
    fn razoes_mapeadas_por_subsistema() {
        // STT_MODEL_ABSENT não afeta playback/capture (voice UNKNOWN) — não escala.
        let (mut pb, mut cap) = (EscalationState::default(), EscalationState::default());
        for _ in 0..NO_GO_PERSIST_MIN + 5 {
            let json = "{\"playback\":\"GO\",\"capture\":\"UNKNOWN\",\"voice\":\"UNKNOWN\",\"reasons\":[\"STT_MODEL_ABSENT\"]}";
            let s = parse_snapshot(json).unwrap();
            let pb_fired = pb.observe(s.playback, first_nogo_reason(json));
            let cap_fired = cap.observe(s.capture, playback_reason_for_cap(json));
            assert!(pb_fired.is_none());
            assert!(cap_fired.is_none());
        }
    }

    #[test]
    fn agente_escalou_para_user_intent_nao_repete() {
        // Integração mínima: alimenta handle_snapshot com NO_GO persistente e
        // conta publicaçőes no EVENT_BUS via USER_INTENT.
        // (host tests rodam com std; EVENT_BUS real funciona — mesmo bus do kernel)
        let mut agent = AudioHealthAgent::new();
        let before = AUDIO_HEALTH_ESCALATIONS.load(Ordering::Relaxed);
        for _ in 0..NO_GO_PERSIST_MIN {
            agent.handle_snapshot(JSON_PLAYBACK_NOGO);
        }
        let after = AUDIO_HEALTH_ESCALATIONS.load(Ordering::Relaxed);
        assert_eq!(after - before, 1);
        // Continua sem escalar (dedupe).
        for _ in 0..NO_GO_PERSIST_MIN {
            agent.handle_snapshot(JSON_PLAYBACK_NOGO);
        }
        assert_eq!(AUDIO_HEALTH_ESCALATIONS.load(Ordering::Relaxed) - before, 1);
    }
}
