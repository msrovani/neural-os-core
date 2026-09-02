//! Piper TTS engine — carrega pesos VITS (PT-BR + EN) exportados do ONNX Piper.
//!
//! Estado real (Sprint 107 Part B #5, honesto — sem VITS completo fake):
//! o pipeline VITS de referencia e encoder → duration predictor (estocastico)
//! → flow decoder (normalizing flow) → HiFi-GAN vocoder (~15M params, várias
//! camadas de conv transposta). Implementar isso no_std/soft-float e um
//! trabalho de dias (upsampling transposto multi-estagio + flow invertivel +
//! ruido gaussiano condicionado), fora do escopo desta sprint.
//!
//! O que HA hoje (`generate()`, "neural-lite"): usa o embedding REAL de cada
//! fonema (`emb.weight`/`sid`, [vocab,192], pesos genuinamente carregados do
//! .bitnet Piper) para derivar amplitude/f0 por fonema, e sintetiza via
//! oscilador harmonico (3 senoides) com envelope ADSR simples — NÃO e
//! HiFi-GAN, mas tambem NÃO e formant puro (usa os pesos reais). Duracao por
//! fonema agora varia (vogal > consoante > espaço) em vez de fixa — pequena
//! melhoria "duration"-like, ainda longe do duration predictor estocastico
//! do VITS real. Fallback final = `audio/tts.rs` (formant) se o embedding
//! nao estiver carregado.

use alloc::vec::Vec;
use alloc::vec;
use alloc::string::String;
use libm::expf;

pub const PIPER_SR: u32 = 22050;

const PIPER_MAGIC: u32 = 0xBE11BE11;
const PIPER_VERSION: u32 = 3;

/// Header v3 exportado por `tools/convert_piper_to_bitnet.py` (≠ BGE/LLM .bitnet).
pub fn is_piper_header(data: &[u8]) -> bool {
    if data.len() < 16 {
        return false;
    }
    let r4 = |o: usize| u32::from_le_bytes(data[o..o + 4].try_into().unwrap_or([0; 4]));
    r4(0) == PIPER_MAGIC
        && r4(4) == PIPER_VERSION
        && (50..=512).contains(&r4(8))
}

/// Tamanho do blob Piper a partir do índice de tensores (autodescritivo).
pub fn piper_blob_size(data: &[u8]) -> Option<usize> {
    if !is_piper_header(data) {
        return None;
    }
    let r4 = |o: usize| u32::from_le_bytes(data[o..o + 4].try_into().unwrap_or([0; 4]));
    let n = r4(8) as usize;
    let idx_end = 16usize.saturating_add(n.saturating_mul(40));
    if data.len() < idx_end {
        return None;
    }
    let mut end_bytes = idx_end;
    for i in 0..n {
        let b = 16 + i * 40;
        let off_f32 = r4(b + 32) as usize;
        let cnt = r4(b + 36) as usize;
        let part_end = off_f32.saturating_add(cnt).saturating_mul(4);
        if part_end > end_bytes {
            end_bytes = part_end;
        }
    }
    Some(end_bytes.min(data.len()))
}

fn sigmoid(x: f32) -> f32 { 1.0 / (1.0 + expf(-x)) }

struct W { name: String, data: Vec<f32>, rows: usize, cols: usize } // rows=out_ch, cols=in_ch*k for conv

impl W {
    fn get(&self, r: usize, c: usize) -> f32 { self.data[r * self.cols + c] }
}

pub struct PiperEngine { w: Vec<W>, loaded: bool }

impl PiperEngine {
    pub const fn new() -> Self { PiperEngine { w: Vec::new(), loaded: false } }

