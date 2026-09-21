//! Wake word detection — MLP classifier + energia temporal.
//! MLP ternario (16→8→1) treinado para reconhecer "JARBAS" vs nao-jarvis.
//! Schedule Continuous (Sprint Sound) — evita dormência EventDriven após 20 ticks.
//!
//! SESSION_352 fix: MLP lê os 16 frames **mais recentes** (janela deslizante),
//! não `energy_history[..16]` (os mais velhos de 64 ≈ 1,28 s atrás).

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use event_bus::{CapabilityToken, Event};
use crate::audio::mic_ring::WAKE_MIC_RING;
use crate::audio::settings::{self, WAKEWORD_SENSITIVITY};
use crate::audio::TOPIC_WAKEWORD;

use core::sync::atomic::Ordering;

/// MLP ternario 16→8→1 para classificacao wake word.
/// Pesos embutidos — acurácia "98,4%" do comentário legado **não tem artefato** no repo.
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

const HIST: usize = 64;
const MLP_WIN: usize = 16;

pub struct WakeWordAgent {
    /// Ring circular de RMS por frame (16 kHz / 320 = 50 Hz).
    energy_history: [f32; HIST],
    /// Próximo slot de escrita (mod HIST).
    write_idx: usize,
    /// Frames válidos no ring (cap HIST).
    filled: usize,
    cooldown: u32,
    ml: WakeWordML,
    last_score_log: u32,
}

impl WakeWordAgent {
    pub fn new() -> Self {
        WakeWordAgent {
            energy_history: [0.0; HIST],
            write_idx: 0,
            filled: 0,
            cooldown: 0,
            ml: WakeWordML::new(),
            last_score_log: 0,
        }
    }

    /// Copia os `MLP_WIN` frames mais recentes (ordem temporal antiga→nova).
    fn recent_energy16(&self) -> [f32; MLP_WIN] {
        let mut out = [0.0f32; MLP_WIN];
        for i in 0..MLP_WIN {
            // write_idx aponta para o próximo slot vazio = um após o mais recente.
            let idx = (self.write_idx + HIST - MLP_WIN + i) % HIST;
            out[i] = self.energy_history[idx];
        }
        out
    }

    /// Detecta padrao "jar-vis" na energia recente: 2 picos separados por ~200-400ms.
    fn detect_wakeword_pattern(&self) -> bool {
        if self.filled < 20 {
            return false;
        }
        let mut peaks = 0u32;
        let mut last_peak = 0usize;
        let sens = WAKEWORD_SENSITIVITY.load(Ordering::Relaxed).max(1) as f32;
        let peak_thr = 500.0 * (6.0 / (sens + 1.0));
        let n = self.filled.min(HIST);
        for i in 1..n.saturating_sub(1) {
            // Índice temporal: do mais antigo ao mais novo na janela filled.
            let idx = (self.write_idx + HIST - n + i) % HIST;
            let prev = self.energy_history[(idx + HIST - 1) % HIST];
            let cur = self.energy_history[idx];
            let next = self.energy_history[(idx + 1) % HIST];
            if cur > peak_thr && cur > prev * 1.3 && cur > next * 1.3 {
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
        k_nano::slog_bin!("WAKEWORD", "ok", "HIT \"jarvis\" via={} score={:.2}", via, score);
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

        let mut drained = 0u32;
        while drained < 16 {
            let Some(pcm) = WAKE_MIC_RING.try_pop() else { break; };
            drained += 1;
            let energy = rms(&pcm);
            self.energy_history[self.write_idx] = energy;
            self.write_idx = (self.write_idx + 1) % HIST;
            if self.filled < HIST {
                self.filled += 1;
            }

            // Avalia a cada frame após encher a janela MLP (não espera 64).
            if self.filled < MLP_WIN {
                continue;
            }
            let energy_16 = self.recent_energy16();
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
        }
        AgentTickResult::Pending
    }
}
