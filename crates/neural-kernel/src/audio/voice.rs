use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use event_bus::{CapabilityToken, Event, Receiver};
use crate::audio::ringbuf::AudioRingBuffer;
use crate::serial_println;

pub static AUDIO_RING: AudioRingBuffer = AudioRingBuffer::new();

const VOICE_MANIFEST: AgentManifest = AgentManifest {
    name: "jarvis_voice",
    kind: AgentKind::Console,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

/// JarvisVoiceAgent — os ouvidos e a boca do JARVIS.
/// - Ouvidos: mic → wake word → STT → texto → USER_INTENT (Hermes decide)
/// - Boca: HERMES_RESPONSE → TTS → áudio → speaker
/// Quem delibera é HermesAgent. Quem processa é Cortex.
pub struct JarvisVoiceAgent {
    audio_in: Receiver,
    hermes_out: Receiver,
    listening: bool,
}

impl JarvisVoiceAgent {
    pub fn new() -> Self {
        JarvisVoiceAgent {
            audio_in: crate::EVENT_BUS.subscribe(crate::audio::TOPIC_AUDIO_IN),
            hermes_out: crate::EVENT_BUS.subscribe("HERMES_RESPONSE"),
            listening: false,
        }
    }
}

impl Agent for JarvisVoiceAgent {
    fn manifest(&self) -> &AgentManifest { &VOICE_MANIFEST }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        // ── Ouvidos: áudio do microfone ──────────────────────────
        while let Some(_ev) = self.audio_in.try_receive() {
            if !self.listening {
                self.listening = true;
                serial_println!("[JARVIS] 🎤 Escutando... (sherpa-onnx + Rustpotter pendente)");
                let _ = crate::EVENT_BUS.publish(Event {
                    id: 0,
                    topic: alloc::string::String::from("HERMES_RESPONSE"),
                    payload: alloc::format!("[JARVIS] 🎤 Escutando...").into_bytes(),
                    token: CapabilityToken::Legacy(1),
                });
            }
        }

        // ── Boca: resposta do Hermes → fala ──────────────────────
        while let Some(ev) = self.hermes_out.try_receive() {
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            if !text.is_empty() && self.listening && !text.starts_with("[JARVIS]") {
                serial_println!("[JARVIS] 🗣️ \"{}\"", text);
                // Stub: aqui chama TtsSkill (sherpa-onnx) quando disponivel
                self.listening = false;
            }
        }

        AgentTickResult::Pending
    }
}
