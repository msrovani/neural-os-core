//! Speech-to-Text engine — tiny CTC model (MFCC → LSTM → CTC decode).
//! Carrega pesos .bin com MAGIC 0xBE11BE11, executa em CPU com fallback GPU.
//! Vocab: 26 letras + espaço + blank = 28 chars.

use alloc::vec::Vec;
use alloc::vec;
use alloc::string::String;
use libm::{tanhf, expf, cosf, log10f, logf};

const SAMPLE_RATE: u32 = 16000;
const FFTSIZE: usize = 512;
const N_MFCC: usize = 13;
const HIDDEN: usize = 64;
const VOCAB: usize = 28; // a-z + space(26) + blank(27)

pub fn mfcc(pcm: &[i16]) -> Vec<f32> {
    let frame_shift = FFTSIZE / 2;
    let n_frames = pcm.len().saturating_sub(FFTSIZE) / frame_shift + 1;
    if n_frames == 0 { return vec![]; }
    let mut feats = vec![0.0f32; n_frames * N_MFCC];
    for t in 0..n_frames {
        let off = t * frame_shift;
        let mut spectrum = [0.0f32; FFTSIZE / 2 + 1];
        for i in 0..FFTSIZE {
            let idx = (off + i).min(pcm.len() - 1);
            let window = 0.54 - 0.46 * cosf(2.0 * core::f32::consts::PI * i as f32 / (FFTSIZE as f32 - 1.0));
            let val = pcm[idx] as f32 * window;
            // Real FFT approximation: magnitude
            if i < spectrum.len() {
                spectrum[i] += val * cosf(2.0 * core::f32::consts::PI * i as f32 / FFTSIZE as f32);
            }
        }
        // Mel filterbank + log + DCT → MFCC (simplified)
        for m in 0..N_MFCC.min(spectrum.len()) {
            let mut mel = 0.0f32;
            for k in 0..spectrum.len() {
                let mel_k = 2595.0 * log10f(1.0 + k as f32 * 16000.0 / FFTSIZE as f32 / 700.0);
                let center = m as f32 * 200.0 + 200.0;
                let bw = 100.0;
                if (mel_k - center).abs() < bw {
                    mel += spectrum[k] * (1.0 - (mel_k - center).abs() / bw);
                }
            }
            feats[t * N_MFCC + m] = if mel > 1e-10 { logf(mel) } else { 0.0 };
        }
    }
    feats
}

fn sigmoid(x: f32) -> f32 { 1.0 / (1.0 + expf(-x)) }

pub struct SttEngine {
    w: Vec<(String, Vec<f32>)>,
    loaded: bool,
}

impl SttEngine {
    pub const fn new() -> Self { SttEngine { w: Vec::new(), loaded: false } }

    pub fn load(&mut self, data: &[u8]) -> bool {
        if data.len() < 16 { return false; }
        let r4 = |o: usize| u32::from_le_bytes(data[o..o+4].try_into().unwrap_or([0; 4]));
        if r4(0) != 0xBE11BE11 { return false; }
        let n = r4(8) as usize;
        let f32s: &[f32] = unsafe { core::slice::from_raw_parts(data.as_ptr() as *const f32, data.len() / 4) };
        for i in 0..n {
            let b = 16 + i * 40;
            if b + 40 > data.len() { break; }
            let nb = &data[b..b+32];
            let nm = String::from_utf8_lossy(&nb[..nb.iter().position(|&x| x==0).unwrap_or(32)]).into_owned();
            let off = r4(b + 32) as usize;
            let cnt = r4(b + 36) as usize;
            if off + cnt > f32s.len() { continue; }
            let mut d = vec![0.0f32; cnt]; d.copy_from_slice(&f32s[off..off+cnt]);
            self.w.push((nm, d));
        }
        self.loaded = !self.w.is_empty();
        if self.loaded {
            let p: usize = self.w.iter().map(|(_, d)| d.len()).sum();
            crate::serial_println!("[STT] {} tensors, {}K params", self.w.len(), p / 1000);
        }
        self.loaded
    }

    pub fn is_loaded(&self) -> bool { self.loaded }

