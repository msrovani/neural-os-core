//! JarbasVoiceAgent — ouvidos (mic → VAD → wake-gated STT) e boca (TTS → speaker).
//! Sprint Sound: gate pós-WAKEWORD com timeout; MIC_CAPTURE_RING separado do playback.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use event_bus::{CapabilityToken, Event, Receiver};
use core::sync::atomic::{AtomicU8, Ordering};
use crate::audio::ringbuf::AudioRingBuffer;
use crate::audio::vad::{VAD, VadTransition};
use crate::audio::ser::{extract_features, classify_emotion};
use crate::audio::settings;
/// Ring de captura (mic) — barge-in / pipeline lê daqui.
pub static MIC_CAPTURE_RING: AudioRingBuffer = AudioRingBuffer::new();
/// Ring de playback (TTS) — mixer drena para HDA/UAC.
pub static PLAYBACK_RING: AudioRingBuffer = AudioRingBuffer::new();

/// Compat: alias histórico (mic). Preferir MIC_CAPTURE_RING / PLAYBACK_RING.
pub static AUDIO_RING: AudioRingBuffer = AudioRingBuffer::new();

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

        if self.wake_window > 0 {
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
            let _ = AUDIO_RING.push(pcm);

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
                            LAST_VOICE_EMOTION.store(emotion as u8, Ordering::Relaxed);
                            k_nano::slog_jarbas!("Jarbas", "info", "Emocao: {:?} (pitch={:.0}Hz, energy={:.0})",
                                emotion,
                                features.pitch_hz,
                                features.energy_rms);
                        }
                    }

                    let text = crate::audio::stt::transcribe_global(&self.pcm_buffer);
                    if !text.is_empty() {
                        k_nano::slog_jarbas!("Jarbas", "info", "STT: \"{}\"", text);
                        let _ = k_nano::EVENT_BUS.publish(Event {
                            id: 0,
                            topic: alloc::string::String::from(crate::audio::TOPIC_STT_TEXT),
                            payload: text.clone().into_bytes(),
                            token: CapabilityToken::Legacy(1),
                        });
                        let _ = k_nano::EVENT_BUS.publish(Event {
                            id: 0,
                            topic: alloc::string::String::from("USER_INTENT"),
                            payload: text.into_bytes(),
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

        // Boca: HERMES_RESPONSE → Piper neural-lite / formant → AUDIO_OUT
        while let Some(ev) = self.hermes_out.try_receive() {
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            if text.is_empty() || text.starts_with("[JARBAS] Escutando") || text.starts_with("[JARBAS] 🎤")
            {
                continue;
            }

            k_nano::slog_jarbas!("Jarbas", "info", "TTS: \"{}\"", text);
            let clean = text
                .trim_start_matches("[JARBAS] ")
                .trim_start_matches("JARVIS: ");
            let pcm = crate::audio::skills::synthesize_tts(clean);

            let _ = k_nano::EVENT_BUS.publish(Event {
                id: 0,
                topic: alloc::string::String::from(crate::audio::TOPIC_AUDIO_OUT),
                payload: pcm.iter().flat_map(|s| s.to_le_bytes()).collect(),
                token: CapabilityToken::Legacy(1),
            });
        }

        AgentTickResult::Pending
    }
}
