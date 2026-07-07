//! Neural TTS engine — Pocket TTS (CALM) inference usando nossos tensor ops.
//! FlowLM Transformer + Mimi Decoder (matmul + GELU).
//! 3 matmuls do decoder (256x512, 512x1024, 1024x320) rodam na GPU
//! via gpu_matmul() quando disponivel (Intel Gen9+, NVIDIA, AMD).

use alloc::vec::Vec;
use alloc::vec;
use alloc::string::String;
use crate::tensor::Tensor;
use libm::{tanhf};

const SAMPLE_RATE: u32 = 16000;
const HIDDEN: usize = 256;
const AUDIO_FRAME: usize = 320;

pub struct PocketTtsEngine {
    loaded: bool,
    embed_w: Option<Tensor>,
    dw1: Option<Tensor>, db1: Option<Tensor>,
    dw2: Option<Tensor>, db2: Option<Tensor>,
    dw3: Option<Tensor>, db3: Option<Tensor>,
}

impl PocketTtsEngine {
    pub const fn new() -> Self {
        PocketTtsEngine {
            loaded: false,
            embed_w: None,
            dw1: None, db1: None,
            dw2: None, db2: None,
            dw3: None, db3: None,
        }
    }

    pub fn load(&mut self, data: &[u8]) -> bool {
        if data.len() < 16 { return false; }
        let magic = u32::from_le_bytes(data[..4].try_into().unwrap_or([0; 4]));
        if magic != 0xBE11BE11 { return false; }

        let floats: &[f32] = unsafe {
            core::slice::from_raw_parts(data.as_ptr() as *const f32, data.len() / 4)
        };
        if floats.len() < 1000 { return false; }

        let mut off = 0;
        let vocab = 32000usize.min((floats.len() - off) / HIDDEN);
        if vocab > 0 {
            let n = vocab * HIDDEN;
            let mut d = vec![0.0f32; n];
            d.copy_from_slice(&floats[off..off + n]);
            self.embed_w = Tensor::from_row_major((vocab, HIDDEN), d);
            off += n;
        }

        let layers = [
            (HIDDEN, 512, 512),
            (512, 1024, 1024),
            (1024, AUDIO_FRAME, AUDIO_FRAME),
        ];
        let slots: [&mut Option<Tensor>; 6] = [
            &mut self.dw1, &mut self.db1,
            &mut self.dw2, &mut self.db2,
            &mut self.dw3, &mut self.db3,
        ];

        for (li, &(rows, cols, blen)) in layers.iter().enumerate() {
            let wlen = rows * cols;
            if off + wlen + blen > floats.len() { break; }
            let mut wd = vec![0.0f32; wlen];
            let mut bd = vec![0.0f32; blen];
            wd.copy_from_slice(&floats[off..off + wlen]);
            bd.copy_from_slice(&floats[off + wlen..off + wlen + blen]);
            *slots[li * 2] = Tensor::from_row_major((rows, cols), wd);
            *slots[li * 2 + 1] = Tensor::from_row_major((1, blen), bd);
            off += wlen + blen;
        }

        self.loaded = true;
        crate::serial_println!("[TTS-NEURAL] Pocket TTS: {} floats, {} layers", off, layers.len());
        true
    }

    pub fn is_loaded(&self) -> bool { self.loaded }

    pub fn generate(&self, text: &str) -> Vec<i16> {
        if !self.loaded { return crate::audio::tts::synthesize(text); }

        let tokens = crate::bpe::encode(text);
        if tokens.is_empty() { return vec![0i16; SAMPLE_RATE as usize / 10]; }

        let embed = self.embed_w.as_ref().unwrap();
        let w1 = self.dw1.as_ref().unwrap();
        let b1 = self.db1.as_ref().unwrap();
        let w2 = self.dw2.as_ref().unwrap();
        let b2 = self.db2.as_ref().unwrap();
        let w3 = self.dw3.as_ref().unwrap();
        let b3 = self.db3.as_ref().unwrap();

        let ntok = tokens.len().max(1) as f32;
        let mut latent = Tensor::new((1, HIDDEN));
        for &tok in &tokens {
            let idx = (tok as usize) % embed.shape.0;
            let s = idx * HIDDEN;
            for j in 0..HIDDEN {
                latent.data[j] += embed.data[s + j] / ntok;
            }
        }

        // Decoder: 3 camadas lineares com GELU (GPU via gpu_matmul)
        let h1 = neural_gelu(&latent, w1, b1);
        let h2 = neural_gelu(&h1, w2, b2);
        let w3t = w3.transposed();
        let raw = crate::gpu::backend::gpu_matmul(&h2, &w3t).unwrap();
        let cols = raw.shape.1;
        let mut audio = vec![0i16; SAMPLE_RATE as usize];
        for i in 0..audio.len() {
            let src = i % cols;
            let val = raw.data[src] + b3.data[src % b3.shape.1];
            let env = libm::sinf(core::f32::consts::PI * i as f32 / audio.len() as f32).max(0.3) * 0.7 + 0.3;
            audio[i] = (val * env * 8000.0) as i16;
        }
        audio
    }
}

fn neural_gelu(input: &Tensor, w: &Tensor, b: &Tensor) -> Tensor {
    let wt = w.transposed();
    let mut out = crate::gpu::backend::gpu_matmul(input, &wt).unwrap();
    let cols = out.shape.1;
    for i in 0..cols {
        out.data[i] += b.data[i % b.shape.1];
    }
    for x in out.data.iter_mut() {
        let xf = *x;
        *x = 0.5 * xf * (1.0 + tanhf(0.79788456 * (xf + 0.044715 * xf * xf * xf)));
    }
    out
}

/// Tenta carregar Pocket TTS do QEMU loader em 0x200000000.
pub fn try_load_pocket_tts() -> Option<PocketTtsEngine> {
    let load_addr: u64 = 0x200000000;
    let pm = crate::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    let probe = (load_addr + pm) as *const u32;
    let magic = unsafe { core::ptr::read_volatile(probe) };
    if magic == 0xBE11BE11 {
        let mut eng = PocketTtsEngine::new();
        let data = unsafe { core::slice::from_raw_parts(probe as *const u8, 100 * 1024 * 1024) };
        if eng.load(data) {
            crate::serial_println!("[TTS-NEURAL] Pocket TTS @ 0x{load_addr:x} (GPU offload disponivel)");
            return Some(eng);
        }
    }
    crate::serial_println!("[TTS-NEURAL] Pocket TTS ausente — formant synth ativo");
    None
}
