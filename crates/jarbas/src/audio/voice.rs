//! VoiceSessionAgent — dona da sessão de voz (WS3/WS4 / SESSION_345).
//!
//! Consome `AUDIO_FRAME` (frames fixos de 320 amostras @16 kHz mono, produzidos pelo
//! `AudioInputAgent`) e `VAD_TRANSITION` (VAD único do sistema). **Não** faz poll de
//! mic, **não** tem VAD próprio e **não** descarta amostras — as três coisas que
//! quebravam o pipeline antes (ver `capture.rs`).
//!
//! O estado é DERIVADO a cada tick e publicado só quando muda (`VOICE_STATE`), então
//! orb/HUD desenham um snapshot em vez de decidirem por conta própria.
//!
//! Barge-in é resolvido aqui (não em um agente separado): detectar fala durante
//! `Speaking` invalida a **geração de TTS** (`TTS_GENERATION`) — sem isso, o
//! `stream_tts` do `JarbasAgent` redrenava o buffer no tick seguinte e a fala voltava.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};
use event_bus::{CapabilityToken, Event, Receiver};

use crate::audio::capture::{FRAME_SAMPLES, TOPIC_AUDIO_FRAME, TOPIC_VAD_TRANSITION};
use crate::audio::ringbuf::AudioRingBuffer;
use crate::audio::ser::{classify_emotion, extract_features};
use crate::audio::settings;
use crate::audio::{TOPIC_WAKEWORD, TOPIC_VOICE_STATE};

/// Ring de playback (TTS) — mixer drena para HDA/UAC.
pub static PLAYBACK_RING: AudioRingBuffer = AudioRingBuffer::new();

/// Emoção dominante da última fala (índice de `hermes::emotion::Emotion`).
pub static LAST_VOICE_EMOTION: AtomicU8 = AtomicU8::new(EMOTION_NEUTRAL);
/// Distribuição de crença por emoção (Q8: 0–255), suavizada entre turnos.
/// Substitui a EWMA anterior, que media o *índice do enum* — joy(0) e sarcasm(7)
/// médiavam para fear(3), uma categoria que nunca foi dita.
pub static EMOTION_DIST: [AtomicU32; 8] = [
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
];

pub const EMOTION_NEUTRAL: u8 = 6;

