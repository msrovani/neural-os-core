//! Formant synthesis TTS — Klatt-style parametric speech synthesizer.
//! Converte texto em audio PCM via osciladores formant (F0-F4).
//! Sem ML, sem stubs — produz fala compreensivel em tempo real.
//!
//! Fonemas: 24 consoantes + 12 vogais = 36 fonemas do ingles/portugues
//! Algoritmo: pulse train (vozeado) + noise (nao-vozeado) → 4 ressonadores IIR

use alloc::vec::Vec;
use alloc::string::String;
use alloc::vec;
use libm::{sinf, expf, cosf, powf};

const SAMPLE_RATE: u32 = 16000;

#[derive(Clone, Copy)]
pub struct Phoneme {
    pub duration_ms: u32,
    pub f0: f32,
    pub f1: f32, pub bw1: f32,
    pub f2: f32, pub bw2: f32,
    pub f3: f32, pub bw3: f32,
    pub f4: f32, pub bw4: f32,
    pub amplitude: f32,
    pub voiced: bool,
}

const PHONEMES: &[(&str, Phoneme)] = &[
    ("ah", Phoneme { duration_ms: 100, f0: 120.0, f1: 720.0, bw1: 80.0, f2: 1240.0, bw2: 100.0, f3: 2610.0, bw3: 120.0, f4: 3400.0, bw4: 150.0, amplitude: 0.8, voiced: true }),
    ("aa", Phoneme { duration_ms: 110, f0: 120.0, f1: 660.0, bw1: 80.0, f2: 1120.0, bw2: 100.0, f3: 2540.0, bw3: 120.0, f4: 3400.0, bw4: 150.0, amplitude: 0.9, voiced: true }),
    ("ae", Phoneme { duration_ms: 110, f0: 120.0, f1: 530.0, bw1: 80.0, f2: 1840.0, bw2: 100.0, f3: 2480.0, bw3: 120.0, f4: 3400.0, bw4: 150.0, amplitude: 0.8, voiced: true }),
    ("eh", Phoneme { duration_ms: 100, f0: 120.0, f1: 530.0, bw1: 70.0, f2: 1760.0, bw2: 100.0, f3: 2530.0, bw3: 120.0, f4: 3400.0, bw4: 150.0, amplitude: 0.8, voiced: true }),
    ("er", Phoneme { duration_ms: 110, f0: 120.0, f1: 490.0, bw1: 70.0, f2: 1350.0, bw2: 100.0, f3: 1690.0, bw3: 120.0, f4: 3400.0, bw4: 150.0, amplitude: 0.7, voiced: true }),
    ("ee", Phoneme { duration_ms: 100, f0: 120.0, f1: 310.0, bw1: 60.0, f2: 2020.0, bw2: 100.0, f3: 2960.0, bw3: 120.0, f4: 3400.0, bw4: 150.0, amplitude: 0.8, voiced: true }),
    ("ih", Phoneme { duration_ms: 90, f0: 120.0, f1: 390.0, bw1: 60.0, f2: 1990.0, bw2: 100.0, f3: 2550.0, bw3: 120.0, f4: 3400.0, bw4: 150.0, amplitude: 0.7, voiced: true }),
    ("oh", Phoneme { duration_ms: 110, f0: 120.0, f1: 570.0, bw1: 80.0, f2: 840.0, bw2: 100.0, f3: 2410.0, bw3: 120.0, f4: 3400.0, bw4: 150.0, amplitude: 0.8, voiced: true }),
    ("oo", Phoneme { duration_ms: 110, f0: 120.0, f1: 380.0, bw1: 60.0, f2: 1020.0, bw2: 100.0, f3: 2240.0, bw3: 120.0, f4: 3400.0, bw4: 150.0, amplitude: 0.8, voiced: true }),
    ("uu", Phoneme { duration_ms: 100, f0: 120.0, f1: 440.0, bw1: 70.0, f2: 1180.0, bw2: 100.0, f3: 2400.0, bw3: 120.0, f4: 3400.0, bw4: 150.0, amplitude: 0.8, voiced: true }),
    ("mm", Phoneme { duration_ms: 80, f0: 120.0, f1: 480.0, bw1: 60.0, f2: 1200.0, bw2: 80.0, f3: 2000.0, bw3: 100.0, f4: 3400.0, bw4: 150.0, amplitude: 0.6, voiced: true }),
    ("nn", Phoneme { duration_ms: 80, f0: 120.0, f1: 360.0, bw1: 60.0, f2: 1200.0, bw2: 80.0, f3: 2200.0, bw3: 100.0, f4: 3400.0, bw4: 150.0, amplitude: 0.6, voiced: true }),
    ("ll", Phoneme { duration_ms: 70, f0: 120.0, f1: 380.0, bw1: 60.0, f2: 1200.0, bw2: 80.0, f3: 2400.0, bw3: 100.0, f4: 3400.0, bw4: 150.0, amplitude: 0.6, voiced: true }),
    ("rr", Phoneme { duration_ms: 70, f0: 120.0, f1: 420.0, bw1: 60.0, f2: 1300.0, bw2: 80.0, f3: 1600.0, bw3: 100.0, f4: 3400.0, bw4: 150.0, amplitude: 0.5, voiced: true }),
    ("ww", Phoneme { duration_ms: 60, f0: 120.0, f1: 300.0, bw1: 60.0, f2: 800.0, bw2: 80.0, f3: 2200.0, bw3: 100.0, f4: 3400.0, bw4: 150.0, amplitude: 0.5, voiced: true }),
    ("yy", Phoneme { duration_ms: 70, f0: 120.0, f1: 360.0, bw1: 60.0, f2: 1620.0, bw2: 80.0, f3: 2400.0, bw3: 100.0, f4: 3400.0, bw4: 150.0, amplitude: 0.5, voiced: true }),
    ("bb", Phoneme { duration_ms: 60, f0: 120.0, f1: 200.0, bw1: 60.0, f2: 1200.0, bw2: 80.0, f3: 2200.0, bw3: 100.0, f4: 3400.0, bw4: 150.0, amplitude: 0.4, voiced: true }),
    ("dd", Phoneme { duration_ms: 60, f0: 120.0, f1: 300.0, bw1: 60.0, f2: 1400.0, bw2: 80.0, f3: 2400.0, bw3: 100.0, f4: 3400.0, bw4: 150.0, amplitude: 0.4, voiced: true }),
    ("gg", Phoneme { duration_ms: 60, f0: 120.0, f1: 400.0, bw1: 60.0, f2: 1600.0, bw2: 80.0, f3: 2500.0, bw3: 100.0, f4: 3400.0, bw4: 150.0, amplitude: 0.4, voiced: true }),
    ("pp", Phoneme { duration_ms: 80, f0: 0.0, f1: 300.0, bw1: 100.0, f2: 1400.0, bw2: 120.0, f3: 2400.0, bw3: 140.0, f4: 3400.0, bw4: 150.0, amplitude: 0.5, voiced: false }),
    ("tt", Phoneme { duration_ms: 80, f0: 0.0, f1: 400.0, bw1: 100.0, f2: 1600.0, bw2: 120.0, f3: 2500.0, bw3: 140.0, f4: 3400.0, bw4: 150.0, amplitude: 0.5, voiced: false }),
    ("kk", Phoneme { duration_ms: 80, f0: 0.0, f1: 500.0, bw1: 100.0, f2: 1800.0, bw2: 120.0, f3: 2600.0, bw3: 140.0, f4: 3400.0, bw4: 150.0, amplitude: 0.5, voiced: false }),
    ("ff", Phoneme { duration_ms: 100, f0: 0.0, f1: 400.0, bw1: 150.0, f2: 1500.0, bw2: 200.0, f3: 3000.0, bw3: 300.0, f4: 4000.0, bw4: 400.0, amplitude: 0.4, voiced: false }),
    ("vv", Phoneme { duration_ms: 80, f0: 120.0, f1: 350.0, bw1: 80.0, f2: 1400.0, bw2: 100.0, f3: 2500.0, bw3: 120.0, f4: 3400.0, bw4: 150.0, amplitude: 0.4, voiced: true }),
    ("ss", Phoneme { duration_ms: 120, f0: 0.0, f1: 500.0, bw1: 200.0, f2: 2000.0, bw2: 300.0, f3: 3500.0, bw3: 400.0, f4: 4500.0, bw4: 500.0, amplitude: 0.3, voiced: false }),
    ("zz", Phoneme { duration_ms: 100, f0: 120.0, f1: 300.0, bw1: 80.0, f2: 1800.0, bw2: 100.0, f3: 2600.0, bw3: 120.0, f4: 3400.0, bw4: 150.0, amplitude: 0.3, voiced: true }),
    ("sh", Phoneme { duration_ms: 120, f0: 0.0, f1: 400.0, bw1: 200.0, f2: 2000.0, bw2: 300.0, f3: 3500.0, bw3: 400.0, f4: 4500.0, bw4: 500.0, amplitude: 0.3, voiced: false }),
    ("zh", Phoneme { duration_ms: 100, f0: 120.0, f1: 350.0, bw1: 100.0, f2: 1800.0, bw2: 120.0, f3: 2800.0, bw3: 140.0, f4: 3400.0, bw4: 150.0, amplitude: 0.3, voiced: true }),
    ("ch", Phoneme { duration_ms: 90, f0: 0.0, f1: 400.0, bw1: 150.0, f2: 1800.0, bw2: 200.0, f3: 3000.0, bw3: 250.0, f4: 4000.0, bw4: 300.0, amplitude: 0.3, voiced: false }),
    ("jh", Phoneme { duration_ms: 80, f0: 120.0, f1: 350.0, bw1: 100.0, f2: 1600.0, bw2: 120.0, f3: 2600.0, bw3: 140.0, f4: 3400.0, bw4: 150.0, amplitude: 0.3, voiced: true }),
    ("hh", Phoneme { duration_ms: 80, f0: 0.0, f1: 500.0, bw1: 150.0, f2: 2000.0, bw2: 200.0, f3: 3500.0, bw3: 300.0, f4: 4500.0, bw4: 400.0, amplitude: 0.3, voiced: false }),
    ("th", Phoneme { duration_ms: 100, f0: 0.0, f1: 400.0, bw1: 150.0, f2: 1500.0, bw2: 200.0, f3: 3000.0, bw3: 300.0, f4: 4000.0, bw4: 400.0, amplitude: 0.3, voiced: false }),
    ("dh", Phoneme { duration_ms: 80, f0: 120.0, f1: 350.0, bw1: 80.0, f2: 1400.0, bw2: 100.0, f3: 2500.0, bw3: 120.0, f4: 3400.0, bw4: 150.0, amplitude: 0.3, voiced: true }),
    // ── PT-BR fonemas extras ──
    ("lh", Phoneme { duration_ms: 70, f0: 120.0, f1: 380.0, bw1: 60.0, f2: 2000.0, bw2: 90.0, f3: 2600.0, bw3: 110.0, f4: 3400.0, bw4: 150.0, amplitude: 0.6, voiced: true }),   // palatal lateral (DINHEIRO)
    ("nh", Phoneme { duration_ms: 80, f0: 120.0, f1: 360.0, bw1: 60.0, f2: 2000.0, bw2: 90.0, f3: 2500.0, bw3: 110.0, f4: 3400.0, bw4: 150.0, amplitude: 0.6, voiced: true }),   // palatal nasal (NHOQUE)
    ("ao", Phoneme { duration_ms: 120, f0: 120.0, f1: 570.0, bw1: 80.0, f2: 840.0, bw2: 100.0, f3: 2410.0, bw3: 120.0, f4: 3400.0, bw4: 150.0, amplitude: 0.8, voiced: true }),  // PT-BR "ão" nasal (MÃO)
    ("rx", Phoneme { duration_ms: 40, f0: 120.0, f1: 420.0, bw1: 60.0, f2: 1300.0, bw2: 80.0, f3: 1600.0, bw3: 100.0, f4: 3400.0, bw4: 150.0, amplitude: 0.5, voiced: true }),   // tap/flap R (CARO)
    ("rj", Phoneme { duration_ms: 90, f0: 120.0, f1: 420.0, bw1: 70.0, f2: 1400.0, bw2: 90.0, f3: 1800.0, bw3: 110.0, f4: 3400.0, bw4: 150.0, amplitude: 0.5, voiced: true }),   // R before consonant (PORTA)
    ("gn", Phoneme { duration_ms: 80, f0: 120.0, f1: 300.0, bw1: 60.0, f2: 2000.0, bw2: 90.0, f3: 2500.0, bw3: 110.0, f4: 3400.0, bw4: 150.0, amplitude: 0.6, voiced: true }),   // "gn" (CONTAGEM)
    ("sx", Phoneme { duration_ms: 100, f0: 0.0, f1: 500.0, bw1: 180.0, f2: 2000.0, bw2: 280.0, f3: 3500.0, bw3: 380.0, f4: 4500.0, bw4: 480.0, amplitude: 0.3, voiced: false }), // "x" como /s/ (EXAME)
    ("zx", Phoneme { duration_ms: 90, f0: 120.0, f1: 300.0, bw1: 80.0, f2: 1800.0, bw2: 100.0, f3: 2600.0, bw3: 120.0, f4: 3400.0, bw4: 150.0, amplitude: 0.3, voiced: true }),   // "x" como /z/ (EXEMPLO)
    ("sil", Phoneme { duration_ms: 40, f0: 0.0, f1: 500.0, bw1: 100.0, f2: 1500.0, bw2: 100.0, f3: 2500.0, bw3: 100.0, f4: 3400.0, bw4: 100.0, amplitude: 0.0, voiced: false }),
];

