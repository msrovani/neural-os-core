use alloc::vec::Vec;
use alloc::vec;
use crate::tensor::Tensor;
use libm::{tanhf};

const SAMPLE_RATE: u32 = 16000;

pub struct PocketTtsEngine {
    loaded: bool,
    embed_w: Option<Tensor>,
    dw1: Option<Tensor>, db1: Option<Tensor>,
    dw2: Option<Tensor>, db2: Option<Tensor>,
    dw3: Option<Tensor>, db3: Option<Tensor>,
    audio_cols: usize,
}

impl PocketTtsEngine {
    pub const fn new() -> Self {
        PocketTtsEngine {
            loaded: false,
            embed_w: None,
            dw1: None, db1: None,
            dw2: None, db2: None,
            dw3: None, db3: None,
            audio_cols: 320,
        }
    }

    pub fn load(&mut self, data: &[u8]) -> bool {
        if data.len() < 16 { return false; }
        let hdr = |off: usize| -> u32 {
            u32::from_le_bytes(data[off..off+4].try_into().unwrap_or([0; 4]))
        };
        if hdr(0) != 0xBE11BE11 { return false; }
        let num_parts = hdr(12) as usize;
        let floats: &[f32] = unsafe {
            core::slice::from_raw_parts(data.as_ptr() as *const f32, data.len() / 4)
        };

        // Parse header entries: name(32B) + offset(4B) + count(4B)
        let mut entries: Vec<(&[u8], usize, usize)> = Vec::new();
        for i in 0..num_parts {
            let base = 16 + i * 40;
            if base + 40 > data.len() { break; }
            let name = &data[base..base+32];
            let off = hdr(base + 32) as usize;
            let cnt = hdr(base + 36) as usize;
            entries.push((name, off, cnt));
        }

        for i in 0..entries.len() {
            let (name_bytes, _off, _cnt) = &entries[i];
            let off = *_off;
            let cnt = *_cnt;
            if off + cnt > floats.len() { continue; }
            let mut f = alloc::vec![0.0f32; cnt];
            f.copy_from_slice(&floats[off..off+cnt]);
            // Compara os primeiros bytes do nome
            let is = |s: &str| -> bool {
                let b = s.as_bytes();
                name_bytes.len() >= b.len() && &name_bytes[..b.len()] == b
            };
            if is("embed") { self.embed_w = Tensor::from_row_major((cnt / 256, 256), f); }
            else if is("dw1_w") { let cols = cnt / 256; self.dw1 = Tensor::from_row_major((256, cols), f); }
            else if is("dw1_b") { self.db1 = Tensor::from_row_major((1, cnt), f); }
            else if is("dw2_w") { let rows = self.dw1.as_ref().map_or(128, |t| t.shape.1); let cols = cnt / rows; self.dw2 = Tensor::from_row_major((rows, cols), f); }
            else if is("dw2_b") { self.db2 = Tensor::from_row_major((1, cnt), f); }
            else if is("dw3_w") { let rows = self.dw2.as_ref().map_or(256, |t| t.shape.1); self.audio_cols = cnt / rows; self.dw3 = Tensor::from_row_major((rows, self.audio_cols), f); }
            else if is("dw3_b") { self.db3 = Tensor::from_row_major((1, cnt), f); }
        }

        self.loaded = self.embed_w.is_some() && self.dw3.is_some();
        if self.loaded {
            crate::serial_println!("[TTS-NEURAL] Pocket TTS loaded: embed={:?}, dw1={:?}, dw2={:?}, dw3={:?}, audio={}cols",
                self.embed_w.as_ref().map(|t| t.shape),
                self.dw1.as_ref().map(|t| t.shape),
                self.dw2.as_ref().map(|t| t.shape),
                self.dw3.as_ref().map(|t| t.shape),
                self.audio_cols);
        }
        self.loaded
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

        let hidden = embed.shape.1;
        let ntok = tokens.len().max(1) as f32;
        let mut latent = Tensor::new((1, hidden));
        for &tok in &tokens {
            let idx = (tok as usize) % embed.shape.0;
            let s = idx * hidden;
            for j in 0..hidden {
                latent.data[j] += embed.data[s + j] / ntok;
            }
        }

        let h1 = gelu_gpu(&latent, w1, b1);
        let h2 = gelu_gpu(&h1, w2, b2);
        let w3t = w3.transposed();
        let raw = crate::gpu::backend::gpu_matmul(&h2, &w3t).unwrap();
        let cols = raw.shape.1;
        let len = SAMPLE_RATE as usize;
        let mut audio = vec![0i16; len];
        for i in 0..len {
            let src = i % cols;
            let val = raw.data[src] + b3.data[src % b3.shape.1];
            let t = i as f32 / len as f32;
            let env = libm::sinf(core::f32::consts::PI * t).max(0.3) * 0.7 + 0.3;
            audio[i] = (val * env * 8000.0) as i16;
        }
        audio
    }
}

fn gelu_gpu(input: &Tensor, w: &Tensor, b: &Tensor) -> Tensor {
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

pub fn try_load_pocket_tts() -> Option<PocketTtsEngine> {
    let load_addr: u64 = 0x100000000;
    let pm = crate::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    let probe = (load_addr + pm) as *const u32;
    let magic = unsafe { core::ptr::read_volatile(probe) };
    crate::serial_println!("[TTS-NEURAL] Probe 0x{:x}+{:#x}: magic=0x{:08x}", load_addr, pm, magic);
    if magic == 0xBE11BE11 {
        let mut eng = PocketTtsEngine::new();
        let data = unsafe { core::slice::from_raw_parts(probe as *const u8, 20 * 1024 * 1024) };
        if eng.load(data) {
            crate::serial_println!("[TTS-NEURAL] Pocket TTS @ 0x{:x} (GPU offload via gpu_matmul)", load_addr);
            return Some(eng);
        } else {
            crate::serial_println!("[TTS-NEURAL] Pocket TTS load FAILED");
        }
    }
    crate::serial_println!("[TTS-NEURAL] Pocket TTS ausente — formant synth ativo");
    None
}
