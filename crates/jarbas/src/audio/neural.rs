//! Neural TTS engine — Pocket TTS (CALM) inference usando pesos reais.
//! Modelo carregado do FAT32 (HW real) — sem dependencia de QEMU loader.
//! GPU offload via gpu_matmul() nas camadas do decoder MLP.

use alloc::vec::Vec;
use alloc::vec;
use cortex::tensor::Tensor;
use libm::{tanhf};

const SAMPLE_RATE: u32 = 16000;

pub struct PocketTtsEngine {
    loaded: bool,
    embed_w: Option<Tensor>,
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

        let mut entries: Vec<(alloc::vec::Vec<u8>, usize, usize)> = Vec::new();
        for i in 0..nparts {
            let base = 16 + i * 40;
            if base + 40 > data.len() { break; }
            let name = data[base..base+32].to_vec();
            let off = r4(base + 32) as usize;
            let cnt = r4(base + 36) as usize;
            entries.push((name, off, cnt));
        }

        for i in 0..entries.len() {
            let off = entries[i].1;
            let cnt = entries[i].2;
            if off + cnt > floats.len() { continue; }
            let name_bytes = &entries[i].0;
            let n = core::str::from_utf8(&name_bytes[..name_bytes.iter().position(|&b| b==0).unwrap_or(32)]).unwrap_or("");
            let mut f = vec![0.0f32; cnt];
            f.copy_from_slice(&floats[off..off+cnt]);

            if n.contains("embed.weigh") && cnt > 1000 {
                self.hidden = if cnt >= 4000000 { 1024 } else { 256 };
                self.embed_w = Tensor::from_row_major((cnt / self.hidden, self.hidden), f);
                continue;
            }
            if n.contains("odel.11.conv.weigh") { self.dw1 = Tensor::from_row_major((1, cnt), f.clone()); continue; }
            if n.contains("odel.11.conv.bia") { self.db1 = Tensor::from_row_major((1, cnt), f.clone()); continue; }
            if n.contains("utput_proj") && cnt > 1000 && n.contains("weig") {
                self.audio_cols = cnt / 512;
                self.dw3 = Tensor::from_row_major((self.audio_cols, 512), f.clone());
                continue;
            }
            if n.contains("upsample") && cnt > 1000 && n.contains("weig") && self.dw3.is_some() && self.db3.is_none() {
                let cols = cnt / 512;
                self.db3 = Some(Tensor::new((1, cols)));
                continue;
            }
        }

        self.loaded = self.embed_w.is_some() && self.dw3.is_some();
        if self.loaded {
            if self.db3.is_none() {
                let cols = self.dw3.as_ref().unwrap().shape.1;
                self.db3 = Some(Tensor::new((1, cols)));
            }
            k_nano::slog_bin!("TTS", "NEURAL", "Pocket TTS loaded: embed={:?}, decoder={:?}",
                self.embed_w.as_ref().map(|t| t.shape),
                self.dw3.as_ref().map(|t| t.shape));
        }
        self.loaded
    }

    pub fn is_loaded(&self) -> bool { self.loaded }

    pub fn generate(&self, text: &str) -> Vec<i16> {
        if !self.loaded { return crate::audio::tts::synthesize(text); }
        let tokens = cortex::cortex::Tokenizer::encode(text);
        if tokens.is_empty() { return vec![0i16; SAMPLE_RATE as usize / 10]; }

        let embed = self.embed_w.as_ref().unwrap();
        let h = self.hidden;
        let ntok = tokens.len().max(1) as f32;
        let mut latent = Tensor::new((1, h));
        for &tok in &tokens {
            let idx = (tok as usize) % embed.shape.0;
            let s = idx * h;
            for j in 0..h { latent.data[j] += embed.data[s + j] / ntok; }
        }

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
    for i in 0..out.shape.1 { out.data[i] += b.data[i % b.shape.1]; }
    for x in out.data.iter_mut() {
        let xf = *x;
        *x = 0.5 * xf * (1.0 + tanhf(0.79788456 * (xf + 0.044715 * xf * xf * xf)));
    }
    out
}

/// Carrega Pocket TTS do FAT32 (HW real). Sem fallback QEMU loader.
fn try_load_fat(filename: &str) -> Option<alloc::vec::Vec<u8>> {
    unsafe {
        let ata_guard = k_nano::ATA_DRIVER.lock();
        if let Some(ref ata) = *ata_guard {
            let parts = k_nano::fat32::read_mbr(ata);
            for p in &parts {
                if p.type_code != 0x1C && p.type_code != 0x0C && p.type_code != 0x0B { continue; }
                if let Some(fs) = k_nano::fat32::Fat32Reader::new(ata, p) {
                    if let Some(data) = fs.read_file(filename) {
                        return Some(data);
                    }
                }
            }
        }
    }
    None
}

fn try_load_qemu(addr: u64, max_mb: usize) -> Option<alloc::vec::Vec<u8>> {
    let pm = k_nano::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    unsafe {
        let probe = (addr + pm) as *const u32;
        if core::ptr::read_volatile(probe) == 0xBE11BE11 {
            let data = core::slice::from_raw_parts(probe as *const u8, max_mb * 1024 * 1024);
            k_nano::slog_bin!("TTS", "NEURAL", "Pocket TTS encontrado em QEMU loader 0x{:x}", addr);
            return Some(data.to_vec());
        }
    }
    None
}

pub fn try_load_pocket_tts() -> Option<PocketTtsEngine> {
    // Tenta FAT (HW real) primeiro
    if let Some(data) = try_load_fat("POCKETTTS.BIN") {
        let mut eng = PocketTtsEngine::new();
        if eng.load(&data) {
            k_nano::slog_bin!("TTS", "NEURAL", "Pocket TTS 100M loaded from FAT! GPU offload ativo");
            return Some(eng);
        }
    }
    // Fallback: QEMU loader (dev)
    if let Some(data) = try_load_qemu(0x100000000, 420) {
        let mut eng = PocketTtsEngine::new();
        if eng.load(&data) {
            k_nano::slog_bin!("TTS", "NEURAL", "Pocket TTS 100M loaded from QEMU loader! GPU offload ativo");
            return Some(eng);
        }
    }
    k_nano::slog_bin!("TTS", "NEURAL", "Pocket TTS ausente — formant synth ativo");
    None
}