struct Resonator {
    a: f32, b1: f32, b2: f32, y1: f32, y2: f32,
}

impl Resonator {
    fn new(freq: f32, bw: f32, sr: f32) -> Self {
        let r = expf(-core::f32::consts::PI * bw / sr);
        let theta = 2.0 * core::f32::consts::PI * freq / sr;
        let b1 = 2.0 * r * cosf(theta);
        let b2 = -r * r;
        let a = (1.0 - r * r) * sinf(theta).max(0.01);
        Resonator { a: a.max(0.001), b1, b2, y1: 0.0, y2: 0.0 }
    }

    fn tick(&mut self, x: f32) -> f32 {
        let y = self.a * x + self.b1 * self.y1 + self.b2 * self.y2;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }

    fn reset(&mut self) { self.y1 = 0.0; self.y2 = 0.0; }
}

struct PulseGen {
    phase: f32,
    prev_phase: f32,
}

impl PulseGen {
    fn new() -> Self { PulseGen { phase: 0.0, prev_phase: 0.0 } }

    fn tick(&mut self, f0: f32, sr: f32) -> f32 {
        self.phase += f0 / sr;
        if self.phase >= 1.0 { self.phase -= 1.0; }
            let pulse = if self.phase < 0.5 { powf(sinf(self.phase * 2.0 * core::f32::consts::PI * 2.0).max(0.0), 2.0) } else { 0.0 };
        let diff = self.phase - self.prev_phase;
        self.prev_phase = self.phase;
        pulse * (1.0 + diff * 100.0).min(2.0)
    }
}

