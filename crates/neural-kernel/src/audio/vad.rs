//! Voice Activity Detection — energia + ZCR + noise-floor adaptativo (Sprint Sound).

use libm::sqrtf;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub static VAD_ACTIVE: AtomicBool = AtomicBool::new(false);
pub static VAD_ENERGY: AtomicU32 = AtomicU32::new(0);

pub struct VAD {
    threshold: f32,
    min_speech_frames: u32,
    min_silence_frames: u32,
    speech_count: u32,
    silence_count: u32,
    active: bool,
    sample_rate: u32,
    /// EMA do piso de ruído (RMS).
    noise_floor: f32,
    /// Histerese: enter = floor + margin; exit = floor + margin*0.6.
    margin: f32,
}

impl VAD {
    pub fn new(threshold: f32, sample_rate: u32) -> Self {
        VAD {
            threshold,
            min_speech_frames: 5,
            min_silence_frames: 15,
            speech_count: 0,
            silence_count: 0,
            active: false,
            sample_rate,
            noise_floor: threshold * 0.5,
            margin: threshold * 0.4,
        }
    }

    pub fn with_hangover(mut self, min_speech: u32, min_silence: u32) -> Self {
        self.min_speech_frames = min_speech;
        self.min_silence_frames = min_silence;
        self
    }

    /// Processa um frame (tipicamente 10–30ms).
    /// Retorna: (energia_rms, zcr, is_speech, transition)
    pub fn process_frame(&mut self, pcm: &[i16]) -> (f32, f32, bool, VadTransition) {
        let n = pcm.len() as f32;
        if n == 0.0 {
            return (0.0, 0.0, self.active, VadTransition::None);
        }

        let mut energy_sum = 0.0f32;
        let mut zcr_count = 0.0f32;
        for i in 0..pcm.len() {
            let s = pcm[i] as f32;
            energy_sum += s * s;
            if i > 0 && (pcm[i] as i32).signum() != (pcm[i - 1] as i32).signum() {
                zcr_count += 1.0;
            }
        }
        let energy_rms = sqrtf(energy_sum / n);
        let zcr = zcr_count / n;

        VAD_ENERGY.store((energy_rms * 100.0) as u32, Ordering::Relaxed);

        // Atualiza noise-floor só em silêncio (EMA lenta).
        if !self.active && energy_rms < self.noise_floor * 1.5 + self.margin {
            self.noise_floor = self.noise_floor * 0.95 + energy_rms * 0.05;
        }

        let enter_thr = (self.noise_floor + self.margin).max(self.threshold * 0.5);
        let exit_thr = (self.noise_floor + self.margin * 0.6).max(self.threshold * 0.35);
        // ZCR secundário: fala tipicamente 0.02–0.25; ruído branco alto.
        let zcr_ok = zcr < 0.35;

        let is_speech = if self.active {
            energy_rms > exit_thr && (zcr_ok || energy_rms > enter_thr * 1.5)
        } else {
            energy_rms > enter_thr && zcr_ok
        };

        let transition = if is_speech {
            self.speech_count += 1;
            self.silence_count = 0;
            if !self.active && self.speech_count >= self.min_speech_frames {
                self.active = true;
                VAD_ACTIVE.store(true, Ordering::Relaxed);
                VadTransition::SpeechStart
            } else {
                VadTransition::None
            }
        } else {
            self.silence_count += 1;
            self.speech_count = 0;
            if self.active && self.silence_count >= self.min_silence_frames {
                self.active = false;
                VAD_ACTIVE.store(false, Ordering::Relaxed);
                VadTransition::SpeechEnd
            } else {
                VadTransition::None
            }
        };

        (energy_rms, zcr, self.active, transition)
    }

    pub fn is_active(&self) -> bool {
        self.active
    }
    pub fn set_threshold(&mut self, t: f32) {
        self.threshold = t;
        self.margin = t * 0.4;
    }
    pub fn noise_floor(&self) -> f32 {
        self.noise_floor
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VadTransition {
    None,
    SpeechStart,
    SpeechEnd,
}
