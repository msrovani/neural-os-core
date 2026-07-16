//! JarvisVoiceAgent — ouvidos (mic → VAD → wake word) e boca (TTS → speaker).
//! Quem ouve e fala com o usuario é o JARVIS.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use event_bus::{CapabilityToken, Event, Receiver};
use core::sync::atomic::{AtomicU8, Ordering};
use crate::audio::ringbuf::AudioRingBuffer;
use crate::audio::vad::{VAD, VadTransition};
use crate::audio::ser::{extract_features, classify_emotion};
use k_nano::serial_println;

pub static AUDIO_RING: AudioRingBuffer = AudioRingBuffer::new();
pub static LAST_VOICE_EMOTION: AtomicU8 = AtomicU8::new(0);

const VOICE_MANIFEST: AgentManifest = AgentManifest {
    name: "jarvis_voice",
    kind: AgentKind::Console,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

pub struct JarvisVoiceAgent {
    audio_in: Receiver,
    hermes_out: Receiver,
    vad: VAD,
    pcm_buffer: alloc::vec::Vec<i16>,
    listening: bool,
    emotion_samples: alloc::vec::Vec<i16>,
}

impl JarvisVoiceAgent {
    pub fn new() -> Self {
        JarvisVoiceAgent {
            audio_in: k_nano::EVENT_BUS.subscribe(crate::audio::TOPIC_AUDIO_IN),
            hermes_out: k_nano::EVENT_BUS.subscribe("HERMES_RESPONSE"),
            vad: VAD::new(300.0, 16000),
            pcm_buffer: alloc::vec::Vec::new(),
            listening: false,
            emotion_samples: alloc::vec::Vec::new(),
        }
    }
}

impl Agent for JarvisVoiceAgent {
    fn manifest(&self) -> &AgentManifest { &VOICE_MANIFEST }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        // Poll HDA DMA para capturar novo audio do microfone
        crate::audio::hda::poll_hda_audio();

        // ── Ouvidos: processa audio do microfone ──────────────
        while let Some(ev) = self.audio_in.try_receive() {
            let pcm: &[i16] = unsafe {
                core::slice::from_raw_parts(
                    ev.payload.as_ptr() as *const i16,
                    ev.payload.len() / 2,
                )
            };
            // FFT audio: processa energia espectral para o orbe
            crate::display::avatar::process_audio_fft(pcm);

            let frame_size = 320; // 20ms @ 16kHz
            for chunk in pcm.chunks(frame_size) {
                if chunk.len() < frame_size { continue; }
                let (_energy, _zcr, _active, transition) = self.vad.process_frame(chunk);

                if transition == VadTransition::SpeechStart {
                    self.listening = true;
                    self.pcm_buffer.clear();
                    self.emotion_samples.clear();
                    serial_println!("[JARVIS] 🎤 Escutando...");
                    let _ = k_nano::EVENT_BUS.publish(Event {
                        id: 0, topic: alloc::string::String::from("HERMES_RESPONSE"),
                        payload: alloc::format!("[JARVIS] 🎤 Escutando...").into_bytes(),
                        token: CapabilityToken::Legacy(1),
                    });
                }

                if self.listening {
                    self.pcm_buffer.extend_from_slice(chunk);
                    if self.emotion_samples.len() < 16000 {
                        self.emotion_samples.extend_from_slice(chunk);
                    }
                }

                if transition == VadTransition::SpeechEnd && !self.pcm_buffer.is_empty() {
                    serial_println!("[JARVIS] Fala detectada: {} amostras", self.pcm_buffer.len());

                    // SER: detecta emoção na voz
                    if self.emotion_samples.len() > 800 {
                        let features = extract_features(&self.emotion_samples);
                        let emotion = classify_emotion(&features);
                        LAST_VOICE_EMOTION.store(emotion as u8, Ordering::Relaxed);
                        serial_println!("[JARVIS] ❤️ Emoção na voz: {:?} (pitch={:.0}Hz, energy={:.0})",
                            emotion, features.pitch_hz, features.energy_rms);
                    }

                    // STT stub: publica texto simulado
                    let _ = k_nano::EVENT_BUS.publish(Event {
                        id: 0, topic: alloc::string::String::from(crate::audio::TOPIC_STT_TEXT),
                        payload: alloc::format!("[audio {} samples]", self.pcm_buffer.len()).into_bytes(),
                        token: CapabilityToken::Legacy(1),
                    });

                    self.listening = false;
                    self.pcm_buffer.clear();
                }
            }

            AUDIO_RING.push(pcm);
        }

        // ── Boca: resposta do Hermes → TTS real → speaker ────
        while let Some(ev) = self.hermes_out.try_receive() {
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            if text.is_empty() || text.starts_with("[JARVIS] 🎤") { continue; }

            serial_println!("[JARVIS] 🗣️ \"{}\"", text);
            let clean = text.trim_start_matches("[JARVIS] ").trim_start_matches("JARVIS: ");
            let pcm = crate::audio::tts::synthesize(clean);

            let _ = k_nano::EVENT_BUS.publish(Event {
                id: 0, topic: alloc::string::String::from(crate::audio::TOPIC_AUDIO_OUT),
                payload: pcm.iter().flat_map(|s| s.to_le_bytes()).collect(),
                token: CapabilityToken::Legacy(1),
            });
        }

        AgentTickResult::Pending
    }
}