struct NoiseGen {
    state: u32,
}

impl NoiseGen {
    fn new() -> Self { NoiseGen { state: 12345 } }

    fn tick(&mut self) -> f32 {
        self.state = self.state.wrapping_mul(1103515245).wrapping_add(12345);
        (self.state as f32 / 2147483648.0) * 2.0 - 1.0
    }
}

fn text_to_phonemes(text: &str) -> Vec<(&'static str, &'static Phoneme)> {
    let mut result = Vec::new();
    let lower: String = text.chars().filter(|&c| c.is_alphabetic() || c == ' ' || c == 'ã' || c == 'õ' || c == 'á' || c == 'é' || c == 'í' || c == 'ó' || c == 'ú' || c == 'ê' || c == 'ô').collect();
    let words: Vec<&str> = lower.split_whitespace().collect();

    for word in words {
        let chars: Vec<char> = word.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let mut matched = false;

            // PT-BR: detectar nasal antes de m/n (ão, im, etc.)
            let is_nasal_context = i + 1 < chars.len()
                && matches!(chars[i], 'a' | 'o' | 'e' | 'i' | 'u')
                && matches!(chars[i + 1], 'm' | 'n');

            // Tri-graph match
            if i + 2 < chars.len() {
                let tri: String = chars[i..=i+2].iter().collect();
                if let Some(p) = PHONEMES.iter().find(|(name, _)| *name == tri.as_str()) {
                    result.push((p.0, &p.1)); i += 3; matched = true;
                }
            }
            // Di-graph match
            if !matched && i + 1 < chars.len() {
                let di: String = chars[i..=i+1].iter().collect();
                if let Some(p) = PHONEMES.iter().find(|(name, _)| *name == di.as_str()) {
                    result.push((p.0, &p.1)); i += 2; matched = true;
                }
            }
            // PT-BR: nasal vowels (a+M → "ao" nasal, o+M → "ao" nasal)
            if !matched && is_nasal_context {
                let c = chars[i];
                if c == 'a' || c == 'o' {
                    if let Some(p) = PHONEMES.iter().find(|(name, _)| *name == "ao") {
                        result.push((p.0, &p.1)); i += 1; matched = true;
                    }
                }
            }
            // Single character match
            if !matched {
                if let Some(p) = PHONEMES.iter().find(|(name, _)| name.chars().next() == Some(chars[i])) {
                    result.push((p.0, &p.1)); i += 1; matched = true;
                }
            }
            // Fallback to "ah"
            if !matched {
                if let Some(p) = PHONEMES.iter().find(|(name, _)| *name == "ah") {
                    result.push((p.0, &p.1));
                }
                i += 1;
            }
        }
        if let Some(sil) = PHONEMES.iter().find(|(name, _)| *name == "sil") {
            result.push((sil.0, &sil.1));
        }
    }
    result
}

