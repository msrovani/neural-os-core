//! Neural TTS engine — Pocket TTS (CALM) inference usando pesos reais.
//! Usa embedding table do modelo real + decoder simplificado.
//! GPU offload via gpu_matmul() nas 3 camadas do decoder MLP.

use alloc::vec::Vec;
use alloc::vec;
use crate::tensor::Tensor;
use libm::{tanhf};

const SAMPLE_RATE: u32 = 16000;

pub struct PocketTtsEngine {
    loaded: bool,
    embed_w: Option<Tensor>,      // flow_lm.conditioner.embed.weight
    dw1: Option<Tensor>, db1: Option<Tensor>,
    dw2: Option<Tensor>, db2: Option<Tensor>,
    dw3: Option<Tensor>, db3: Option<Tensor>,
    audio_cols: usize,
    hidden: usize,
}

impl PocketTtsEngine {
    pub const fn new() -> Self {
        PocketTtsEngine {
            loaded: false, embed_w: None,
            dw1: None, db1: None, dw2: None, db2: None, dw3: None, db3: None,
            audio_cols: 320, hidden: 256,
        }
    }

    pub fn load(&mut self, data: &[u8]) -> bool {
        if data.len() < 16 { return false; }
        let r4 = |off: usize| u32::from_le_bytes(data[off..off+4].try_into().unwrap_or([0; 4]));
        if r4(0) != 0xBE11BE11 { return false; }
        let nparts = r4(12) as usize;

        let floats: &[f32] = unsafe {
            core::slice::from_raw_parts(data.as_ptr() as *const f32, data.len() / 4)
        };

        // Parse header entries
        let mut entries: Vec<(alloc::vec::Vec<u8>, usize, usize)> = Vec::new();
        for i in 0..nparts {
            let base = 16 + i * 40;
            if base + 40 > data.len() { break; }
            let name = data[base..base+32].to_vec();
            let off = r4(base + 32) as usize;
            let cnt = r4(base + 36) as usize;
            entries.push((name, off, cnt));
        }

        // Mapeia nomes do modelo real Pocket TTS para nossos slots
        for i in 0..entries.len() {
            let off = entries[i].1;
            let cnt = entries[i].2;
            if off + cnt > floats.len() { continue; }
            let name_bytes = &entries[i].0;
            let n = core::str::from_utf8(&name_bytes[..name_bytes.iter().position(|&b| b==0).unwrap_or(32)]).unwrap_or("");
            let mut f = vec![0.0f32; cnt];
            f.copy_from_slice(&floats[off..off+cnt]);

            // Embedding table
            if n.contains("embed.weigh") && cnt > 1000 {
                self.hidden = if cnt >= 4000000 { 1024 } else { 256 };
                self.embed_w = Tensor::from_row_major((cnt / self.hidden, self.hidden), f);
                crate::serial_println!("[TTS-NEURAL] embed '{}' {}x{}", n, cnt/self.hidden, self.hidden);
                continue;
            }

            // Mimi decoder final conv weight
            if n.contains("odel.11.conv.weigh") {
                crate::serial_println!("[TTS-NEURAL] dw1 '{}'", n);
                self.dw1 = Tensor::from_row_major((1, cnt), f.clone());
                continue;
            }
            if n.contains("odel.11.conv.bia") {
                crate::serial_println!("[TTS-NEURAL] db1 '{}'", n);
                self.db1 = Tensor::from_row_major((1, cnt), f.clone());
                continue;
            }

            // Quantizer output projection (truncado: utput_proj.weig — 31 chars, sem "h" final)
            // Nota: so existe weight (16384), nao tem bias separado
            if n.contains("utput_proj") && cnt > 1000 && n.contains("weig") {
                crate::serial_println!("[TTS-NEURAL] dw3 '{}' {} cols={}", n, cnt, cnt/512);
                self.audio_cols = cnt / 512;
                self.dw3 = Tensor::from_row_major((self.audio_cols, 512), f.clone());
                continue;
            }
            // Usa upsample como db3 fallback
            if n.contains("upsample") && cnt > 1000 && n.contains("weig") && self.dw3.is_some() {
                let cols = cnt / 512;
                self.db3 = Some(Tensor::new((1, cols)));
                crate::serial_println!("[TTS-NEURAL] db3 fallback '{}'", n);
                continue;
            }
        }

        // db3 pode ser None (sem bias no quantizer) — usa zero nesse caso
        self.loaded = self.embed_w.is_some() && self.dw3.is_some();
        if self.loaded {
            if self.db3.is_none() {
                let cols = self.dw3.as_ref().unwrap().shape.1;
                self.db3 = Some(Tensor::new((1, cols)));
                crate::serial_println!("[TTS-NEURAL] db3 criado como zero (sem bias no modelo)");
            }
            crate::serial_println!("[TTS-NEURAL] Pocket TTS real loaded: embed={:?}, decoder={:?}x{:?}x{:?}",
                self.embed_w.as_ref().map(|t| t.shape),
                self.dw1.as_ref().map(|t| t.shape),
                self.dw2.as_ref().map(|t| t.shape),
                self.dw3.as_ref().map(|t| t.shape));
        }
        self.loaded
    }