/// Geração corrente de TTS. Incrementar invalida qualquer fala em curso (barge-in).
pub static TTS_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Estado da sessão, publicado em `VOICE_STATE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceState {
    Sleeping = 0,
    Listening = 1,
    Thinking = 2,
    Speaking = 3,
    BargeIn = 4,
    Error = 5,
}

impl VoiceState {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => VoiceState::Listening,
            2 => VoiceState::Thinking,
            3 => VoiceState::Speaking,
            4 => VoiceState::BargeIn,
            5 => VoiceState::Error,
            _ => VoiceState::Sleeping,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            VoiceState::Sleeping => "SLEEPING",
            VoiceState::Listening => "LISTENING",
            VoiceState::Thinking => "THINKING",
            VoiceState::Speaking => "SPEAKING",
            VoiceState::BargeIn => "BARGE_IN",
            VoiceState::Error => "ERROR",
        }
    }
}

static VOICE_STATE: AtomicU8 = AtomicU8::new(VoiceState::Sleeping as u8);
static BARGE_IN_COUNT: AtomicU64 = AtomicU64::new(0);

pub fn state() -> VoiceState {
    VoiceState::from_u8(VOICE_STATE.load(Ordering::Relaxed))
}

pub fn barge_in_count() -> u64 {
    BARGE_IN_COUNT.load(Ordering::Relaxed)
}

fn publish_state(next: VoiceState) {
    let prev = VOICE_STATE.swap(next as u8, Ordering::Relaxed);
    if prev == next as u8 {
        return;
    }
    let _ = k_nano::EVENT_BUS.publish(Event {
        id: 0,
        topic: String::from(TOPIC_VOICE_STATE),
        payload: alloc::format!("{} {}", next as u8, next.label()).into_bytes(),
        token: CapabilityToken::Legacy(1),
    });
    k_nano::slog_jarbas!("Jarbas", "ok", "VOICE_STATE {} -> {}", VoiceState::from_u8(prev).label(), next.label());
}

/// Invalida a fala em curso: limpa o ring E a geração de TTS.
/// Retorna true se havia algo para interromper.
pub fn request_interrupt() -> bool {
    let had = PLAYBACK_RING.available() > 0;
    PLAYBACK_RING.clear();
    TTS_GENERATION.fetch_add(1, Ordering::AcqRel);
    cortex::infer_queue::cancel_active();
    k_nano::slog_jarbas!(
        "Jarbas",
        "ok",
        "barge-in: geração de TTS invalidada (havia playback={})",
        had
    );
    BARGE_IN_COUNT.fetch_add(1, Ordering::Relaxed);
    had
}

pub fn tts_generation() -> u64 {
    TTS_GENERATION.load(Ordering::Acquire)
}

/// A fala ainda é válida? (o JarbasAgent consulta antes de cada chunk)
pub fn tts_generation_valid(gen: u64) -> bool {
    TTS_GENERATION.load(Ordering::Acquire) == gen
}

/// Abre mic só quando o playback ring está quieto (pós-greeting).
/// Evita VAD→barge-in durante SPEAKING (SESSION_352 / s361).
pub fn maybe_enable_open_mic_after_playback() {
    if settings::open_mic_enabled() {
        return;
    }
    if PLAYBACK_RING.available() > 0 {
        return;
    }
    settings::enable_open_mic();
    k_nano::slog_jarbas!("Jarbas", "ok", "open_mic ON (playback quiet)");
}

const VOICE_MANIFEST: AgentManifest = AgentManifest {
    name: "jarvis_voice",
    kind: AgentKind::Console,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

pub struct JarbasVoiceAgent {
    frame_in: Receiver,
    vad_in: Receiver,
    hermes_out: Receiver,
    wakeword_in: Receiver,
    pcm_buffer: Vec<i16>,
    listening: bool,
    emotion_samples: Vec<i16>,
    /// Ticks restantes na janela pós-wake (0 = dormindo).
    wake_window: u32,
    /// Texto do usuário pendente (aguardando resposta para parear).
    pending_user_text: Option<String>,
    /// Histórico de conversa: pares (user, assistant).
    conversation: Vec<(String, String)>,
    max_conversation: usize,
    /// Job de STT em slices (nunca dentro do tick inteiro — ver `stt::SttJob`).
    stt_busy: bool,
}

impl JarbasVoiceAgent {
    pub fn new() -> Self {
        JarbasVoiceAgent {
            frame_in: k_nano::EVENT_BUS.subscribe(TOPIC_AUDIO_FRAME),
            vad_in: k_nano::EVENT_BUS.subscribe(TOPIC_VAD_TRANSITION),
            hermes_out: k_nano::EVENT_BUS.subscribe("HERMES_RESPONSE"),
            wakeword_in: k_nano::EVENT_BUS.subscribe(TOPIC_WAKEWORD),
            pcm_buffer: Vec::new(),
            listening: false,
            emotion_samples: Vec::new(),
            wake_window: 0,
            pending_user_text: None,
            conversation: Vec::new(),
            max_conversation: 10,
            stt_busy: false,
        }
    }

    fn can_listen(&self) -> bool {
        settings::wake_gate_bypassed()
            || settings::open_mic_enabled()
            || self.wake_window > 0
            || crate::display::chat_window::MIC_ACTIVE.load(Ordering::Relaxed)
    }

    fn finish_utterance(&mut self) {
        if self.pcm_buffer.is_empty() {
            return;
        }
        k_nano::slog_jarbas!(
            "Jarbas",
            "info",
            "Fala detectada: {} amostras ({} s)",
            self.pcm_buffer.len(),
            self.pcm_buffer.len() / crate::audio::capture::VOICE_RATE as usize
        );

        // --- SER: emoção como distribuição de crença, não média de índices ---
        if self.emotion_samples.len() >= settings::ser_min_samples() {
            let features = extract_features(&self.emotion_samples);
            if features.energy_rms > 50.0 {
                let emotion = classify_emotion(&features);
                let idx = emotion as u8 as usize % 8;
                // decaimento α=0.7 nas outras, +0.3 na observada (Q8)
                for (i, slot) in EMOTION_DIST.iter().enumerate() {
                    let cur = slot.load(Ordering::Relaxed) as f32;
                    let next = if i == idx {
                        cur * 0.7 + 255.0 * 0.3
                    } else {
                        cur * 0.7
                    };
                    slot.store(next as u32, Ordering::Relaxed);
                }
                // Dominante = argmax da distribuição suavizada (não o índice cru).
                let mut best = 0usize;
                let mut best_v = 0u32;
                for (i, slot) in EMOTION_DIST.iter().enumerate() {
                    let v = slot.load(Ordering::Relaxed);
                    if v > best_v {
                        best_v = v;
                        best = i;
                    }
                }
                LAST_VOICE_EMOTION.store(best as u8, Ordering::Relaxed);

                let valence = match emotion {
                    hermes::emotion::Emotion::Joy => 0.8,
                    hermes::emotion::Emotion::Sadness => -0.7,
                    hermes::emotion::Emotion::Anger => -0.8,
                    hermes::emotion::Emotion::Fear => -0.6,
                    hermes::emotion::Emotion::Surprise => 0.3,
                    hermes::emotion::Emotion::Disgust => -0.5,
                    hermes::emotion::Emotion::Sarcasm => 0.2,
                    _ => 0.0,
                };
                let _ = k_nano::EVENT_BUS.publish(Event {
                    id: 0,
                    topic: String::from("VOICE_EMOTION"),
                    payload: alloc::format!(
                        "valence={:.2} emotion={:?} confidence={:.2}",
                        valence,
                        emotion,
                        features.confidence
                    )
                    .into_bytes(),
                    token: CapabilityToken::Legacy(1),
                });
                k_nano::slog_jarbas!(
                    "Jarbas",
                    "info",
                    "Emocao: {:?} (pitch={:.0}Hz, energy={:.0}, conf={:.2})",
                    emotion,
                    features.pitch_hz,
                    features.energy_rms,
                    features.confidence
                );
            }
        }

        // --- STT em SLICES: o job roda nos ticks seguintes, não neste ---
        if crate::audio::stt::begin_job(&self.pcm_buffer) {
            self.stt_busy = true;
        } else {
            k_nano::slog_jarbas!(
                "Jarbas",
                "warn",
                "STT indisponível ({} amostras) — sem USER_INTENT",
                self.pcm_buffer.len()
            );
            let _ = k_nano::EVENT_BUS.publish(Event {
                id: 0,
                topic: String::from(crate::audio::TOPIC_STT_TEXT),
                payload: b"[stt-offline]".to_vec(),
                token: CapabilityToken::Legacy(1),
            });
        }

        self.listening = false;
        self.pcm_buffer.clear();
    }

    fn deliver_transcript(&mut self, text: String) {
        if text.is_empty() {
            k_nano::slog_jarbas!("Jarbas", "info", "STT vazio — sem USER_INTENT");
            let _ = k_nano::EVENT_BUS.publish(Event {
                id: 0,
                topic: String::from(crate::audio::TOPIC_STT_TEXT),
                payload: b"[vazio]".to_vec(),
                token: CapabilityToken::Legacy(1),
            });
            return;
        }
        k_nano::slog_jarbas!("Jarbas", "info", "STT: \"{}\"", text);
        let original = text.clone();
        self.pending_user_text = Some(original.clone());

        // Continuidade de conversa: prepend do contexto recente.
        let enhanced = if !self.conversation.is_empty() {
            let mut ctx = String::new();
            let start = self.conversation.len().saturating_sub(3);
            for (u, a) in &self.conversation[start..] {
                ctx.push_str(&alloc::format!("User: {}\nAssistant: {}\n", u, a));
            }
            alloc::format!("{}\nUser: {}", ctx, text)
        } else {
            text
        };

        let _ = k_nano::EVENT_BUS.publish(Event {
            id: 0,
            topic: String::from(crate::audio::TOPIC_STT_TEXT),
            payload: original.into_bytes(),
            token: CapabilityToken::Legacy(1),
        });
        let _ = k_nano::EVENT_BUS.publish(Event {
            id: 0,
            topic: String::from("USER_INTENT"),
            payload: enhanced.into_bytes(),
            token: CapabilityToken::Legacy(1),
        });
        if !settings::wake_gate_bypassed() {
            self.wake_window = self.wake_window.min(120);
        }
    }

    /// Estado derivado — uma autoridade só, publicada quando muda.
    fn derive_state(&self) -> VoiceState {
        if PLAYBACK_RING.available() > 0 {
            return VoiceState::Speaking;
        }
        if self.listening {
            return VoiceState::Listening;
        }
        if self.stt_busy || self.pending_user_text.is_some() {
            return VoiceState::Thinking;
        }
        VoiceState::Sleeping
    }
}

impl Agent for JarbasVoiceAgent {
    fn manifest(&self) -> &AgentManifest {
        &VOICE_MANIFEST
    }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        // Mic é do AudioInputAgent — aqui não há poll, VAD nem ring de captura.

        if self.wake_window > 0 && self.pending_user_text.is_none() && !crate::display::chat_window::MIC_ACTIVE.load(Ordering::Relaxed) {
            self.wake_window -= 1;
            if self.wake_window == 0 {
                k_nano::slog_jarbas!("Jarbas", "info", "wake window expirada — dormindo");
                self.listening = false;
                self.pcm_buffer.clear();
            }
        }

        while let Some(ev) = self.wakeword_in.try_receive() {
            let kw = core::str::from_utf8(&ev.payload).unwrap_or("?");
            if self.wake_window == 0 {
                self.wake_window = settings::wake_listen_ticks();
                k_nano::slog_jarbas!(
                    "Jarbas",
                    "info",
                    "wake word \"{}\" — janela {} ticks",
                    kw,
                    self.wake_window
                );
            }
        }

        // --- STT em slices: avança o job pendente com orçamento por tick ---
        if self.stt_busy {
            if let Some(text) = crate::audio::stt::step_job(SLICE_BUDGET_FRAMES) {
                self.stt_busy = false;
                self.deliver_transcript(text);
            }
        }

        // --- VAD único (vindo do dono do mic) ---
        // Budget: EventBus sem teto + AUDIO_FRAME @50Hz = tick infinito (SESSION_352).
        let mut vad_n = 0u32;
        while vad_n < 8 {
            let Some(ev) = self.vad_in.try_receive() else { break; };
            vad_n += 1;
            let tag = core::str::from_utf8(&ev.payload).unwrap_or("");
            if !self.can_listen() {
                continue;
            }
            if tag.starts_with("start") {
                // SESSION_352 / s361: sem AEC, VAD durante playback = eco do
                // greeting → barge-in cancela TTS+infer e congela o UI.
                // Só interrompe se já estávamos em Listening (turno real).
                if PLAYBACK_RING.available() > 0 {
                    if self.listening {
                        publish_state(VoiceState::BargeIn);
                        request_interrupt();
                        settings::force_wake_open();
                    } else {
                        k_nano::slog_jarbas!(
                            "Jarbas",
                            "ok",
                            "VAD start ignorado durante SPEAKING (sem AEC)"
                        );
                        continue;
                    }
                }
                self.listening = true;
                self.pcm_buffer.clear();
                self.emotion_samples.clear();
                k_nano::slog_jarbas!("Jarbas", "ok", "Escutando...");
            } else if tag.starts_with("end") {
                self.finish_utterance();
            }
        }

        // --- Frames fixos de 16 kHz mono ---
        let mut frame_n = 0u32;
        while frame_n < 16 {
            let Some(ev) = self.frame_in.try_receive() else { break; };
            frame_n += 1;
            if ev.payload.len() < FRAME_SAMPLES * 2 {
                continue;
            }
            if !self.can_listen() || !self.listening {
                continue;
            }
            let pcm: &[i16] = unsafe {
                core::slice::from_raw_parts(ev.payload.as_ptr() as *const i16, FRAME_SAMPLES)
            };
            self.pcm_buffer.extend_from_slice(pcm);
            if self.emotion_samples.len() < 16000 {
                self.emotion_samples.extend_from_slice(pcm);
            }
        }

        // --- Pareamento de turno: HERMES_RESPONSE fecha (user, assistant) ---
        let mut hermes_n = 0u32;
        while hermes_n < 4 {
            let Some(ev) = self.hermes_out.try_receive() else { break; };
            hermes_n += 1;
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            if text.is_empty()
                || text.starts_with("[JARBAS] Escutando")
                || text.starts_with("[JARBAS] 🎤")
            {
                continue;
            }
            if self.wake_window == 0
                && !settings::open_mic_enabled()
                && !settings::wake_gate_bypassed()
            {
                self.wake_window = settings::wake_listen_ticks();
            }
            if let Some(user_text) = self.pending_user_text.take() {
                let clean = text
                    .trim_start_matches("[JARBAS] ")
                    .trim_start_matches("JARVIS: ");
                self.conversation.push((user_text, clean.to_string()));
                while self.conversation.len() > self.max_conversation {
                    self.conversation.remove(0);
                }
                k_nano::slog_jarbas!(
                    "Jarbas",
                    "info",
                    "Conversa turno {}",
                    self.conversation.len()
                );
            }
        }

        publish_state(self.derive_state());
        AgentTickResult::Pending
    }
}

/// Frames de MFCC/LSTM processados por tick no job de STT (~64 frames ≈ 1 s de áudio).
/// Mantém o orb/mouse vivos durante a transcrição.
const SLICE_BUDGET_FRAMES: usize = 64;