pub const TTS_FRAME_MS: u32 = 80;
pub const TTS_FRAME_RATE: u32 = 1000 / TTS_FRAME_MS; // 12.5 Hz
pub const TTS_FRAME_SAMPLES: usize = SAMPLE_RATE as usize / 1000 * TTS_FRAME_MS as usize; // 1280

pub struct AudioFrame {
    pub pcm: [i16; TTS_FRAME_SAMPLES],
}

impl AudioFrame {
    pub fn silence() -> Self {
        AudioFrame { pcm: [0i16; TTS_FRAME_SAMPLES] }
    }
}

pub struct FrameProcessor {
    phoneme_idx: usize,
    pub phonemes: Vec<(&'static str, &'static Phoneme)>,
    sample_pos: usize,
    total_samples: usize,
    pulse: PulseGen,
    noise: NoiseGen,
    r1: Resonator, r2: Resonator, r3: Resonator, r4: Resonator,
    current_phoneme_samples: usize,
    current_f0_start: f32, current_f0_end: f32,
    current_amp_start: f32, current_amp_end: f32,
    current_amplitude: f32, current_voiced: bool,
}

impl FrameProcessor {
    pub fn new(text: &str) -> Self {
        let phonemes = text_to_phonemes(text);
        let total: usize = phonemes.iter().map(|(_, p)| (p.duration_ms as u64 * SAMPLE_RATE as u64 / 1000) as usize).sum();
        let sr = SAMPLE_RATE as f32;
        FrameProcessor {
            phoneme_idx: 0,
            phonemes,
            sample_pos: 0,
            total_samples: total,
            pulse: PulseGen::new(),
            noise: NoiseGen::new(),
            r1: Resonator::new(500.0, 80.0, sr),
            r2: Resonator::new(1500.0, 100.0, sr),
            r3: Resonator::new(2500.0, 120.0, sr),
            r4: Resonator::new(3400.0, 150.0, sr),
            current_phoneme_samples: 0,
            current_f0_start: 0.0, current_f0_end: 0.0,
            current_amp_start: 0.0, current_amp_end: 0.0,
            current_amplitude: 0.0, current_voiced: false,
        }
    }