    pub fn load(&mut self, data: &[u8]) -> bool {
        if data.len() < 16 { return false; }
        let r4 = |o: usize| u32::from_le_bytes(data[o..o+4].try_into().unwrap_or([0; 4]));
        if r4(0) != 0xBE11BE11 { return false; }
        let n = r4(8) as usize;
        if n == 0 || n > 4096 { return false; }
        let f32s: &[f32] = unsafe { core::slice::from_raw_parts(data.as_ptr() as *const f32, data.len() / 4) };
        // Formato canônico: name[32]+off+cnt. Se índice veio zerado (BIN legado),
        // tenta layout alternativo off+cnt+name[32] após o bloco reservado.
        let mut use_alt = false;
        {
            let b0 = 16;
            let cnt0 = r4(b0 + 36) as usize;
            if cnt0 == 0 {
                use_alt = true;
            }
        }
        for i in 0..n {
            let (nm, off, cnt) = if !use_alt {
                let b = 16 + i * 40;
                if b + 40 > data.len() { break; }
                let nb = &data[b..b+32];
                let nm = String::from_utf8_lossy(&nb[..nb.iter().position(|&x| x==0).unwrap_or(32)]).into_owned();
                (nm, r4(b + 32) as usize, r4(b + 36) as usize)
            } else {
                // Legado quebrado: procura primeiro registro off+cnt+name após 16+n*40
                let base = 16 + n * 40;
                let mut p = base;
                // pula padding até achar cnt>0
                if i == 0 {
                    while p + 40 <= data.len() {
                        let c = r4(p + 4) as usize;
                        if c > 0 && c < f32s.len() { break; }
                        p += 4;
                    }
                    // guarda base via nome sentinela no primeiro W? recalcula por i
                }
                let start = {
                    // varre registros de 40 bytes a partir do primeiro off/cnt válido
                    let mut p = base;
                    while p + 40 <= data.len() {
                        let c = r4(p + 4) as usize;
                        if c > 0 && (r4(p) as usize) + c <= f32s.len() { break; }
                        p += 4;
                    }
                    p + i * 40
                };
                if start + 40 > data.len() { break; }
                let off = r4(start) as usize;
                let cnt = r4(start + 4) as usize;
                let nb = &data[start + 8..start + 40];
                let nm = String::from_utf8_lossy(&nb[..nb.iter().position(|&x| x==0).unwrap_or(32)]).into_owned();
                (nm, off, cnt)
            };
            if cnt == 0 || off + cnt > f32s.len() { continue; }
            let mut d = vec![0.0f32; cnt]; d.copy_from_slice(&f32s[off..off+cnt]);
            // Infer shape from name + data size
            let (rows, cols) = if nm.contains("emb.weight") || nm == "sid" { (cnt / 192, 192)
            } else if nm.contains("ups.") {
                // Transposed conv: [out, in, k]
                if cnt == 524288 { (256, 2048) } else if cnt == 131072 { (128, 1024) } else if cnt == 16384 { (64, 256) } else { (1, cnt) }
            } else if nm.contains("conv_1.weight") { (768, 192 * 3)
            } else if nm.contains("conv_2.weight") { (192, 768 * 3)
            } else if nm.contains("convs.0.weight") { (128, 128 * 3)
            } else if nm.contains("convs.1.weight") {
                let k = if cnt == 49152 { 3 } else if cnt == 81920 { 5 } else if cnt == 114688 { 7 } else { 3 };
                (128, 128 * k)
            } else {
                // Conv or linear: try to determine rows/cols
                let _r = if cnt > 100000 { 384 } else if nm.contains("conv_pre") { 256 }
                       else if nm.contains("proj.") { 192 } else if nm.contains("pre.") { 192 }
                       else if nm.contains("convs_1x1") { 192 }
                       else if nm.contains("weight") && cnt > 1000 {
                           let k = if cnt == 73728 { 384 } else if cnt == 344064 { 256 } else if cnt == 368640 { 384 } else { 192 };
                           k
                       } else { 1 };
                (1, cnt)
            };
            self.w.push(W { name: nm, data: d, rows, cols });
        }
        self.loaded = !self.w.is_empty();
        if self.loaded {
            let p: usize = self.w.iter().map(|w| w.data.len()).sum();
            k_nano::slog_jarbas!("Audio", "piper", "{} tensors, {}M params", self.w.len(), p / 1000000);
        }
        self.loaded
    }

    pub fn is_loaded(&self) -> bool { self.loaded }

    fn w(&self, name: &str) -> &W {
        for w in &self.w { if w.name.contains(name) { return w; } }
        &self.w[0]
    }

