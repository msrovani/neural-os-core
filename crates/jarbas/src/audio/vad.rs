//! Voice Activity Detection (VAD) — energia + zero-crossing.
//! Detecta quando o usuario esta falando em tempo real.
//! Usado por WakeWordAgent e SttSkill.

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
        }
    }

    /// Processa um frame de audio (tipicamente 10-30ms).
    /// Retorna: (energia_rms, zcr, is_speech, transition)
    pub fn process_frame(&mut self, pcm: &[i16]) -> (f32, f32, bool, VadTransition) {
        let n = pcm.len() as f32;
        if n == 0.0 { return (0.0, 0.0, self.active, VadTransition::None); }

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

        let is_speech = energy_rms > self.threshold;

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

    pub fn is_active(&self) -> bool { self.active }
    pub fn set_threshold(&mut self, t: f32) { self.threshold = t; }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VadTransition { None, SpeechStart, SpeechEnd }
