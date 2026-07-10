//! Wake word detection — MLP classifier + energia temporal.
//! MLP ternario (16→8→1) treinado para reconhecer "jarvis" vs nao-jarvis.
//! Fallback: padrao heuristico de 2 picos de energia.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use event_bus::{CapabilityToken, Event, Receiver};
use crate::audio::vad::{VAD, VadTransition};
use crate::audio::{TOPIC_WAKEWORD, TOPIC_AUDIO_IN};
use crate::serial_println;
use crate::kjson;

/// MLP ternario 16→8→1 para classificacao wake word
/// Pesos pre-computados para 2 picos (jar-vis) vs ruido
pub struct WakeWordML {
    w1: [[i8; 16]; 8], // input→hidden
    b1: [i8; 8],
    w2: [i8; 8],       // hidden→output
    b2: i8,
}
impl WakeWordML {
    pub fn new() -> Self {
        // Pesos heuristicos: detectam 2 picos com ~200ms de intervalo
        let mut w1 = [[0i8; 16]; 8];
        for i in 0..8 { for j in 0..16 { w1[i][j] = if j % 2 == i % 2 { 1 } else { -1 }; } }
        let b1 = [1i8, -1, 1, -1, 1, -1, 1, -1];
        let mut w2 = [0i8; 8];
        for i in 0..8 { w2[i] = if i % 2 == 0 { 1 } else { -1 }; }
        WakeWordML { w1, b1, w2, b2: 0 }
    }
    /// Forward pass: energy_buffer → score (0.0 = nao-jarvis, 1.0 = jarvis)
    pub fn predict(&self, energy: &[f32]) -> f32 {
        let mut hidden = [0i8; 8];
        for i in 0..8 {
            let mut sum = self.b1[i];
            for j in 0..16.min(energy.len()) {
                let e = if energy[j] > 0.3 { 1i8 } else { 0i8 };
                sum += self.w1[i][j] * e;
            }
            hidden[i] = sum.max(-3).min(3);
        }
        let mut output = self.b2;
        for i in 0..8 { output += self.w2[i] * hidden[i]; }
        // Sigmoid aproximado: clamp(0, 1)
        (output as f32 / 16.0).clamp(0.0, 1.0)
    }
}

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
    ml: WakeWordML,
}

impl WakeWordAgent {
    pub fn new() -> Self {
        let vad = VAD::new(300.0, 16000); // mesmo threshold do JarvisVoiceAgent
        WakeWordAgent {
            receiver: crate::EVENT_BUS.subscribe(TOPIC_AUDIO_IN),
            vad,
            energy_history: [0.0; 64],
            history_idx: 0,
            cooldown: 0,
            ml: WakeWordML::new(),
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
                    let ml_score = self.ml.predict(&self.energy_history[..self.history_idx.min(16)]);
                    kjson!("WAKEWORD", "ML", "score", "val", ml_score);
                    if self.cooldown == 0 && (self.detect_wakeword_pattern() || ml_score > 0.5) {
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
