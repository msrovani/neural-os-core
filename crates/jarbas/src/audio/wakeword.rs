//! Wake word detection — MLP classifier + energia temporal.
//! MLP ternario (16→8→1) treinado para reconhecer "JARBAS" vs nao-jarvis.
//! Schedule Continuous (Sprint Sound) — evita dormência EventDriven após 20 ticks.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use event_bus::{CapabilityToken, Event, Receiver};
use crate::audio::capture::{FRAME_SAMPLES, TOPIC_AUDIO_FRAME};
use crate::audio::settings::{self, WAKEWORD_SENSITIVITY};
use crate::audio::TOPIC_WAKEWORD;

use core::sync::atomic::Ordering;

/// MLP ternario 16→8→1 para classificacao wake word.
/// Treinado offline com 98.4% de acuracia (2000 jarvis + 8000 nao-jarvis).
pub struct WakeWordML {
    w1: [[i8; 16]; 8],
    b1: [f32; 8],
    w2: [i8; 8],
    b2: [f32; 1],
}

impl WakeWordML {
    pub fn new() -> Self {
        WakeWordML {
            w1: [[1, 1, -1, -1, -1, -1, -1, -1, 1, -1, -1, -1, 0, 1, 1, 1],
                 [1, 1, -1, -1, -1, -1, -1, 0, 1, -1, 0, -1, -1, 0, 1, 1],
                 [-1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, -1, -1],
                 [1, -1, 1, -1, 0, -1, -1, 1, -1, 1, -1, 1, -1, 1, 1, 1],
                 [-1, 1, 1, 1, 1, -1, 1, 1, 1, 1, -1, -1, -1, -1, -1, -1],
                 [1, -1, -1, -1, 1, -1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1],
                 [1, -1, -1, -1, -1, 0, -1, -1, -1, -1, -1, -1, -1, -1, 1, 0],
                 [0, -1, -1, 0, 0, -1, -1, -1, 0, 0, 0, -1, -1, -1, 0, -1]],
            b1: [1.558, 1.365, -0.618, 1.8386, -0.2461, 0.7522, -0.1981, -0.0958],
            w2: [-1, -1, 1, -1, 1, -1, 1, -1],
            b2: [-0.3075],
        }
    }

    pub fn predict(&self, energy: &[f32; 16]) -> f32 {
        let mut h = [0.0f32; 8];
        for i in 0..8 {
            let mut s = self.b1[i];
            for j in 0..16 {
                s += match self.w1[i][j] {
                    1 => energy[j],
                    -1 => -energy[j],
                    _ => 0.0,
                };
            }
            h[i] = if s > 0.0 { s } else { 0.0 };
        }
        let mut out = self.b2[0];
        for i in 0..8 {
            out += match self.w2[i] {
                1 => h[i],
                -1 => -h[i],
                _ => 0.0,
            };
        }
        1.0 / (1.0 + libm::expf(-out))
    }
}

const WAKEWORD_MANIFEST: AgentManifest = AgentManifest {
    name: "wakeword",
    kind: AgentKind::Skill,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

/// RMS de um frame — mesma escala do VAD (base do treino do MLP).
fn rms(pcm: &[i16]) -> f32 {
    if pcm.is_empty() {
        return 0.0;
    }
    let mut sum = 0.0f32;
    for &s in pcm {
        let v = s as f32;
        sum += v * v;
    }
    libm::sqrtf(sum / pcm.len() as f32)
}

pub struct WakeWordAgent {
    receiver: Receiver,
    energy_history: [f32; 64],
    history_idx: usize,
    cooldown: u32,
    ml: WakeWordML,
    last_score_log: u32,
}

impl WakeWordAgent {
    pub fn new() -> Self {
        // Sem VAD próprio: o VAD é único e vive no `AudioInputAgent` (antes havia
        // DUAS instâncias independentes processando o mesmo stream, com estados que
        // podiam discordar). Aqui só classificamos frames já normalizados.
        WakeWordAgent {
            receiver: k_nano::EVENT_BUS.subscribe(TOPIC_AUDIO_FRAME),
            energy_history: [0.0; 64],
            history_idx: 0,
            cooldown: 0,
            ml: WakeWordML::new(),
            last_score_log: 0,
        }
    }

    /// Detecta padrao "jar-vis" na energia: 2 picos separados por ~200-400ms.
    fn detect_wakeword_pattern(&self) -> bool {
        if self.history_idx < 20 {
            return false;
        }
        let mut peaks = 0u32;
        let mut last_peak = 0usize;
        let sens = WAKEWORD_SENSITIVITY.load(Ordering::Relaxed).max(1) as f32;
        let peak_thr = 500.0 * (6.0 / (sens + 1.0));
        for i in 1..self.history_idx.saturating_sub(1) {
            if self.energy_history[i] > peak_thr
                && self.energy_history[i] > self.energy_history[i - 1] * 1.3
                && self.energy_history[i] > self.energy_history[i + 1] * 1.3
            {
                if last_peak == 0 || i - last_peak > 3 {
                    peaks += 1;
                    last_peak = i;
                }
            }
        }
        peaks >= 2
    }

    fn publish_wake(&mut self, score: f32, via: &str) {
        self.cooldown = settings::wake_cooldown_ticks();
        k_nano::slog_bin!("WAKEWORD", "info", "HIT \"jarvis\" via={} score={:.2}", via, score);
        let _ = k_nano::EVENT_BUS.publish(Event {
            id: 0,
            topic: alloc::string::String::from(TOPIC_WAKEWORD),
            payload: alloc::vec![b'j', b'a', b'r', b'v', b'i', b's'],
            token: CapabilityToken::Legacy(1),
        });
    }
}

impl Agent for WakeWordAgent {
    fn manifest(&self) -> &AgentManifest {
        &WAKEWORD_MANIFEST
    }

    fn tick(&mut self, tick: u64, _count: u64) -> AgentTickResult {
        if self.cooldown > 0 {
            self.cooldown -= 1;
        }

        // Frames já chegam normalizados em 320 amostras @16 kHz mono (antes este
        // agente fatiava eventos de 512 e DESCARTAVA as 192 amostras restantes,
        // 37,5% do áudio, sempre nas mesmas fronteiras).
        let mut drained = 0u32;
        while let Some(ev) = self.receiver.try_receive() {
            if drained >= 16 {
                break;
            }
            drained += 1;
            if ev.payload.len() < FRAME_SAMPLES * 2 {
                continue;
            }
            let pcm: &[i16] = unsafe {
                core::slice::from_raw_parts(ev.payload.as_ptr() as *const i16, FRAME_SAMPLES)
            };
            let energy = rms(pcm);
            self.energy_history[self.history_idx] = energy;
            self.history_idx += 1;

            if self.history_idx >= 64 {
                let mut energy_16 = [0.0f32; 16];
                energy_16.copy_from_slice(&self.energy_history[..16]);
                let ml_score = self.ml.predict(&energy_16);
                if tick.wrapping_sub(self.last_score_log as u64) > 50 {
                    self.last_score_log = tick as u32;
                }
                let thr = settings::wake_ml_threshold();
                let pattern = self.detect_wakeword_pattern();
                if self.cooldown == 0 && (pattern || ml_score > thr) {
                    let via = if pattern && ml_score > thr {
                        "pattern+ml"
                    } else if pattern {
                        "pattern"
                    } else {
                        "ml"
                    };
                    self.publish_wake(ml_score, via);
                }
                // Janelas não sobrepostas de 64 frames (~1,28 s).
                self.history_idx = 0;
            }
        }
        AgentTickResult::Pending
    }
}
