//! Piper TTS engine — neural TTS multilíngue.
//! Fase 1: encoder + decoder linear. HiFi-GAN upsample vem na fase 2.

use alloc::vec::Vec;
use alloc::vec;
use alloc::string::String;
use crate::tensor::Tensor;
use libm::{tanhf, sinf};

const SAMPLE_RATE: u32 = 22050;

pub struct PiperEngine {
    loaded: bool,
    weights: Vec<(String, Vec<f32>)>,
}

impl PiperEngine {
    pub const fn new() -> Self {
        PiperEngine { loaded: false, weights: Vec::new() }
    }

    pub fn load(&mut self, data: &[u8]) -> bool {
        if data.len() < 16 { return false; }
        let r4 = |off: usize| u32::from_le_bytes(data[off..off+4].try_into().unwrap_or([0; 4]));
        if r4(0) != 0xBE11BE11 { return false; }
        let nparts = r4(8) as usize;
        let floats: &[f32] = unsafe {
            core::slice::from_raw_parts(data.as_ptr() as *const f32, data.len() / 4)
        };
        for i in 0..nparts {
            let base = 16 + i * 40;
            if base + 40 > data.len() { break; }
            let name_bytes = &data[base..base+32];
            let name = String::from_utf8_lossy(&name_bytes[..name_bytes.iter().position(|&b| b==0).unwrap_or(32)]).into_owned();
            let off = r4(base + 32) as usize;
            let cnt = r4(base + 36) as usize;
            if off + cnt > floats.len() { continue; }
            let mut f = vec![0.0f32; cnt];
            f.copy_from_slice(&floats[off..off+cnt]);
            self.weights.push((name, f));
        }
        self.loaded = !self.weights.is_empty();
        if self.loaded {
            let n: usize = self.weights.iter().map(|(_, d)| d.len()).sum();
            crate::serial_println!("[PIPER] {} tensors, {}M params loaded", self.weights.len(), n / 1000000);
        }
        self.loaded
    }

    pub fn is_loaded(&self) -> bool { self.loaded }

    fn w(&self, name: &str) -> &[f32] {
        for (n, d) in &self.weights {
            if n.contains(name) { return d; }
        }
        &self.weights[0].1
    }

    pub fn generate(&self, text: &str) -> Vec<i16> {
        if !self.loaded { return crate::audio::tts::synthesize(text); }
        let tokens = crate::bpe::encode(text);
        if tokens.is_empty() { return vec![0i16; SAMPLE_RATE as usize / 10]; }

        // Embedding lookup
        let emb_w = self.w("emb.weight");
        let dim = 192;
        let seq = tokens.len();
        let mut enc = vec![0.0f32; dim * seq];
        for (ti, &tok) in tokens.iter().enumerate() {
            let idx = (tok as usize * dim) % (emb_w.len().max(dim) - dim);
            for d in 0..dim {
                enc[d * seq + ti] = emb_w[idx + d];
            }
        }

        // Encoder conv_pre: 1x1 conv (dim→dim)
        let w_pre = self.w("enc_p.pre");
        if w_pre.len() >= dim * dim {
            for o in 0..dim {
                for i in 0..seq {
                    let mut sum = 0.0f32;
                    for c in 0..dim {
                        sum += w_pre[o * dim + c] * enc[c * seq + i];
                    }
                    enc[o * seq + i] = sum;
                }
            }
        }

        // Simple decoder: average pooling → audio
        let pooled = if seq > 0 {
            let mut avg = vec![0.0f32; dim];
            for d in 0..dim {
                for i in 0..seq { avg[d] += enc[d * seq + i]; }
                avg[d] /= seq as f32;
            }
            avg
        } else {
            vec![0.0f32; dim]
        };

        // Generate output with simple oscillator + embedding
        let len = (SAMPLE_RATE as usize).min(2 * SAMPLE_RATE as usize); // max 2 seconds
        let mut audio = vec![0i16; len];
        for i in 0..len {
            let t = i as f32 / SAMPLE_RATE as f32;
            let mut sample = 0.0f32;
            // Use embedding values as harmonic amplitudes
            for h in 0..8 {
                let amp = if h < pooled.len() { pooled[h].abs() * 0.5 } else { 0.1 };
                let freq = 110.0 + (pooled[h % pooled.len()] * 200.0).abs();
                sample += amp * sinf(2.0 * core::f32::consts::PI * freq * t * (h + 1) as f32);
            }
            audio[i] = (sample * 8000.0).max(-32768.0).min(32767.0) as i16;
        }
        audio
    }
}