    /// Embedding de fonemas: `emb.weight` (alias) ou `sid` [V,192] do ONNX Piper.
    fn emb_table(&self) -> Option<&W> {
        for key in ["emb.weight", "sid"] {
            for w in &self.w {
                if w.name == key || w.name.contains(key) {
                    if w.data.len() >= 192 * 2 { return Some(w); }
                }
            }
        }
        // Fallback: maior tensor com cols=192 e rows em 64..512 (vocab fonemas)
        let mut best: Option<&W> = None;
        for w in &self.w {
            if w.cols == 192 && w.rows >= 64 && w.rows <= 512 {
                if best.map(|b| w.data.len() > b.data.len()).unwrap_or(true) {
                    best = Some(w);
                }
            }
        }
        best
    }

    fn dump_w(&self) { // debug
        for w in &self.w { if w.data.len() > 1000 { k_nano::slog_bin!("Log", "msg", "{}: {}x{}={}", w.name, w.rows, w.cols, w.data.len()); } }
    }

    // Conv1d: [in_ch, in_len] × weight[out_ch, in_ch*k] → [out_ch, out_len]
    fn conv1d(&self, input: &[f32], in_ch: usize, in_len: usize, wt: &W, k: usize, out_ch: usize, stride: usize) -> (Vec<f32>, usize) {
        let out_len = if stride > 1 { in_len * stride } else { core::cmp::max(in_len as isize - k as isize + 1, 0) as usize };
        let mut out = vec![0.0f32; out_ch * out_len];
        if out_len == 0 { return (out, 0); }
        // Weight layout: wt.data[row * (in_ch * k) + c * k + j]
        for o in 0..out_ch {
            for i in 0..out_len {
                let mut sum = 0.0f32;
                if stride == 1 {
                    for c in 0..in_ch {
                        for j in 0..k {
                            let src = i + j;
                            if src < in_len {
                                sum += wt.get(o, c * k + j) * input[c * in_len + src];
                            }
                        }
                    }
                }
                out[o * out_len + i] = sum;
            }
        }
        (out, out_len)
    }

    // Transposed conv (upsample): [in_ch, in_len] → [out_ch, in_len * stride]
    fn conv_transpose1d(&self, input: &[f32], in_ch: usize, in_len: usize, wt: &W, k: usize, out_ch: usize, stride: usize) -> (Vec<f32>, usize) {
        let out_len = in_len * stride;
        let mut out = vec![0.0f32; out_ch * out_len];
        for o in 0..out_ch {
            for i in 0..in_len {
                for c in 0..in_ch {
                    for j in 0..k {
                        let dst = i * stride + j;
                        if dst < out_len {
                            out[o * out_len + dst] += wt.get(o, c * k + (k - 1 - j)) * input[c * in_len + i];
                        }
                    }
                }
            }
        }
        (out, out_len)
    }