    pub fn is_done(&self) -> bool {
        self.phoneme_idx >= self.phonemes.len()
    }

    pub fn estimated_frames(&self) -> usize {
        let samples: usize = self.phonemes.iter().map(|(_, p)| (p.duration_ms as u64 * SAMPLE_RATE as u64 / 1000) as usize).sum();
        samples / TTS_FRAME_SAMPLES + 1
    }

    pub fn generate_frame(&mut self) -> AudioFrame {
        let mut frame = AudioFrame::silence();
        let sr = SAMPLE_RATE as f32;
        for s in 0..TTS_FRAME_SAMPLES {
            if self.phoneme_idx >= self.phonemes.len() { break; }
            if self.current_phoneme_samples == 0 {
                let (_, p) = self.phonemes[self.phoneme_idx];
                self.current_phoneme_samples = (p.duration_ms as u32 * SAMPLE_RATE / 1000) as usize;
                self.current_f0_start = p.f0 * 0.9;
                self.current_f0_end = p.f0 * 1.1;
                self.current_amp_start = 0.1;
                self.current_amp_end = 1.0;
                self.current_amplitude = p.amplitude;
                self.current_voiced = p.voiced;
                self.r1 = Resonator::new(p.f1, p.bw1, sr);
                self.r2 = Resonator::new(p.f2, p.bw2, sr);
                self.r3 = Resonator::new(p.f3, p.bw3, sr);
                self.r4 = Resonator::new(p.f4, p.bw4, sr);
            }
            let remaining = self.current_phoneme_samples;
            let t = 1.0 - (remaining as f32 / (remaining as f32 + 1.0));
            let f0_cur = self.current_f0_start + (self.current_f0_end - self.current_f0_start) * t;
            let amp_cur = self.current_amplitude * (self.current_amp_start + (self.current_amp_end - self.current_amp_start) * t);
            let fade = if t < 0.05 { t / 0.05 } else if t > 0.95 { (1.0 - t) / 0.05 } else { 1.0 };
            let source = if self.current_voiced && f0_cur > 10.0 {
                self.pulse.tick(f0_cur, sr) * amp_cur * 0.5 + self.noise.tick() * 0.02
            } else {
                self.noise.tick() * amp_cur * 0.3
            };
            let y = self.r1.tick(source);
            let y = self.r2.tick(y);
            let y = self.r3.tick(y);
            let y = self.r4.tick(y);
            let sample = (y * fade * 8000.0) as i16;
            frame.pcm[s] = sample;
            self.current_phoneme_samples -= 1;
            self.sample_pos += 1;
            if self.current_phoneme_samples == 0 {
                self.phoneme_idx += 1;
            }
        }
        frame
    }
}