    fn w(&self, name: &str) -> &[f32] {
        for (n, d) in &self.w { if n.contains(name) { return d; } }
        &self.w[0].1
    }

    // LSTM cell: h, c = lstm(x, h, c, w_ih, w_hh, b_ih, b_hh)
    fn lstm_cell(&self, x: &[f32], h: &mut [f32], c: &mut [f32], dim: usize, w_ih: &[f32], w_hh: &[f32], b_ih: &[f32], b_hh: &[f32]) {
        let gates = |i: usize, inp: &[f32], w: &[f32]| -> f32 {
            let mut s = 0.0f32;
            for j in 0..inp.len() { s += w[i * dim + j] * inp[j]; }
            s
        };
        for i in 0..dim {
            let f = sigmoid(gates(i, x, w_ih) + gates(i, h, w_hh) + b_ih[i] + b_hh[i]);
            let in_g = sigmoid(gates(i + dim, x, w_ih) + gates(i + dim, h, w_hh) + b_ih[i + dim] + b_hh[i + dim]);
            let g = tanhf(gates(i + dim * 2, x, w_ih) + gates(i + dim * 2, h, w_hh) + b_ih[i + dim * 2] + b_hh[i + dim * 2]);
            let o = sigmoid(gates(i + dim * 3, x, w_ih) + gates(i + dim * 3, h, w_hh) + b_ih[i + dim * 3] + b_hh[i + dim * 3]);
            c[i] = f * c[i] + in_g * g;
            h[i] = o * tanhf(c[i]);
        }
    }

    pub fn transcribe(&self, pcm: &[i16]) -> String {
        if !self.loaded || pcm.len() < FFTSIZE { return String::new(); }
        let feats = mfcc(pcm);
        if feats.is_empty() { return String::new(); }
        let n_frames = feats.len() / N_MFCC;
        if n_frames == 0 { return String::new(); }

        // LSTM forward
        let w_ih0 = self.w("lstm0.w_ih");
        let w_hh0 = self.w("lstm0.w_hh");
        let b_ih0 = self.w("lstm0.b_ih");
        let b_hh0 = self.w("lstm0.b_hh");
        let w_ih1 = self.w("lstm1.w_ih");
        let w_hh1 = self.w("lstm1.w_hh");
        let b_ih1 = self.w("lstm1.b_ih");
        let b_hh1 = self.w("lstm1.b_hh");
        let w_out = self.w("out.weight");
        let b_out = self.w("out.bias");

        let mut h0 = vec![0.0f32; HIDDEN];
        let mut c0 = vec![0.0f32; HIDDEN];
        let mut h1 = vec![0.0f32; HIDDEN];
        let mut c1 = vec![0.0f32; HIDDEN];
        let mut logits = vec![0.0f32; n_frames * VOCAB];

        for t in 0..n_frames {
            let x = &feats[t * N_MFCC..(t + 1) * N_MFCC];
            self.lstm_cell(x, &mut h0, &mut c0, HIDDEN, w_ih0, w_hh0, b_ih0, b_hh0);
            self.lstm_cell(&h0, &mut h1, &mut c1, HIDDEN, w_ih1, w_hh1, b_ih1, b_hh1);
            for c in 0..VOCAB {
                let mut s = b_out[c];
                for j in 0..HIDDEN { s += w_out[c * HIDDEN + j] * h1[j]; }
                logits[t * VOCAB + c] = s;
            }
        }

        // CTC decode: best path (argmax + collapse repeats + remove blank)
        let mut prev = VOCAB - 1; // blank
        let mut out = Vec::new();
        for t in 0..n_frames {
            let mut best = 0usize;
            let mut best_v = logits[t * VOCAB];
            for c in 1..VOCAB {
                if logits[t * VOCAB + c] > best_v { best = c; best_v = logits[t * VOCAB + c]; }
            }
            if best != prev && best != VOCAB - 1 {
                let ch = if best < 26 { (b'a' + best as u8) as char } else { ' ' };
                out.push(ch);
            }
            prev = best;
        }
        out.iter().collect()
    }
}