    pub fn generate(&self, text: &str) -> Vec<i16> {
        if !self.loaded { return crate::audio::tts::synthesize(text); }

        let dim = 192;
        // Embedding lookup — `sid`/`emb.weight`; se ausente, formant (pcm>0)
        let Some(ew) = self.emb_table() else {
            k_nano::slog_jarbas!("Audio", "piper", "emb invalid len=0 name='' -> formant fallback");
            return crate::audio::tts::synthesize(text);
        };
        let ew_len = ew.data.len();
        let vocab = if ew.rows > 0 { ew.rows } else { ew_len / dim };
        if ew_len < dim * 2 || vocab < 2 {
            k_nano::slog_jarbas!("Audio", "piper", "emb invalid len={} name='{}' -> formant fallback",
                ew_len,
                ew.name);
            return crate::audio::tts::synthesize(text);
        }

        // Tokenização ASCII/PT: strip acentos comuns → a-z; mapa estável → emb id.
        let mut ids: Vec<usize> = Vec::new();
        let mut norm_bytes: Vec<u8> = Vec::new();
        ids.push(1.min(vocab - 1)); // BOS
        for b in text.bytes() {
            let nb = match b {
                b'A'..=b'Z' => b - b'A' + b'a',
                // UTF-8 PT comum: treat continuation / high bytes as skip; Latin1-ish
                0xC3 => continue, // lead of áéíóúãõç — next byte handled below
                0xA1 | 0xA0 | 0xA2 | 0xA3 | 0xA4 | 0xA5 => b'a', // á à â ã
                0xA9 | 0xA8 | 0xAA => b'e',
                0xAD | 0xAC => b'i',
                0xB3 | 0xB2 | 0xB4 | 0xB5 => b'o',
                0xBA | 0xB9 => b'u',
                0xA7 => b'c', // ç
                b'a'..=b'z' | b' ' | b'.' | b',' | b'?' | b'!' => b,
                _ if b < 128 => b' ',
                _ => continue,
            };
            let id = match nb {
                b' ' | b'.' | b',' | b'?' | b'!' => 3,
                b'a'..=b'z' => 10 + (nb - b'a') as usize,
                _ => 3,
            };
            ids.push(id % vocab);
            norm_bytes.push(nb);
        }
        ids.push(2.min(vocab - 1)); // EOS
        if ids.len() <= 2 {
            return crate::audio::tts::synthesize(text);
        }
        let seq = ids.len();
        k_nano::slog_jarbas!("Audio", "piper", "neural-lite emb='{}' vocab={} seq={} (VITS/HiFi-GAN blocked soft-float)", ew.name, vocab, seq);

        // Soft-float: neural-lite com emb real + prosódia (não VITS pleno).
        {
            let sr = PIPER_SR as usize;
            let base_samples = (sr / 18).max(64);
            let phoneme_dur = |b: u8| -> usize {
                match b {
                    b'a' | b'e' | b'i' | b'o' | b'u' => (base_samples * 14) / 10,
                    b'm' | b'n' | b'l' | b'r' => (base_samples * 11) / 10,
                    b' ' => base_samples / 2,
                    b'.' | b'!' | b'?' => base_samples,
                    b',' => (base_samples * 3) / 4,
                    _ => (base_samples * 8) / 10,
                }
            };
            let durations: Vec<usize> = ids
                .iter()
                .enumerate()
                .map(|(ti, _)| {
                    if ti == 0 || ti + 1 >= ids.len() {
                        base_samples / 2
                    } else {
                        norm_bytes
                            .get(ti - 1)
                            .copied()
                            .map(phoneme_dur)
                            .unwrap_or(base_samples)
                    }
                })
                .collect();
            let max_phonemes = 96.min(ids.len());
            let total: usize = durations.iter().take(max_phonemes).sum();
            let mut audio = vec![0i16; total];
            let mut phase = 0.0f32;
            let mut cursor = 0usize;
            // Contorno F0 leve (declinação).
            let n_ph = max_phonemes.max(1) as f32;
            for (ti, &id) in ids.iter().take(max_phonemes).enumerate() {
                let samples_per = durations[ti];
                let base = id * dim;
                if base + dim > ew_len {
                    cursor += samples_per;
                    continue;
                }
                let mut e = 0.0f32;
                let mut fsum = 0.0f32;
                for d in 0..dim {
                    let v = ew.data[base + d];
                    e += v * v;
                    if d < 8 {
                        fsum += v;
                    }
                }
                let amp = libm::sqrtf(libm::sqrtf(e / dim as f32)).clamp(0.05, 0.85);
                let decl = 1.0 - 0.12 * (ti as f32 / n_ph);
                let f0 = (145.0 + 70.0 * libm::sinf(fsum * 0.5)) * decl;
                for s in 0..samples_per {
                    phase += f0 / sr as f32;
                    let fade = (samples_per / 8).max(1);
                    let env = if s < fade {
                        s as f32 / fade as f32
                    } else if s > samples_per - fade {
                        (samples_per - s) as f32 / fade as f32
                    } else {
                        1.0
                    };
                    let sample = amp
                        * env
                        * (0.55 * libm::sinf(phase * 6.2831855)
                            + 0.28 * libm::sinf(phase * 12.566371)
                            + 0.12 * libm::sinf(phase * 18.849557)
                            + 0.05 * libm::sinf(phase * 25.132742));
                    let idx = cursor + s;
                    if idx < audio.len() {
                        audio[idx] = (sample * 22000.0).clamp(-32768.0, 32767.0) as i16;
                    }
                }
                cursor += samples_per;
            }
            if audio.iter().any(|&s| s != 0) {
                k_nano::slog_jarbas!("Audio", "piper", "neural-lite pcm_samples={} (prosody+duration)", audio.len());
                return audio;
            }
        }

        k_nano::slog_jarbas!("Audio", "piper", "neural-lite empty -> formant fallback");
        crate::audio::tts::synthesize(text)
    }
}