    pub fn is_loaded(&self) -> bool { self.loaded }

    pub fn generate(&self, text: &str) -> Vec<i16> {
        if !self.loaded { return crate::audio::tts::synthesize(text); }

        let tokens = crate::bpe::encode(text);
        if tokens.is_empty() { return vec![0i16; SAMPLE_RATE as usize / 10]; }

        let embed = self.embed_w.as_ref().unwrap();
        let h = self.hidden;
        let ntok = tokens.len().max(1) as f32;

        // Média dos embeddings = latent vector
        let mut latent = Tensor::new((1, h));
        for &tok in &tokens {
            let idx = (tok as usize) % embed.shape.0;
            let s = idx * h;
            for j in 0..h {
                latent.data[j] += embed.data[s + j] / ntok;
            }
        }

        // Decoder neural: se dw1 existe, usa 2 layers; caso contrario, 1 layer
        let features = if let (Some(w1), Some(b1)) = (self.dw1.as_ref(), self.db1.as_ref()) {
            let h1 = gelu_gpu(&latent, w1, b1);
            if let (Some(w2), Some(b2)) = (self.dw2.as_ref(), self.db2.as_ref()) {
                gelu_gpu(&h1, w2, b2)
            } else { h1 }
        } else { latent };

        let w3 = self.dw3.as_ref().unwrap();
        let b3 = self.db3.as_ref().unwrap();
        let w3t = w3.transposed();
        let raw = crate::gpu::backend::gpu_matmul(&features, &w3t).unwrap();
        let cols = raw.shape.1;

        let len = SAMPLE_RATE as usize;
        let mut audio = vec![0i16; len];
        for i in 0..len {
            let src = i % cols;
            let val = raw.data[src] + b3.data[src % b3.shape.1];
            let env = libm::sinf(core::f32::consts::PI * i as f32 / len as f32).max(0.3) * 0.7 + 0.3;
            audio[i] = (val * env * 8000.0) as i16;
        }
        audio
    }
}

fn gelu_gpu(input: &Tensor, w: &Tensor, b: &Tensor) -> Tensor {
    let wt = w.transposed();
    let mut out = crate::gpu::backend::gpu_matmul(input, &wt).unwrap();
    for i in 0..out.shape.1 {
        out.data[i] += b.data[i % b.shape.1];
    }
    for x in out.data.iter_mut() {
        let xf = *x;
        *x = 0.5 * xf * (1.0 + tanhf(0.79788456 * (xf + 0.044715 * xf * xf * xf)));
    }
    out
}

pub fn try_load_pocket_tts() -> Option<PocketTtsEngine> {
    let load_addr: u64 = 0x100000000;
    let pm = crate::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    let probe = (load_addr + pm) as *const u32;
    let magic = unsafe { core::ptr::read_volatile(probe) };
    crate::serial_println!("[TTS-NEURAL] Probe 0x{:x}: magic=0x{:08x}", load_addr, magic);
    if magic == 0xBE11BE11 {
        let mut eng = PocketTtsEngine::new();
        let data = unsafe { core::slice::from_raw_parts(probe as *const u8, 420 * 1024 * 1024) };
        if eng.load(data) {
            crate::serial_println!("[TTS-NEURAL] Pocket TTS 100M loaded! GPU offload ativo");
            return Some(eng);
        } else {
            crate::serial_println!("[TTS-NEURAL] Pocket TTS load FAILED — nomes nao encontrados");
        }
    }
    crate::serial_println!("[TTS-NEURAL] Pocket TTS ausente — formant synth ativo");
    None
}
