//! Wake word detection — energia + padrao temporal.
//! Usa VAD para detectar atividade de voz + padrao de 2-3 silabas.
//! Quando o padrao "jar-vis" (2 picos de energia) e detectado,
//! publica TOPIC_WAKEWORD.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use event_bus::{CapabilityToken, Event, Receiver};
use crate::audio::vad::{VAD, VadTransition};
use crate::audio::TOPIC_WAKEWORD;
use crate::serial_println;

const WAKEWORD_MANIFEST: AgentManifest = AgentManifest {
    name: "wakeword",
    kind: AgentKind::Skill,
    schedule: ScheduleKind::EventDriven,
    auto_start: true,
    persist: false,
};

pub struct WakeWordAgent {
    receiver: Receiver,
    vad: VAD,
    energy_history: [f32; 64],
    history_idx: usize,
    cooldown: u32,
}

impl WakeWordAgent {
    pub fn new() -> Self {
        WakeWordAgent {
            receiver: crate::EVENT_BUS.subscribe(crate::audio::TOPIC_AUDIO_IN),
            vad: VAD::new(300.0, 16000),
            energy_history: [0.0; 64],
            history_idx: 0,
            cooldown: 0,
        }
    }

    /// Detecta padrao "jar-vis" na energia: 2 picos separados por ~200-400ms
    fn detect_wakeword_pattern(&self) -> bool {
        if self.history_idx < 20 { return false; }
        let mut peaks = 0u32;
        let mut last_peak = 0usize;
        for i in 1..self.history_idx.saturating_sub(1) {
            if self.energy_history[i] > 500.0
                && self.energy_history[i] > self.energy_history[i-1] * 1.3
                && self.energy_history[i] > self.energy_history[i+1] * 1.3
            {
                if last_peak == 0 || i - last_peak > 3 {
                    peaks += 1;
                    last_peak = i;
                }
            }
        }
        peaks >= 2
    }
}

impl Agent for WakeWordAgent {
    fn manifest(&self) -> &AgentManifest { &WAKEWORD_MANIFEST }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        if self.cooldown > 0 { self.cooldown -= 1; }

        while let Some(ev) = self.receiver.try_receive() {
            let pcm: &[i16] = unsafe {
                core::slice::from_raw_parts(
                    ev.payload.as_ptr() as *const i16,
                    ev.payload.len() / 2,
                )
            };

            let frame_size = 320; // 20ms @ 16kHz
            for chunk in pcm.chunks(frame_size) {
                let (energy, _zcr, _active, transition) = self.vad.process_frame(chunk);

                self.energy_history[self.history_idx % 64] = energy;
                self.history_idx = (self.history_idx + 1).min(64);

                if transition == VadTransition::SpeechStart {
                    serial_println!("[WAKEWORD] Voz detectada (energy={:.0})", energy);
                }

                if transition == VadTransition::SpeechEnd {
                    serial_println!("[WAKEWORD] Silencio detectado");
                    if self.cooldown == 0 && self.detect_wakeword_pattern() {
                        serial_println!("[WAKEWORD] 🔥 'Jarvis' detectado!");
                        self.cooldown = 100;
                        let _ = crate::EVENT_BUS.publish(Event {
                            id: 0,
                            topic: alloc::string::String::from(TOPIC_WAKEWORD),
                            payload: alloc::vec![b'j', b'a', b'r', b'v', b'i', b's'],
                            token: CapabilityToken::Legacy(1),
                        });
                    }
                    self.history_idx = 0;
                }
            }
        }
        AgentTickResult::Pending
    }
}
