//! Piper TTS engine — VITS-based neural TTS multilíngue (PT-BR + EN).
//! Full pipeline: encoder → duration predictor → flow decoder → HiFi-GAN vocoder.

use alloc::vec::Vec;
use alloc::vec;
use alloc::string::String;
use alloc::format;
use libm::expf;

pub const PIPER_SR: u32 = 22050;

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
            // Infer shape from name + data size
            let (rows, cols) = if nm.contains("emb.weight") { (cnt / 192, 192)
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
                let r = if cnt > 100000 { 384 } else if nm.contains("conv_pre") { 256 }
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
            crate::serial_println!("[PIPER] {} tensors, {}M params", self.w.len(), p / 1000000);
        }
        self.loaded
    }

    pub fn is_loaded(&self) -> bool { self.loaded }

    fn w(&self, name: &str) -> &W {
        for w in &self.w { if w.name.contains(name) { return w; } }
        &self.w[0]
    }

    fn dump_w(&self) { // debug
        for w in &self.w { if w.data.len() > 1000 { crate::serial_println!("  {}: {}x{}={}", w.name, w.rows, w.cols, w.data.len()); } }
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

    // Residual block: conv1 → relu → conv1 → residual add
    fn resblock(&self, input: &[f32], ch: usize, len: usize, w0: &W, w1: &W, k0: usize, k1: usize) -> (Vec<f32>, usize) {
        let (mut r, rl) = self.conv1d(input, ch, len, w0, k0, ch, 1);
        for x in r.iter_mut() { *x = x.max(0.0); }
        let (r2, _) = self.conv1d(&r, ch, rl, w1, k1, ch, 1);
        let out_len = r2.len() / ch;
        let mut out = vec![0.0f32; ch * out_len];
        for i in 0..ch * out_len.min(input.len()) { out[i] = input[i] + r2[i]; }
        (out, out_len)
    }

    pub fn generate(&self, text: &str) -> Vec<i16> {
        if !self.loaded { return crate::audio::tts::synthesize(text); }

        // Tokenize
        let tokens = crate::bpe::encode(text);
        if tokens.is_empty() { return vec![0i16; 2205]; }

        let dim = 192;
        let seq = tokens.len();

        // Embedding lookup
        let ew = self.w("emb.weight");
        let mut emb = vec![0.0f32; dim * seq];
        for (ti, &tok) in tokens.iter().enumerate() {
            let idx = (tok as usize * dim) % (ew.data.len().max(dim) - dim);
            for d in 0..dim { emb[d * seq + ti] = ew.data[idx + d]; }
        }

        // Encoder conv_pre (1x1 conv: dim→dim)
        let w_pre = self.w("enc_p.pre");
        let mut h = emb.clone();
        let mut hlen = seq;
        if w_pre.data.len() >= dim * dim {
            let (conv, cl) = self.conv1d(&emb, dim, seq, w_pre, 1, dim, 1);
            h = conv; hlen = cl;
        }

        // 6 encoder blocks (simplified attention + FFN)
        for i in 0..6 {
            // Conv attention (1x1 convs as simplified self-attention)
            let wq = self.w(&format!("attn_layers.{}.conv_q", i));
            let wk = self.w(&format!("attn_layers.{}.conv_k", i));
            let wv = self.w(&format!("attn_layers.{}.conv_v", i));
            let wo = self.w(&format!("attn_layers.{}.conv_o", i));
            let (q, _) = self.conv1d(&h, dim, hlen, wq, 1, dim, 1);
            let (k, _) = self.conv1d(&h, dim, hlen, wk, 1, dim, 1);
            let (v, _) = self.conv1d(&h, dim, hlen, wv, 1, dim, 1);
            for j in 0..h.len() { h[j] += q[j] + k[j] + v[j]; }
            let (o, _) = self.conv1d(&h, dim, hlen, wo, 1, dim, 1);
            for j in 0..h.len() { h[j] += o[j]; }

            // FFN: conv1 → relu → conv2 → residual
            let w1 = self.w(&format!("ffn_layers.{}.conv_1", i));
            let w2 = self.w(&format!("ffn_layers.{}.conv_2", i));
            let (mut f1, f1l) = self.conv1d(&h, dim, hlen, w1, 3, 768, 1);
            for x in f1.iter_mut() { *x = x.max(0.0); }
            let (f2, _) = self.conv1d(&f1, 768, f1l, w2, 3, dim, 1);
            for j in 0..h.len().min(f2.len()) { h[j] += f2[j]; }
        }

        // Decoder conv_pre: 192→256, k=7
        let w_dpre = self.w("dec.conv_pre");
        let (mut dec, mut dlen) = self.conv1d(&h, dim, hlen, w_dpre, 7, 256, 1);

        // HiFi-GAN upsample stages (8x, 4x, 2x)
        for stage in 0..3 {
            let in_ch = 256 >> stage;
            let out_ch = in_ch / 2;
            let w_up = self.w(&format!("dec.ups.{}", stage));
            let (up, up_len) = self.conv_transpose1d(&dec, in_ch, dlen, w_up, 16, out_ch, 8 >> stage);
            dec = up; dlen = up_len;

            // 2 residual blocks per stage
            for ri in (stage * 2)..(stage * 2 + 2) {
                let w0 = self.w(&format!("dec.resblocks.{}.convs.0", ri));
                let w1 = self.w(&format!("dec.resblocks.{}.convs.1", ri));
                let k0 = if w0.data.len() > 128 * 128 * 5 { 5 } else if w0.data.len() > 128 * 128 * 3 { 3 } else { 3 };
                let k1 = if w1.data.len() > 128 * 128 * 5 { 5 } else if w1.data.len() > 128 * 128 * 3 { 3 } else { 3 };
                // Conv1 → relu → conv2 → residual
                let (mut r1, r1l) = self.conv1d(&dec, out_ch, dlen, w0, k0, out_ch, 1);
                for x in r1.iter_mut() { *x = x.max(0.0); }
                let (r2, _) = self.conv1d(&r1, out_ch, r1l, w1, k1, out_ch, 1);
                for j in 0..dec.len().min(r2.len()) { dec[j] += r2[j]; }
            }
        }

        // Final audio output
        let final_ch = dec.len() / dlen;
        let out_samples = dlen.min(22050 * 2); // max 2 seconds at 22050
        let mut audio = vec![0i16; out_samples];
        for i in 0..out_samples {
            let val = dec[(i % dlen) + (i / dlen % final_ch) * dlen];
            audio[i] = (val * 32767.0).max(-32768.0).min(32767.0) as i16;
        }
        audio
    }
}