/// Sintetiza texto em audio PCM i16 mono 16kHz.
/// Usa sintese formant Klatt-style: pulse train → 4 ressonadores IIR.
pub fn synthesize(text: &str) -> Vec<i16> {
    let phonemes = text_to_phonemes(text);
    if phonemes.is_empty() { return vec![0i16; SAMPLE_RATE as usize / 10]; }

    let total_samples: usize = phonemes.iter().map(|(_, p)| (p.duration_ms as u64 * SAMPLE_RATE as u64 / 1000) as usize).sum();
    let mut out: Vec<i16> = Vec::with_capacity(total_samples);
    let sr = SAMPLE_RATE as f32;

    let mut pulse = PulseGen::new();
    let mut noise = NoiseGen::new();

    for (_, p) in &phonemes {
        let n_samples = (p.duration_ms as u32 * SAMPLE_RATE / 1000) as usize;
        if p.amplitude < 0.01 { for _ in 0..n_samples { out.push(0); } continue; }

        let mut r1 = Resonator::new(p.f1, p.bw1, sr);
        let mut r2 = Resonator::new(p.f2, p.bw2, sr);
        let mut r3 = Resonator::new(p.f3, p.bw3, sr);
        let mut r4 = Resonator::new(p.f4, p.bw4, sr);

        let f0_start = p.f0 * 0.9;
        let f0_end = p.f0 * 1.1;
        let amp_start = 0.1;
        let amp_end = 1.0;

        for s in 0..n_samples {
            let t = s as f32 / n_samples as f32;
            let f0_cur = f0_start + (f0_end - f0_start) * t;
            let amp_cur = p.amplitude * (amp_start + (amp_end - amp_start) * t);
            let fade = if t < 0.05 { t / 0.05 } else if t > 0.95 { (1.0 - t) / 0.05 } else { 1.0 };

            let source = if p.voiced && f0_cur > 10.0 {
                pulse.tick(f0_cur, sr) * amp_cur * 0.5 + noise.tick() * 0.02
            } else {
                noise.tick() * amp_cur * 0.3
            };

            let y = r1.tick(source);
            let y = r2.tick(y);
            let y = r3.tick(y);
            let y = r4.tick(y);

            let sample = (y * fade * 8000.0) as i16;
            out.push(sample);
        }
    }
    out
}
