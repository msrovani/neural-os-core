//! JarbasVoiceAgent — ouvidos (mic → VAD → wake-gated STT) e boca (TTS → speaker).
//! Sprint Sound: gate pós-WAKEWORD com timeout; MIC_CAPTURE_RING separado do playback.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use event_bus::{CapabilityToken, Event, Receiver};
use core::sync::atomic::{AtomicU8, Ordering};
use alloc::string::ToString;
use crate::audio::ringbuf::AudioRingBuffer;
use crate::audio::vad::{VAD, VadTransition};
use crate::audio::ser::{extract_features, classify_emotion};
use crate::audio::settings;
/// Ring de captura (mic) — barge-in / pipeline lê daqui.
pub static MIC_CAPTURE_RING: AudioRingBuffer = AudioRingBuffer::new();
/// Ring de playback (TTS) — mixer drena para HDA/UAC.
pub static PLAYBACK_RING: AudioRingBuffer = AudioRingBuffer::new();

pub static LAST_VOICE_EMOTION: AtomicU8 = AtomicU8::new(0);

const VOICE_MANIFEST: AgentManifest = AgentManifest {
    name: "jarvis_voice",
    kind: AgentKind::Console,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

pub struct JarbasVoiceAgent {
    audio_in: Receiver,
    hermes_out: Receiver,
    wakeword_in: Receiver,
    vad: VAD,
    pcm_buffer: alloc::vec::Vec<i16>,
    listening: bool,
    emotion_samples: alloc::vec::Vec<i16>,
    /// Ticks restantes na janela pós-wake (0 = dormindo).
    wake_window: u32,
    /// Texto do usuário pendente (aguardando resposta do assistente para parear).
    pending_user_text: Option<alloc::string::String>,
    /// Histórico de conversa: pares (user, assistant).
    conversation: alloc::vec::Vec<(alloc::string::String, alloc::string::String)>,
    /// Máximo de turnos no histórico.
    max_conversation: usize,
}

impl JarbasVoiceAgent {
    pub fn new() -> Self {
        JarbasVoiceAgent {
            audio_in: k_nano::EVENT_BUS.subscribe(crate::audio::TOPIC_AUDIO_IN),
            hermes_out: k_nano::EVENT_BUS.subscribe("HERMES_RESPONSE"),
            wakeword_in: k_nano::EVENT_BUS.subscribe(crate::audio::TOPIC_WAKEWORD),
            vad: VAD::new(settings::vad_threshold(), 16000),
            pcm_buffer: alloc::vec::Vec::new(),
            listening: false,
            emotion_samples: alloc::vec::Vec::new(),
            wake_window: 0,
            pending_user_text: None,
            conversation: alloc::vec::Vec::new(),
            max_conversation: 10,
        }
    }

    fn can_listen(&self) -> bool {
        settings::wake_gate_bypassed() || self.wake_window > 0
            || crate::display::chat_window::MIC_ACTIVE.load(core::sync::atomic::Ordering::Relaxed)
    }
}

impl Agent for JarbasVoiceAgent {
    fn manifest(&self) -> &AgentManifest {
        &VOICE_MANIFEST
    }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        k_hal::audio::hda::poll_hda_audio();
        crate::audio::usb::poll_uac_audio();

        // Pausar decremento da wake window enquanto aguarda resposta do assistente
        if self.wake_window > 0 && self.pending_user_text.is_none() {
            self.wake_window -= 1;
            if self.wake_window == 0 {
                k_nano::slog_jarbas!("Jarbas", "info", "wake window expirada — dormindo");
                self.listening = false;
                self.pcm_buffer.clear();
            }
        }

        while let Some(ev) = self.wakeword_in.try_receive() {
            let kw = core::str::from_utf8(&ev.payload).unwrap_or("?");
            self.wake_window = settings::wake_listen_ticks();
            k_nano::slog_jarbas!("Jarbas", "info", "wake word \"{}\" — janela {} ticks",
                kw,
                self.wake_window);
        }

        while let Some(ev) = self.audio_in.try_receive() {
            let pcm: &[i16] = unsafe {
                core::slice::from_raw_parts(
                    ev.payload.as_ptr() as *const i16,
                    ev.payload.len() / 2,
                )
            };
            crate::display::avatar::process_audio_fft(pcm);
            let _ = MIC_CAPTURE_RING.push(pcm);

            let frame_size = 320;
            for chunk in pcm.chunks(frame_size) {
                if chunk.len() < frame_size {
                    continue;
                }
                let (_energy, _zcr, _active, transition) = self.vad.process_frame(chunk);

                if !self.can_listen() {
                    continue;
                }

                if transition == VadTransition::SpeechStart {
                    self.listening = true;
                    self.pcm_buffer.clear();
                    self.emotion_samples.clear();
                    k_nano::slog_jarbas!("Jarbas", "info", "Escutando...");
                }

                if self.listening {
                    self.pcm_buffer.extend_from_slice(chunk);
                    if self.emotion_samples.len() < 16000 {
                        self.emotion_samples.extend_from_slice(chunk);
                    }
                }

                if transition == VadTransition::SpeechEnd && !self.pcm_buffer.is_empty() {
                    k_nano::slog_jarbas!("Jarbas", "info", "Fala detectada: {} amostras", self.pcm_buffer.len());

                    if self.emotion_samples.len() >= settings::ser_min_samples() {
                        let features = extract_features(&self.emotion_samples);
                        if features.energy_rms > 50.0 {
                            let emotion = classify_emotion(&features);
                            // EWMA decay: suaviza transições de emoção (α=0.3)
                            let prev = LAST_VOICE_EMOTION.load(Ordering::Relaxed) as f32;
                            let new_val = emotion as u8 as f32;
                            let smoothed = prev * 0.7 + new_val * 0.3;
                            LAST_VOICE_EMOTION.store(smoothed as u8, Ordering::Relaxed);
                            k_nano::slog_jarbas!("Jarbas", "info", "Emocao: {:?} (pitch={:.0}Hz, energy={:.0})",
                                emotion,
                                features.pitch_hz,
                                features.energy_rms);
                        }
                    }

                    let text = crate::audio::stt::transcribe_global(&self.pcm_buffer);
                    if !text.is_empty() {
                        k_nano::slog_jarbas!("Jarbas", "info", "STT: \"{}\"", text);

                        // --- Conversation continuity: prepend contexto das últimas 3 trocas ---
                        let original = text.clone();
                        self.pending_user_text = Some(original.clone());

                        let enhanced = if !self.conversation.is_empty() {
                            let mut ctx = alloc::string::String::new();
                            let start = self.conversation.len().saturating_sub(6);
                            for (u, a) in &self.conversation[start..] {
                                ctx.push_str(&alloc::format!("User: {}\nAssistant: {}\n", u, a));
                            }
                            alloc::format!("{}\nUser: {}", ctx, text)
                        } else {
                            text
                        };

                        let _ = k_nano::EVENT_BUS.publish(Event {
                            id: 0,
                            topic: alloc::string::String::from(crate::audio::TOPIC_STT_TEXT),
                            payload: original.into_bytes(),
                            token: CapabilityToken::Legacy(1),
                        });
                        let _ = k_nano::EVENT_BUS.publish(Event {
                            id: 0,
                            topic: alloc::string::String::from("USER_INTENT"),
                            payload: enhanced.into_bytes(),
                            token: CapabilityToken::Legacy(1),
                        });
                        // Fecha janela após comando útil (economiza false STT).
                        if !settings::wake_gate_bypassed() {
                            self.wake_window = self.wake_window.min(120);
                        }
                    } else {
                        k_nano::slog_jarbas!("Jarbas", "info", "STT vazio ({} amostras) — sem USER_INTENT", self.pcm_buffer.len());
                        let _ = k_nano::EVENT_BUS.publish(Event {
                            id: 0,
                            topic: alloc::string::String::from(crate::audio::TOPIC_STT_TEXT),
                            payload: alloc::format!("[audio {} samples]", self.pcm_buffer.len())
                                .into_bytes(),
                            token: CapabilityToken::Legacy(1),
                        });
                    }

                    self.listening = false;
                    self.pcm_buffer.clear();
                }
            }
        }

        // --- Conversation tracking: HERMES_RESPONSE → parear (user, assistant) ---
        // TTS foi movido para JarbasAgent (jarvis.rs) que faz streaming para PLAYBACK_RING.
        while let Some(ev) = self.hermes_out.try_receive() {
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            if text.is_empty() || text.starts_with("[JARBAS] Escutando") || text.starts_with("[JARBAS] 🎤")
            {
                continue;
            }

            // Resposta recebida: reabre wake window para continuar conversando
            if self.wake_window == 0 && !settings::wake_gate_bypassed() {
                self.wake_window = settings::wake_listen_ticks();
                k_nano::slog_jarbas!("Jarbas", "info", "resposta recebida — wake window reaberta");
            }

            if let Some(user_text) = self.pending_user_text.take() {
                let clean = text
                    .trim_start_matches("[JARBAS] ")
                    .trim_start_matches("JARVIS: ");
                self.conversation.push((user_text, clean.to_string()));
                while self.conversation.len() > self.max_conversation {
                    self.conversation.remove(0);
                }
                k_nano::slog_jarbas!("Jarbas", "info", "Conversa turno {}: user=\"{}\" asst=\"{}\"",
                    self.conversation.len(),
                    &self.conversation.last().map(|(u,_)| u.as_str()).unwrap_or("?"),
                    &self.conversation.last().map(|(_,a)| a.as_str()).unwrap_or("?"));
            }
        }

        AgentTickResult::Pending
    }
}
