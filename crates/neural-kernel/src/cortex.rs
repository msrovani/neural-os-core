use alloc::vec;
use alloc::vec::Vec;
use alloc::string::String;
use alloc::boxed::Box;
use core::f32::NEG_INFINITY;

pub const TOPIC_LLM_REQUEST: &str = "LLM_REQUEST";
pub const TOPIC_LLM_RESPONSE: &str = "LLM_RESPONSE";
pub const TOPIC_KERNEL_ERROR: &str = "KERNEL_ERROR";
pub const TOPIC_MODEL_UPDATE: &str = "MODEL_UPDATE";

pub static GLOBAL_MODEL_PARAMS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
use crate::nn::{silu, rms_norm};
use crate::tensor::{PackedTernaryTensor, Tensor};

const BOS: u16 = 0;
const EOS: u16 = 1;
const PAD: u16 = 2;
const CHAR_OFFSET: u16 = 3;
pub const VOCAB_SIZE: u16 = 99;
pub const MAX_SEQ: usize = 64;
const HIDDEN: usize = 64;
const NUM_LAYERS: usize = 4;
const NUM_HEADS: usize = 4;
const HEAD_DIM: usize = HIDDEN / NUM_HEADS;
const FFN_DIM: usize = HIDDEN * 2;

pub struct Tokenizer;

impl Tokenizer {
    pub fn encode(text: &str) -> Vec<u16> {
        let mut tokens = vec![BOS];
        for b in text.bytes() {
            if b >= 32 && b <= 126 {
                tokens.push((b - 32) as u16 + CHAR_OFFSET);
            }
        }
        tokens.push(EOS);
        tokens.truncate(MAX_SEQ);
        tokens
    }

    pub fn decode(tokens: &[u16]) -> alloc::string::String {
        let mut s = alloc::string::String::new();
        for &t in tokens {
            match t {
                BOS | PAD => continue,
                EOS => break,
                _ if t < VOCAB_SIZE => s.push((t - CHAR_OFFSET + 32) as u8 as char),
                _ => {}
            }
        }
        s
    }

    pub fn decode_char(t: u16) -> Option<char> {
        match t {
            BOS | PAD | EOS => None,
            _ if t < VOCAB_SIZE => Some((t - CHAR_OFFSET + 32) as u8 as char),
            _ => None,
        }
    }
}

fn softmax_inplace(logits: &mut [f32]) {
    let max = logits.iter().fold(NEG_INFINITY, |a, &b| a.max(b));
    let mut sum = 0.0;
    for v in logits.iter_mut() {
        *v = libm::expf(*v - max);
        sum += *v;
    }
    let inv = 1.0 / sum;
    for v in logits.iter_mut() { *v *= inv; }
}

fn rope_precompute(max_seq: usize, head_dim: usize, theta: f32) -> (Vec<f32>, Vec<f32>) {
    let half = head_dim / 2;
    let n = max_seq * half;
    let mut cos_table = vec![0.0f32; n];
    let mut sin_table = vec![0.0f32; n];
    for pos in 0..max_seq {
        for d in 0..half {
            let inv_freq = libm::powf(theta, -2.0 * d as f32 / head_dim as f32);
            let val = pos as f32 * inv_freq;
            cos_table[pos * half + d] = libm::cosf(val);
            sin_table[pos * half + d] = libm::sinf(val);
        }
    }
    (cos_table, sin_table)
}

fn rope_apply_heads(data: &mut [f32], seq_len: usize, num_heads: usize, head_dim: usize,
                    cos: &[f32], sin: &[f32], start_pos: usize) {
    let half = head_dim / 2;
    for s in 0..seq_len {
        let pos = start_pos + s;
        let base = s * num_heads * head_dim;
        let rope_off = pos * half;
        for h in 0..num_heads {
            let off = base + h * head_dim;
            for d in 0..half {
                let x = data[off + 2 * d];
                let y = data[off + 2 * d + 1];
                let c = cos[rope_off + d];
                let si = sin[rope_off + d];
                data[off + 2 * d] = x * c - y * si;
                data[off + 2 * d + 1] = x * si + y * c;
            }
        }
    }
}

pub struct LayerWeights {
    pub rms_attn: Vec<f32>,
    pub q: PackedTernaryTensor,
    pub k: PackedTernaryTensor,
    pub v: PackedTernaryTensor,
    pub o: PackedTernaryTensor,
    pub rms_ffn: Vec<f32>,
    pub rms_inner_attn: Vec<f32>,
    pub rms_ffn_norm: Vec<f32>,
    pub gate: PackedTernaryTensor,
    pub up: PackedTernaryTensor,
    pub down: PackedTernaryTensor,
    // GQA fields
    pub kv_dim: usize,
    pub num_kv_heads: usize,
    // BitFFN fields
    pub intermediate_size: usize,
    pub ffn_group_size: usize,
}

pub struct TransformerModel {
    pub embed: PackedTernaryTensor,
    pub layers: Vec<LayerWeights>,
    pub rms_final: Vec<f32>,
    pub unembed: PackedTernaryTensor,
    pub medusa_heads: Vec<MedusaHead>,
    pub vocab_size: u32,
    pub hidden: usize,
    pub num_layers: usize,
    pub max_seq: usize,
    // GQA fields
    pub num_heads: usize,
    pub num_kv_heads: usize,
    pub head_dim: usize,
    pub kv_dim: usize,
    // BitFFN fields
    pub intermediate_size: usize,
    pub ffn_group_size: usize,
    // Embedding tie flag
    pub tie_embeddings: bool,
    // RoPE
    pub rope_theta: f32,
    pub rope_cos: Vec<f32>,
    pub rope_sin: Vec<f32>,
}

/// Cache de Key/Value para geracao autoregressiva.
/// Armazena K e V por layer, evitando reprocessar tokens anteriores.
pub struct KvCache {
    pub k: Vec<Vec<f32>>,
    pub v: Vec<Vec<f32>>,
    pub len: usize,
    k_dim: usize,
    kv_dim: usize,
}

impl KvCache {
    pub fn new(num_layers: usize, k_dim: usize, kv_dim: usize) -> Self {
        KvCache {
            k: (0..num_layers).map(|_| Vec::new()).collect(),
            v: (0..num_layers).map(|_| Vec::new()).collect(),
            len: 0, k_dim, kv_dim,
        }
    }

    pub fn append(&mut self, layer: usize, k_new: &Tensor, v_new: &Tensor) {
        self.k[layer].extend_from_slice(&k_new.data);
        self.v[layer].extend_from_slice(&v_new.data);
    }

    pub fn advance(&mut self, n: usize) {
        self.len += n;
    }

    pub fn k_all(&self, layer: usize, seq_len: usize) -> Tensor {
        let data = self.k[layer].clone();
        let expected = seq_len * self.k_dim;
        if data.len() != expected {
            crate::serial_println!("[KV] k_all mismatch: layer={} data.len={} expected={} (seq={} k_dim={})",
                layer, data.len(), expected, seq_len, self.k_dim);
        }
        Tensor::from_row_major((seq_len, self.k_dim), data).unwrap()
    }

    pub fn v_all(&self, layer: usize, seq_len: usize) -> Tensor {
        let data = self.v[layer].clone();
        let expected = seq_len * self.k_dim;
        if data.len() != expected {
            crate::serial_println!("[KV] v_all mismatch: layer={} data.len={} expected={} (seq={} k_dim={})",
                layer, data.len(), expected, seq_len, self.k_dim);
        }
        Tensor::from_row_major((seq_len, self.k_dim), data).unwrap()
    }

    pub fn len(&self) -> usize { self.len }
}

const MEDUSA_HEADS: usize = 3;

pub struct MedusaHead {
    pub w: PackedTernaryTensor,
}

impl MedusaHead {
    pub fn new_random(seed: &mut u32, hidden: usize, vocab: usize) -> Self {
        MedusaHead { w: random_ternary(seed, hidden, vocab) }
    }

    pub fn forward(&self, hidden: &Tensor) -> Tensor {
        self.w.matmul_hybrid(hidden).unwrap()
    }
}

pub fn random_ternary(seed: &mut u32, rows: usize, cols: usize) -> PackedTernaryTensor {
    let packed_len = (rows * cols + 3) / 4;
    let mut packed = vec![0u8; packed_len];
    for (_i, byte) in packed.iter_mut().enumerate() {
        let mut b = 0u8;
        for j in 0..4 {
            *seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            let r = (*seed % 3) as i8;
            let v = if r == 2 { -1i8 } else { r };
            let bits = match v {
                -1 => 0b10,
                0 => 0b00,
                1 => 0b01,
                _ => 0b00,
            };
            b |= bits << (j * 2);
        }
        *byte = b;
    }
    PackedTernaryTensor { shape: (rows, cols), packed_data: packed }
}

impl TransformerModel {
    pub fn new() -> Self {
        let mut seed: u32 = 42;
        let mut layers = Vec::with_capacity(NUM_LAYERS);
        let rms_default: Vec<f32> = vec![1.0; HIDDEN];
        for _ in 0..NUM_LAYERS {
            layers.push(LayerWeights {
                rms_attn: rms_default.clone(),
                q: random_ternary(&mut seed, HIDDEN, HIDDEN),
                k: random_ternary(&mut seed, HIDDEN, HIDDEN),
                v: random_ternary(&mut seed, HIDDEN, HIDDEN),
                o: random_ternary(&mut seed, HIDDEN, HIDDEN),
                rms_ffn: rms_default.clone(),
                rms_inner_attn: rms_default.clone(),
                rms_ffn_norm: vec![1.0; FFN_DIM * 2],
                gate: random_ternary(&mut seed, HIDDEN, FFN_DIM),
                up: random_ternary(&mut seed, HIDDEN, FFN_DIM),
                down: random_ternary(&mut seed, FFN_DIM, HIDDEN),
                kv_dim: HIDDEN,
                num_kv_heads: NUM_HEADS,
                intermediate_size: FFN_DIM,
                ffn_group_size: FFN_DIM,
            });
        }
        let (rope_cos, rope_sin) = rope_precompute(MAX_SEQ, HEAD_DIM, 10000.0);
        let medusa_heads = (0..MEDUSA_HEADS).map(|_| MedusaHead::new_random(&mut seed, HIDDEN, VOCAB_SIZE as usize)).collect();
        TransformerModel {
            embed: random_ternary(&mut seed, HIDDEN, VOCAB_SIZE as usize),
            layers,
            rms_final: rms_default,
            unembed: random_ternary(&mut seed, HIDDEN, VOCAB_SIZE as usize),
            medusa_heads,
            vocab_size: VOCAB_SIZE as u32,
            hidden: HIDDEN,
            num_layers: NUM_LAYERS,
            max_seq: MAX_SEQ,
            num_heads: NUM_HEADS,
            num_kv_heads: NUM_HEADS,
            head_dim: HEAD_DIM,
            kv_dim: HIDDEN,
            intermediate_size: FFN_DIM,
            ffn_group_size: FFN_DIM,
            tie_embeddings: false,
            rope_theta: 10000.0,
            rope_cos,
            rope_sin,
        }
    }

    fn embed_lookup(&self, token: u16) -> Tensor {
        let t = (token as usize).min(self.embed.shape.1 - 1);
        let mut data = Vec::with_capacity(self.hidden);
        for row in 0..self.hidden {
            let idx = row * self.embed.shape.1 + t;
            data.push(self.embed.get_weight(idx) as f32);
        }
        Tensor::from_row_major((1, self.hidden), data).unwrap()
    }

    fn rms_norm_tensor(&self, x: &Tensor, weight: &[f32]) -> Tensor {
        let mut t = Tensor::from_row_major(x.shape, x.data.clone()).unwrap();
        rms_norm(&mut t, weight, 1e-6);
        t
    }

    pub fn forward_with_kv(&self, tokens: &[u16], cache: &mut KvCache) -> (Tensor, Tensor) {
        let seq_len = tokens.len();
        let is_first_pass = cache.len == 0;
        let new_len = if is_first_pass { seq_len.min(self.max_seq) } else { seq_len };
        let total_seq = if is_first_pass { new_len } else { cache.len + seq_len };

        // Embed only the new tokens
        let start_pos = if is_first_pass { 0 } else { cache.len };
        let mut x = Tensor::new((new_len, self.hidden));
        for (i, &t) in tokens.iter().enumerate().take(new_len) {
            let emb = self.embed_lookup(t);
            for j in 0..self.hidden {
                x.data[i * self.hidden + j] = emb.data[j];
            }
        }

        // Causal mask for the new tokens over the full sequence
        let mut mask_data = vec![0.0f32; new_len * total_seq];
        for i in 0..new_len {
            let global_i = start_pos + i;
            for j in (global_i + 1)..total_seq {
                mask_data[i * total_seq + j] = NEG_INFINITY;
            }
        }
        let mask = Tensor::from_row_major((new_len, total_seq), mask_data).unwrap();

        let _layer_count = self.layers.len();
        for (layer_idx, layer) in self.layers.iter().enumerate() {
            let norm = self.rms_norm_tensor(&x, &layer.rms_attn);

            // QKV for new tokens
            let mut q = layer.q.matmul_hybrid(&norm).unwrap();
            let mut k = layer.k.matmul_hybrid(&norm).unwrap();
            let v = layer.v.matmul_hybrid(&norm).unwrap();

            // RoPE on Q and K before cache storage
            let qk_head_dim = self.kv_dim / self.num_heads;
            rope_apply_heads(&mut q.data, new_len, self.num_heads, qk_head_dim,
                &self.rope_cos, &self.rope_sin, start_pos);
            rope_apply_heads(&mut k.data, new_len, self.num_kv_heads, qk_head_dim,
                &self.rope_cos, &self.rope_sin, start_pos);

            // Append new K,V to cache (K is RoPE-rotated)
            cache.append(layer_idx, &k, &v);

            // Advance cache.len only once after all layers
            if layer_idx + 1 == self.layers.len() {
                cache.advance(new_len);
            }

            // Full K,V from cache for attention
            let total_k = cache.k_all(layer_idx, total_seq);
            let total_v = cache.v_all(layer_idx, total_seq);

            // GQA attention
            let num_heads = self.num_heads;
            let num_kv_heads = self.num_kv_heads;
            let kv_dim = self.kv_dim;
            let q_group_size = num_heads / num_kv_heads;
            let k_dim = total_k.shape.1;
            let v_dim = total_v.shape.1;
            let mut attn_out_data = vec![0.0f32; new_len * kv_dim];

            for kv_g in 0..num_kv_heads {
                let kv_start = kv_g * qk_head_dim;
                let mut k_g = Tensor::new((total_seq, qk_head_dim));
                let mut v_g = Tensor::new((total_seq, qk_head_dim));
                for s in 0..total_seq {
                    for d in 0..qk_head_dim {
                        let kd = kv_start + d;
                        if kd < k_dim { k_g.data[s * qk_head_dim + d] = total_k.data[s * k_dim + kd]; }
                        if kd < v_dim { v_g.data[s * qk_head_dim + d] = total_v.data[s * v_dim + kd]; }
                    }
                }

                for qh in 0..q_group_size {
                    let head_idx = kv_g * q_group_size + qh;
                    let head_start = head_idx * qk_head_dim;
                    let mut q_h = Tensor::new((new_len, qk_head_dim));
                    for s in 0..new_len {
                        for d in 0..qk_head_dim {
                            q_h.data[s * qk_head_dim + d] = q.data[s * kv_dim + head_start + d];
                        }
                    }

                    let k_g_t = k_g.transposed();
                    let mut scores = q_h.matmul(&k_g_t).unwrap();
                    let scale = 1.0 / libm::sqrtf(qk_head_dim as f32);
                    for s in scores.data.iter_mut() { *s *= scale; }
                    for i in 0..new_len {
                        for j in 0..total_seq {
                            scores.data[i * total_seq + j] += mask.data[i * total_seq + j];
                        }
                    }
                    for i in 0..new_len {
                        let start = i * total_seq;
                        softmax_inplace(&mut scores.data[start..start + total_seq]);
                    }
                    let attn_h = scores.matmul(&v_g).unwrap();
                    for s in 0..new_len {
                        for d in 0..qk_head_dim {
                            attn_out_data[s * kv_dim + head_start + d] = attn_h.data[s * qk_head_dim + d];
                        }
                    }
                }
            }

            let attn_out = Tensor::from_row_major((new_len, kv_dim), attn_out_data).unwrap();
            let attn_out_norm = self.rms_norm_tensor(&attn_out, &layer.rms_inner_attn);
            let proj = layer.o.matmul_hybrid(&attn_out_norm).unwrap();
            x = x.add(&proj).unwrap();

            // BitFFN
            let norm2 = self.rms_norm_tensor(&x, &layer.rms_ffn);
            let gate = layer.gate.matmul_hybrid(&norm2).unwrap();
            let up = layer.up.matmul_hybrid(&norm2).unwrap();
            let ffn_group = gate.shape.1;
            let mut gated = Tensor::from_row_major(gate.shape, gate.data.clone()).unwrap();
            for (i, g) in gated.data.iter_mut().enumerate() { *g = silu(*g) * up.data[i]; }

            let intermediate_size = layer.intermediate_size;
            let down_out = layer.down.shape.1;
            let num_groups = intermediate_size / ffn_group;
            let mut gated_full = Tensor::new((new_len, intermediate_size));
            for s in 0..new_len {
                for g in 0..num_groups {
                    let g_off = g * ffn_group;
                    for d in 0..ffn_group {
                        gated_full.data[s * intermediate_size + g_off + d] = gated.data[s * ffn_group + d];
                    }
                }
            }

            let gated_norm = self.rms_norm_tensor(&gated_full, &layer.rms_ffn_norm);
            let down = layer.down.matmul_hybrid(&gated_norm).unwrap();
            for s in 0..new_len {
                for d in 0..down_out.min(self.hidden) {
                    x.data[s * self.hidden + d] += down.data[s * down_out + d];
                }
            }
        }

        let final_norm = self.rms_norm_tensor(&x, &self.rms_final);
        let last_hidden = Tensor::from_row_major((1, self.hidden),
            final_norm.data[(new_len - 1) * self.hidden..new_len * self.hidden].to_vec()).unwrap();
        let logits = if self.tie_embeddings {
            self.embed.matmul_hybrid(&last_hidden).unwrap()
        } else {
            self.unembed.matmul_hybrid(&last_hidden).unwrap()
        };
        (last_hidden, logits)
    }

    pub fn forward_hidden(&self, tokens: &[u16]) -> (Tensor, Tensor) {
        let seq_len = tokens.len().min(self.max_seq);
        let num_heads = self.num_heads;
        let num_kv_heads = self.num_kv_heads;
        let qk_head_dim = self.kv_dim / num_heads; // 32 for BitNet-b1.58
        let kv_dim = self.kv_dim;
        let q_group_size = num_heads / num_kv_heads; // 4 Q heads per KV head
        let mut x = Tensor::new((seq_len, self.hidden));
        for (i, &t) in tokens.iter().enumerate().take(seq_len) {
            let emb = self.embed_lookup(t);
            for j in 0..self.hidden {
                x.data[i * self.hidden + j] = emb.data[j];
            }
        }

        let mut mask_data = vec![0.0f32; seq_len * seq_len];
        for i in 0..seq_len {
            for j in (i + 1)..seq_len {
                mask_data[i * seq_len + j] = NEG_INFINITY;
            }
        }
        let mask = Tensor::from_row_major((seq_len, seq_len), mask_data).unwrap();

        let layer_count = self.layers.len();
        for (layer_idx, layer) in self.layers.iter().enumerate() {
            let lt0 = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            let norm = self.rms_norm_tensor(&x, &layer.rms_attn);

            // QKV projections with GQA dimensions
            let t_q0 = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            let mut q = layer.q.matmul_hybrid(&norm).unwrap();  // (seq, kv_dim)
            let t_q1 = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            let mut k = layer.k.matmul_hybrid(&norm).unwrap();  // (seq, k_dim)
            let _t_k1 = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            let v = layer.v.matmul_hybrid(&norm).unwrap();  // (seq, k_dim)
            let t_v1 = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);

            // RoPE on Q and K
            rope_apply_heads(&mut q.data, seq_len, num_heads, qk_head_dim,
                &self.rope_cos, &self.rope_sin, 0);
            rope_apply_heads(&mut k.data, seq_len, num_kv_heads, qk_head_dim,
                &self.rope_cos, &self.rope_sin, 0);

            // GQA attention: each KV head serves q_group_size query heads
            let k_dim = k.shape.1;
            let v_dim = v.shape.1;
            let mut attn_out_data = vec![0.0f32; seq_len * kv_dim];

            for kv_g in 0..num_kv_heads {
                let kv_start = kv_g * qk_head_dim;
                // Extract K and V for this KV group
                let mut k_g = Tensor::new((seq_len, qk_head_dim));
                let mut v_g = Tensor::new((seq_len, qk_head_dim));
                for s in 0..seq_len {
                    for d in 0..qk_head_dim {
                        let kd = kv_start + d;
                        if kd < k_dim {
                            k_g.data[s * qk_head_dim + d] = k.data[s * k_dim + kd];
                        }
                        if kd < v_dim {
                            v_g.data[s * qk_head_dim + d] = v.data[s * v_dim + kd];
                        }
                    }
                }

                for qh in 0..q_group_size {
                    let head_idx = kv_g * q_group_size + qh;
                    let head_start = head_idx * qk_head_dim;
                    // Extract Q for this head
                    let mut q_h = Tensor::new((seq_len, qk_head_dim));
                    for s in 0..seq_len {
                        for d in 0..qk_head_dim {
                            q_h.data[s * qk_head_dim + d] = q.data[s * kv_dim + head_start + d];
                        }
                    }

                    // scores = q_h @ k_g.T
                    let k_g_t = k_g.transposed();
                    let mut scores = q_h.matmul(&k_g_t).unwrap();
                    let scale = 1.0 / libm::sqrtf(qk_head_dim as f32);
                    for s in scores.data.iter_mut() { *s *= scale; }
                    // Mask + softmax
                    for i in 0..seq_len {
                        for j in 0..seq_len {
                            scores.data[i * seq_len + j] += mask.data[i * seq_len + j];
                        }
                    }
                    for i in 0..seq_len {
                        let start = i * seq_len;
                        softmax_inplace(&mut scores.data[start..start + seq_len]);
                    }
                    // attn_out_h = scores @ v_g
                    let attn_h = scores.matmul(&v_g).unwrap();
                    // Write to output
                    for s in 0..seq_len {
                        for d in 0..qk_head_dim {
                            attn_out_data[s * kv_dim + head_start + d] = attn_h.data[s * qk_head_dim + d];
                        }
                    }
                }
            }

            let t_attn1 = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);

            let attn_out = Tensor::from_row_major((seq_len, kv_dim), attn_out_data).unwrap();
            let attn_out_norm = self.rms_norm_tensor(&attn_out, &layer.rms_inner_attn);
            let proj = layer.o.matmul_hybrid(&attn_out_norm).unwrap();  // (seq, kv_dim) @ (kv_dim, hidden) = (seq, hidden)
            x = x.add(&proj).unwrap();
            let t_proj1 = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);

            // BitFFN
            let norm2 = self.rms_norm_tensor(&x, &layer.rms_ffn);
            let gate = layer.gate.matmul_hybrid(&norm2).unwrap();  // (seq, ffn_group_size)
            let up = layer.up.matmul_hybrid(&norm2).unwrap();      // (seq, ffn_group_size)
            let t_ffn1 = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            let ffn_group = gate.shape.1;
            let mut gated = Tensor::from_row_major(gate.shape, gate.data.clone()).unwrap();
            for (i, g) in gated.data.iter_mut().enumerate() {
                *g = silu(*g) * up.data[i];
            }

            // Expand gated by repeating 4x for full intermediate dim
            // gated: (seq, ffn_group) -> expand -> (seq, intermediate_size)
            let intermediate_size = layer.intermediate_size;
            let down_out = layer.down.shape.1; // kv_dim for BitNet
            let num_groups = intermediate_size / ffn_group;
            let mut gated_full = Tensor::new((seq_len, intermediate_size));
            for s in 0..seq_len {
                for g in 0..num_groups {
                    let g_off = g * ffn_group;
                    for d in 0..ffn_group {
                        gated_full.data[s * intermediate_size + g_off + d] = gated.data[s * ffn_group + d];
                    }
                }
            }

            let gated_norm = self.rms_norm_tensor(&gated_full, &layer.rms_ffn_norm);
            let down = layer.down.matmul_hybrid(&gated_norm).unwrap();  // (seq, intermediate) @ (intermediate, down_out) = (seq, down_out)

            // Add FFN output to residual (first down_out dims)
            for s in 0..seq_len {
                for d in 0..down_out.min(self.hidden) {
                    x.data[s * self.hidden + d] += down.data[s * down_out + d];
                }
            }

            let lt1 = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            if layer_idx == 0 {
                crate::serial_println!("[FWD] L0 qkv:{} attn:{} proj:{} ffn_gateup:{} down:{} total:{}",
                    t_q1 - t_q0, t_attn1 - t_v1, t_proj1 - t_attn1, t_ffn1 - t_proj1, lt1 - t_ffn1, lt1 - lt0);
            }
            if lt1 - lt0 > 5 || layer_idx == 0 || layer_idx + 1 == layer_count {
                crate::serial_println!("[FWD] layer {}/{}: {} ticks", layer_idx + 1, layer_count, lt1 - lt0);
            }
        }

        let final_norm = self.rms_norm_tensor(&x, &self.rms_final);
        let last_hidden = Tensor::from_row_major((1, self.hidden),
            final_norm.data[(seq_len - 1) * self.hidden..seq_len * self.hidden].to_vec()).unwrap();
        let t_unembed0 = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
        let logits = if self.tie_embeddings {
            self.embed.matmul_hybrid(&last_hidden).unwrap()
        } else {
            self.unembed.matmul_hybrid(&last_hidden).unwrap()
        };
        let t_unembed1 = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
        if t_unembed1 - t_unembed0 > 10 {
            crate::serial_println!("[FWD] unembed: {} ticks", t_unembed1 - t_unembed0);
        }
        (last_hidden, logits)
    }

    pub fn forward(&self, tokens: &[u16]) -> Tensor {
        self.forward_hidden(tokens).1
    }

pub fn generate_next(&self, tokens: &[u16]) -> u16 {
    let logits = self.forward(tokens);
    argmax_row(&logits, 0)
}

pub fn sample(&self, tokens: &[u16], top_k: usize, temperature: f32) -> u16 {
    let logits = self.forward(tokens);
    let mut probs: Vec<(usize, f32)> = logits.data.iter().enumerate()
        .map(|(i, &v)| (i, v / temperature.max(0.01))).collect();

    if top_k > 0 && top_k < probs.len() {
        probs.select_nth_unstable_by(top_k - 1, |a, b| b.1.partial_cmp(&a.1).unwrap());
        probs.truncate(top_k);
    }
    let max_logit = probs.iter().map(|(_, v)| *v).fold(NEG_INFINITY, |a, b| a.max(b));
    let mut sum = 0.0f32;
    for (_, v) in probs.iter_mut() { *v = libm::expf(*v - max_logit); sum += *v; }
    let mut r = (sum * 0.5 + 0.5).max(0.0).min(sum); // deterministic for no_std
    for &(idx, prob) in &probs {
        let p = prob / sum;
        r -= p;
        if r <= 0.0 { return idx as u16; }
    }
    argmax_row(&logits, 0)
}
}

fn read_f32(data: &[u8], offset: &mut usize) -> Option<f32> {
    if *offset + 4 > data.len() { return None; }
    let bytes = data[*offset..*offset + 4].try_into().ok()?;
    *offset += 4;
    Some(f32::from_le_bytes(bytes))
}

fn read_u16(data: &[u8], offset: &mut usize) -> Option<u16> {
    if *offset + 2 > data.len() { return None; }
    let bytes = data[*offset..*offset + 2].try_into().ok()?;
    *offset += 2;
    Some(u16::from_le_bytes(bytes))
}

fn read_u8(data: &[u8], offset: &mut usize) -> Option<u8> {
    if *offset + 1 > data.len() { return None; }
    let v = data[*offset];
    *offset += 1;
    Some(v)
}

fn read_u32(data: &[u8], offset: &mut usize) -> Option<u32> {
    if *offset + 4 > data.len() { return None; }
    let bytes = data[*offset..*offset + 4].try_into().ok()?;
    *offset += 4;
    Some(u32::from_le_bytes(bytes))
}

fn read_ternary_tensor(data: &[u8], offset: &mut usize, rows: usize, cols: usize) -> Option<PackedTernaryTensor> {
    let count = (rows * cols + 3) / 4;
    if *offset + count > data.len() { return None; }
    let packed = data[*offset..*offset + count].to_vec();
    *offset += count;
    Some(PackedTernaryTensor { shape: (rows, cols), packed_data: packed })
}

fn read_f32_tensor(data: &[u8], offset: &mut usize, rows: usize, cols: usize) -> Option<Tensor> {
    let count = rows * cols;
    if *offset + count * 4 > data.len() { return None; }
    let mut raw = Vec::with_capacity(count);
    for _ in 0..count {
        raw.push(read_f32(data, offset)?);
    }
    Tensor::from_row_major((rows, cols), raw)
}

fn read_f32_vec(data: &[u8], offset: &mut usize, n: usize) -> Option<Vec<f32>> {
    if *offset + n * 4 > data.len() { return None; }
    let mut v = Vec::with_capacity(n);
    for _ in 0..n {
        v.push(read_f32(data, offset)?);
    }
    Some(v)
}

pub fn load_model(data: &[u8]) -> Option<TransformerModel> {
    let mut off = 0;
    let magic = read_u32(data, &mut off)?;
    if magic != 0xBE11BE11 { return None; }
    let version = read_u16(data, &mut off)?;
    let _num_params = read_u32(data, &mut off)?;
    let hidden = read_u16(data, &mut off)? as usize;
    let num_layers = read_u16(data, &mut off)? as usize;
    // Auto-expand heap based on header (before main parsing)
    {
        let _nh = read_u16(data, &mut off)? as usize;
        let vs = read_u32(data, &mut off)? as usize;
        let _ms = read_u16(data, &mut off)? as usize;
        let isize = read_u16(data, &mut off)? as usize;
        let embed_bytes = (hidden * vs / 4) as u64;
        // 4 aten tensors (q,k,v,o: hidden² each) + 3 FFN tensors (gate,up,down: hidden×isize each)
        let layer_bytes = (4u64 * hidden as u64 * hidden as u64 / 4 + 3u64 * hidden as u64 * isize as u64 / 4) * num_layers as u64;
        let unembed_bytes = (hidden as u64 * vs as u64 / 4) as u64;
        let estimated = ((embed_bytes + layer_bytes + unembed_bytes) / (1024 * 1024)) as usize;
        let cur_mb = crate::allocator::CURRENT_HEAP_MB.load(core::sync::atomic::Ordering::Relaxed);
        if estimated > cur_mb {
            let total_mb = estimated + (estimated / 4).max(64);
            crate::allocator::resize_heap_to_mb(total_mb);
        }
    }
    // Reset offset past magic+version+num_params+hidden+num_layers for main parsing
    off = 4 + 2 + 4 + 2 + 2;
    let num_heads = read_u16(data, &mut off)? as usize;
    let vocab_size = read_u32(data, &mut off)?;
    let max_seq = read_u16(data, &mut off)?;

    // v3: interleaved GQA/BitFFN fields
    // v2: ffn_dim (grouped)
    // v1: no ffn_dim
    let mut intermediate_size = hidden * 4;
    let mut num_kv_heads = num_heads;
    let mut num_medusa = 0usize;
    let mut tie_embeddings = false;

    if version >= 3 {
        intermediate_size = read_u16(data, &mut off)? as usize;
        num_kv_heads = read_u16(data, &mut off)? as usize;
        let q_dim = read_u16(data, &mut off)? as usize;  // Q projection output dim
        num_medusa = read_u32(data, &mut off)? as usize;
        // v3.1: tie_word_embeddings flag (4 bytes)
        if off + 4 <= data.len() {
            tie_embeddings = &data[off..off + 4] == b"TIED";
        }
        off += 4;
        let _tok_type = if off < data.len() { data[off] } else { 0 }; off += 1;
        let tok_len = read_u32(data, &mut off)? as usize;
        if tok_len > 0 && off + tok_len <= data.len() {
            let tok_data = &data[off..off + tok_len];
            let first = if tok_len >= 8 { &tok_data[..8] } else { tok_data };
            crate::serial_println!("[BPE] Tokenizer data: {} bytes, starts {:02x?}", tok_len, first);
            // BPE tokenizer skipped for v3 (large tokenizer needs proper JSON parser)
        }
        off += tok_len;

        // v4: layer_features byte (bit 0 = inner_attn_ln, bit 1 = ffn_layernorm, bit 2 = RoPE)
        let layer_features = if version >= 4 { read_u8(data, &mut off)? } else { 0u8 };
        let has_inner_attn_ln = (layer_features & 0x01) != 0;
        let has_ffn_layernorm = (layer_features & 0x02) != 0;
        let has_rope = (layer_features & 0x04) != 0;

        let embed = read_ternary_tensor(data, &mut off, hidden, vocab_size as usize)?;

        // GQA/BitFFN dimensions from header
        let kv_head_dim = q_dim / num_heads;                  // 640/20 = 32
        let k_dim = num_kv_heads * kv_head_dim;              // 5*32 = 160
        let ffn_group = intermediate_size * q_dim / hidden;  // 6912*640/2560 = 1728
        let down_out = q_dim;                                 // 640

        let mut layers = Vec::with_capacity(num_layers);
        for _ in 0..num_layers {
            let rms_attn = read_f32_vec(data, &mut off, hidden)?;
            let rms_ffn = read_f32_vec(data, &mut off, hidden)?;
            let rms_inner_attn = if has_inner_attn_ln {
                read_f32_vec(data, &mut off, kv_head_dim * num_heads)?
            } else {
                vec![1.0; kv_head_dim * num_heads]
            };
            let rms_ffn_norm = if has_ffn_layernorm {
                read_f32_vec(data, &mut off, intermediate_size)?
            } else {
                vec![1.0; intermediate_size]
            };
            layers.push(LayerWeights {
                rms_attn,
                q: read_ternary_tensor(data, &mut off, hidden, q_dim)?,
                k: read_ternary_tensor(data, &mut off, hidden, k_dim)?,
                v: read_ternary_tensor(data, &mut off, hidden, k_dim)?,
                o: read_ternary_tensor(data, &mut off, q_dim, hidden)?,
                rms_ffn,
                rms_inner_attn,
                rms_ffn_norm,
                gate: read_ternary_tensor(data, &mut off, hidden, ffn_group)?,
                up: read_ternary_tensor(data, &mut off, hidden, ffn_group)?,
                down: read_ternary_tensor(data, &mut off, intermediate_size, down_out)?,
                kv_dim: q_dim,
                num_kv_heads,
                intermediate_size,
                ffn_group_size: ffn_group,
            });
        }

        // v3: rms_final may not be present (tied models skip it)
        let rms_final = if off + hidden * 4 <= data.len() {
            read_f32_vec(data, &mut off, hidden)?
        } else {
            vec![1.0; hidden]
        };

        // v3: unembed may be absent (tie_word_embeddings).
        // Try to read unembed from file. If tie_embeddings flag was set (header) or the data
        // reads as all-zero (past end of actual file in QEMU device-loader memory region),
        // allocate a zero tensor and mark tie_embeddings.
        let expected = (hidden * vocab_size as usize + 3) / 4;
        let unembed = if !tie_embeddings && off + expected <= data.len() {
            // Check first 16 bytes are non-zero (zero = past file end = tied)
            let is_zeroed = data[off..(off + 16).min(data.len())].iter().all(|&b| b == 0);
            if is_zeroed {
                tie_embeddings = true;
                PackedTernaryTensor { shape: (hidden, vocab_size as usize), packed_data: vec![0u8; expected] }
            } else {
                read_ternary_tensor(data, &mut off, hidden, vocab_size as usize)?
            }
        } else {
            tie_embeddings = true;
            PackedTernaryTensor { shape: (hidden, vocab_size as usize), packed_data: vec![0u8; expected] }
        };

        let mut medusa_heads = Vec::with_capacity(num_medusa);
        if num_medusa > 0 {
            for _ in 0..num_medusa {
                let w = read_ternary_tensor(data, &mut off, hidden, vocab_size as usize)?;
                medusa_heads.push(MedusaHead { w });
            }
        }

        let (rope_cos, rope_sin) = if has_rope {
            let rope_theta_val = read_f32(data, &mut off)?;
            rope_precompute(max_seq as usize, kv_head_dim, rope_theta_val)
        } else {
            (vec![], vec![])
        };

        let model = TransformerModel {
            embed, layers, rms_final, unembed, medusa_heads,
            vocab_size, hidden, num_layers, max_seq: max_seq as usize,
            num_heads, num_kv_heads, head_dim: kv_head_dim, kv_dim: q_dim,
            intermediate_size,
            ffn_group_size: ffn_group,
            tie_embeddings,
            rope_theta: 10000.0,
            rope_cos, rope_sin,
        };
        GLOBAL_MODEL_PARAMS.store(_num_params as u64, core::sync::atomic::Ordering::Relaxed);
        return Some(model);
    } else if version >= 2 {
        let ffn_dim = read_u16(data, &mut off)? as usize;
        intermediate_size = ffn_dim * 4; // assume 4 groups for v2 BitFFN
        num_kv_heads = if num_heads > 0 { num_heads / 4 } else { num_heads };
        num_medusa = {
            let mut n = 0usize;
            if off + 4 > data.len() { return None; }
            n = data[off] as usize; off += 4;
            n
        };
        let _tok_type = if off < data.len() { data[off] } else { 0 }; off += 1;
        let tok_len = read_u32(data, &mut off)? as usize;
        if tok_len > 0 && off + tok_len <= data.len() {
            let _tok_data = &data[off..off + tok_len];
        }
        off += tok_len;
    } else {
        // v1: no tokenizer, no ffn_dim, no medusa
        num_medusa = 0;
    }

    let embed = read_f32_tensor(data, &mut off, vocab_size as usize, hidden)?;

    // Compute GQA dimensions
    let head_dim = hidden / num_heads.max(1);
    let kv_dim = num_kv_heads * head_dim; // = 5*128 = 640, but actual k_proj out is 160
    // For BitNet, the per-head KV dim is smaller: k_per_head = 32
    // qk_head_dim = kv_dim / num_kv_heads = head_dim (standard)
    // But BitNet uses qk_head_dim = 32, making kv_dim = num_kv_heads * 32 = 160
    // Let's read actual tensor sizes from the byte stream

    let mut layers = Vec::with_capacity(num_layers);
    for _ in 0..num_layers {
        let rms_attn = if version >= 2 {
            read_f32_vec(data, &mut off, hidden)?
        } else {
            let s = read_f32(data, &mut off)?;
            vec![s; hidden]
        };
        let rms_ffn = if version >= 2 {
            read_f32_vec(data, &mut off, hidden)?
        } else {
            let s = read_f32(data, &mut off)?;
            vec![s; hidden]
        };

        // Determine tensor sizes by reading from the byte stream
        // v3: transposed (in, out) layout
        // Q: (hidden, q_dim) where q_dim = kv_dim standard, 640 for BitNet
        // K: (hidden, k_dim) where k_dim = kv_head_dim = 160 for BitNet
        // V: (hidden, k_dim)
        // O: (q_dim, hidden)

        if version >= 3 {
            // Read the actual tensor shapes from context
            // For BitNet-b1.58-2B-4T: q_dim=640, k_dim=160, ffn_group=1728, down_out=640
            // We infer q_dim from num_heads * qk_head_dim where qk_head_dim = 32 (BitNet specific)
            let qk_head_dim = if num_kv_heads > 0 && kv_dim / num_kv_heads > 1 {
                // Try to use the stored kv_dim / num_kv_heads for per-head KV dim
                32 // BitNet uses 32; fallback for other models
            } else {
                32
            };
            let q_dim = num_heads * qk_head_dim;
            let k_dim = num_kv_heads * qk_head_dim;
            let ffn_group = intermediate_size / 4; // default: 4 groups
            let down_out = q_dim; // BitNet: down projects to kv_dim (same as q_dim)

            let rms_inner_attn = vec![1.0; q_dim];
            let rms_ffn_norm = vec![1.0; intermediate_size];
            layers.push(LayerWeights {
                rms_attn,
                q: read_ternary_tensor(data, &mut off, hidden, q_dim)?,
                k: read_ternary_tensor(data, &mut off, hidden, k_dim)?,
                v: read_ternary_tensor(data, &mut off, hidden, k_dim)?,
                o: read_ternary_tensor(data, &mut off, q_dim, hidden)?,
                rms_ffn,
                rms_inner_attn,
                rms_ffn_norm,
                gate: read_ternary_tensor(data, &mut off, hidden, ffn_group)?,
                up: read_ternary_tensor(data, &mut off, hidden, ffn_group)?,
                down: read_ternary_tensor(data, &mut off, intermediate_size, down_out)?,
                kv_dim: q_dim,
                num_kv_heads,
                intermediate_size,
                ffn_group_size: ffn_group,
            });
        } else {
            // v1/v2: non-transposed layout, (out, in)
            // For backward compat, keep old format
            let ffn_dim = intermediate_size / 4;
            let rms_inner_attn = vec![1.0; hidden];
            let rms_ffn_norm = vec![1.0; ffn_dim * 4];
            layers.push(LayerWeights {
                rms_attn,
                q: read_ternary_tensor(data, &mut off, hidden, hidden)?,
                k: read_ternary_tensor(data, &mut off, hidden, hidden)?,
                v: read_ternary_tensor(data, &mut off, hidden, hidden)?,
                o: read_ternary_tensor(data, &mut off, hidden, hidden)?,
                rms_ffn,
                rms_inner_attn,
                rms_ffn_norm,
                gate: read_ternary_tensor(data, &mut off, ffn_dim, hidden)?,
                up: read_ternary_tensor(data, &mut off, ffn_dim, hidden)?,
                down: read_ternary_tensor(data, &mut off, hidden, ffn_dim)?,
                kv_dim: hidden,
                num_kv_heads,
                intermediate_size,
                ffn_group_size: ffn_dim,
            });
        }
    }

    let rms_final = if version >= 2 {
        // v3 may not have rms_final (tied model)
        if off + hidden * 4 <= data.len() {
            read_f32_vec(data, &mut off, hidden)?
        } else {
            vec![1.0; hidden]
        }
    } else {
        vec![1.0; hidden]
    };

    // Unembed - v3 may have tie_word_embeddings (no unembed)
    let unembed = if off + 4 < data.len() {
        // Try to read unembed; if insufficient data, use embed as tied weights
        let remaining = data.len() - off;
        let expected = (hidden * vocab_size as usize + 3) / 4;
        if remaining >= expected {
            read_ternary_tensor(data, &mut off, hidden, vocab_size as usize)?
        } else {
            tie_embeddings = true;
            // Create empty placeholder - will be filled from embed at inference
            let packed = vec![0u8; expected];
            PackedTernaryTensor { shape: (hidden, vocab_size as usize), packed_data: packed }
        }
    } else {
        tie_embeddings = true;
        let expected = (hidden * vocab_size as usize + 3) / 4;
        let packed = vec![0u8; expected];
        PackedTernaryTensor { shape: (hidden, vocab_size as usize), packed_data: packed }
    };

    let mut medusa_heads = Vec::with_capacity(num_medusa);
    if num_medusa > 0 {
        for _ in 0..num_medusa {
            let w = read_ternary_tensor(data, &mut off, hidden, vocab_size as usize)?;
            medusa_heads.push(MedusaHead { w });
        }
    }

    let q_dim = if version >= 3 {
        let qk_head_dim = 32;
        num_heads * qk_head_dim
    } else {
        hidden
    };

    // v1/v2: embed is Tensor → convert to PackedTernaryTensor (hidden, vocab_size)
    let embed = {
        let hidden = embed.shape.1;
        let vocab = embed.shape.0;
        let mut vals = Vec::with_capacity(hidden * vocab);
        for h in 0..hidden {
            for v in 0..vocab {
                vals.push(if embed.data[v * hidden + h] > 0.0 { 1i8 } else if embed.data[v * hidden + h] < 0.0 { -1i8 } else { 0i8 });
            }
        }
        let packed = PackedTernaryTensor::pack_weights(&vals);
        PackedTernaryTensor { shape: (hidden, vocab), packed_data: packed }
    };

    let model = TransformerModel {
        embed, layers, rms_final, unembed, medusa_heads,
        vocab_size, hidden, num_layers, max_seq: max_seq as usize,
        num_heads, num_kv_heads, head_dim, kv_dim: q_dim,
        intermediate_size,
        ffn_group_size: intermediate_size / 4,
        tie_embeddings,
        rope_theta: 10000.0,
        rope_cos: vec![],
        rope_sin: vec![],
    };
    GLOBAL_MODEL_PARAMS.store(_num_params as u64, core::sync::atomic::Ordering::Relaxed);
    Some(model)
}

fn argmax_row(logits: &Tensor, row: usize) -> u16 {
    let cols = logits.shape.1;
    let start = row * cols;
    let mut best = 0u16;
    let mut best_val = NEG_INFINITY;
    for j in 0..cols {
        let v = logits.data[start + j];
        if v > best_val { best_val = v; best = j as u16; }
    }
    best
}

pub fn generate_speculative(model: &TransformerModel, prompt: &str) -> alloc::string::String {
    let max_seq = model.max_seq;
    let use_bpe = crate::bpe::is_loaded();
    let mut tokens = if use_bpe { crate::bpe::encode(prompt) } else { Tokenizer::encode(prompt) };
    let prompt_len = tokens.len();
    crate::serial_println!("[GEN] prompt_len={}, max_seq={}", prompt_len, max_seq);

    // Inicializa KV cache com dimensoes do modelo
    // Use model.kv_dim (Q output dim) for kv_dim, and infer k_dim from first layer's K projection
    let kv_dim = model.kv_dim;
    let k_dim = if model.layers.is_empty() { kv_dim } else {
        model.layers[0].k.shape.1 // actual K projection output dimension
    };
    let mut cache = KvCache::new(model.layers.len(), k_dim, kv_dim);

    // Processa o prompt completo (preenche cache)
    let t0 = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
    let (mut last_hidden, _) = model.forward_with_kv(&tokens, &mut cache);
    let t1 = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
    crate::serial_println!("[GEN] prompt fwd: {} ticks", t1 - t0);

    let max_gen = max_seq.saturating_sub(prompt_len).min(8);
    for step in 0..max_gen {
        if tokens.len() >= max_seq { break; }

        // Gera proximo token a partir do ultimo hidden state
        let logits = if model.tie_embeddings {
            model.embed.matmul_hybrid(&last_hidden).unwrap()
        } else {
            model.unembed.matmul_hybrid(&last_hidden).unwrap()
        };
        let next = argmax_row(&logits, 0);
        let eos = if use_bpe { 2u16 } else { EOS };
        if next == eos { break; }

        tokens.push(next);

        // Processa apenas o novo token com KV cache
        let t_step = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
        let (new_hidden, _) = model.forward_with_kv(&[next], &mut cache);
        let t_step1 = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
        if t_step1 - t_step > 5 {
            crate::serial_println!("[GEN] step={} token={} kv_cache: {} ticks (ctx={})",
                step + 1, next, t_step1 - t_step, tokens.len());
        }
        last_hidden = new_hidden;
    }

    let gen = &tokens[prompt_len..];
    if use_bpe { crate::bpe::decode(gen) } else { Tokenizer::decode(gen) }
}

pub fn generate_text(model: &TransformerModel, prompt: &str) -> alloc::string::String {
    let raw = generate_speculative(model, prompt);
    // TV-DSL determinism
    if raw.contains("[TV-DSL: ") {
        match crate::tv_dsl::scan_and_execute(&raw) {
            Ok(processed) => processed,
            Err(_) => raw,
        }
    } else {
        raw
    }
}

// ---------------------------------------------------------------------------
// Model trait — engine de LLM plugável (BitNet / GGUF / PTRM)
// ---------------------------------------------------------------------------

pub trait Model: Send {
    fn generate(&self, prompt: &str) -> String;
    fn embed_dim(&self) -> usize;
    fn vocab_size(&self) -> u32;
    fn max_seq(&self) -> usize;
}

static CURRENT_MODEL: spin::Mutex<Option<Box<dyn Model>>> = spin::Mutex::new(None);
pub static RUSTCODER_MODEL: spin::Mutex<Option<Box<dyn Model>>> = spin::Mutex::new(None);
pub static HWEXPERT_MODEL: spin::Mutex<Option<Box<dyn Model>>> = spin::Mutex::new(None);

pub fn set_model(model: Box<dyn Model>) {
    *CURRENT_MODEL.lock() = Some(model);
    crate::serial_println!("[CORTEX] Model swapped.");
}

pub fn set_rustcoder_model(model: Box<dyn Model>) {
    *RUSTCODER_MODEL.lock() = Some(model);
    crate::serial_println!("[CORTEX] RustCoder expert model loaded.");
}

pub fn set_hwexpert_model(model: Box<dyn Model>) {
    *HWEXPERT_MODEL.lock() = Some(model);
    crate::serial_println!("[CORTEX] HW Expert model loaded (SDIO MoE).");
}

pub fn generate_via_model(prompt: &str) -> String {
    // MoE routing: Trinity classifica intencao, roteia para expert se disponivel
    let expert_name = {
        let trinity = crate::TRINITY.lock();
        let expert = trinity.classify_intent(prompt);
        let name = expert.name;
        drop(trinity);
        name
    };
    // Tenta expert RustCoder
    if expert_name == "rust_coder" {
        let guard = RUSTCODER_MODEL.lock();
        if let Some(m) = guard.as_ref() {
            crate::serial_println!("[TRINITY] MoE routing: RustCoder expert");
            return m.generate(&alloc::format!(
                "{\"role\":\"system\",\"content\":\"Gere apenas codigo Rust valido.\"}\n{}\n", prompt));
        }
    }
    // Tenta expert HW Identify
    if expert_name == "hw_identify" {
        let guard = HWEXPERT_MODEL.lock();
        if let Some(m) = guard.as_ref() {
            crate::serial_println!("[TRINITY] MoE routing: HWIdentify expert");
            return m.generate(&alloc::format!("identifique hardware {}", prompt));
        }
    }
    // Fallback: modelo principal (BitNet LLM)
    let guard = CURRENT_MODEL.lock();
    match guard.as_ref() {
        Some(m) => m.generate(prompt),
        None => String::from("[CORTEX] No model loaded"),
    }
}

pub fn generate_via_rustcoder(prompt: &str) -> String {
    let guard = RUSTCODER_MODEL.lock();
    match guard.as_ref() {
        Some(m) => m.generate(prompt),
        None => String::from("[RUSTCODER] No expert model loaded"),
    }
}

pub fn generate_via_hwexpert(prompt: &str) -> String {
    let guard = HWEXPERT_MODEL.lock();
    match guard.as_ref() {
        Some(m) => m.generate(prompt),
        None => String::from("[HWEXPERT] No HW expert model loaded"),
    }
}

// Wrap TransformerModel as Model
impl Model for TransformerModel {
    fn generate(&self, prompt: &str) -> String { generate_text(self, prompt) }
    fn embed_dim(&self) -> usize { self.hidden }
    fn vocab_size(&self) -> u32 { self.vocab_size }
    fn max_seq(&self) -> usize { self.max_seq }
}

// ---------------------------------------------------------------------------
// PTRM — Probabilistic Tiny Recursive Model (±300 LOC)
// Gaussian noise + Q-head + parallel trajectories
// ---------------------------------------------------------------------------

/// Box-Muller transform for Gaussian noise (no_std, using libm)
pub fn gaussian_noise(mean: f32, std: f32) -> f32 {
    // Use a simple LCG + Box-Muller
    static SEED: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(42);
    let s = SEED.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    let u1 = (s as f32) / 4294967296.0;
    let u2 = ((s as u64).wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407) as f32) / 4294967296.0;
    let r = unsafe { libm::sqrtf(-2.0 * libm::logf(u1.max(0.0001))) };
    let theta = 6.283185307 * u2;
    mean + std * r * unsafe { libm::cosf(theta) }
}

/// PTRM: gera texto com ruído + trajetórias paralelas + Q-head
pub fn ptrm_generate(model: &TransformerModel, prompt: &str) -> String {
    let tokens = Tokenizer::encode(prompt);
    let mut best_text = alloc::string::String::new();
    let mut best_score = -1000.0f32;

    for _traj in 0..3 {
        let mut t = tokens.clone();
        let mut traj_text = alloc::string::String::new();

        for _step in 0..16 {
            if t.len() >= MAX_SEQ { break; }

            // Forward + noise injection
            let (_hidden, logits) = model.forward_hidden(&t);

            // Q-head: confidence score (max logit)
            let q = (0..logits.shape.1).fold(0.0f32, |max, i| {
                let v = logits.data[i];
                if v > max { v } else { max }
            });

            // Sample com ruído (exploração)
            let mut noisy_logits = logits.data.clone();
            for v in &mut noisy_logits {
                *v += gaussian_noise(0.0, 0.05);
            }

            let next = argmax_from_slice(&noisy_logits, 0);

            if next == EOS || next >= VOCAB_SIZE { break; }
            t.push(next);
            traj_text.push(Tokenizer::decode_char(next).unwrap_or('?'));

            // Atualiza best score
            if q > best_score && traj_text.len() > 3 {
                best_score = q;
                best_text = traj_text.clone();
            }
        }
    }

    if best_text.is_empty() { Tokenizer::decode(&tokens) } else { best_text }
}

fn argmax_from_slice(data: &[f32], row: usize) -> u16 {
    let cols = data.len().max(1);
    let start = row * cols;
    let end = core::cmp::min(start + cols, data.len());
    if start >= end { return EOS; }
    let mut best = start;
    for i in start..end {
        if data[i] > data[best] { best = i; }
    }
    ((best - start) as u16).min(VOCAB_SIZE - 1)
}

pub struct Cortex {
    pub tokenizer: Tokenizer,
}

impl Cortex {
    pub const fn new() -> Self { Cortex { tokenizer: Tokenizer } }

    pub fn think(&self, text: &str) -> Intent {
        let lower = text.to_ascii_lowercase();
        if lower.contains("status") || lower.contains("system") || lower.contains("info") {
            Intent::SystemStatus
        } else if lower.contains("echo") || lower.contains("reverse") || lower.contains("repeat") {
            Intent::Echo
        } else if lower.contains("hw") || lower.contains("hardware") {
            if lower.contains("identify") || lower.contains("identifique") || lower.contains("id ") || lower == "hw" {
                Intent::HardwareIdentify
            } else {
                Intent::HardwareInfo
            }
        } else if lower.contains("trust allow") {
            Intent::TrustAllow
        } else if lower.contains("trust deny") {
            Intent::TrustDeny
        } else if lower.contains("ping") || lower.contains("net") || lower.contains("diag") {
            Intent::Network
        } else if lower.contains("fetch") || lower.contains("http") {
            Intent::HttpFetch
        } else if lower.contains("help") || lower.contains("?") {
            Intent::Help
        } else if lower.contains("conv") || lower.contains("history") {
            Intent::Conversation
        } else if lower.contains("usage") || lower.contains("metrics") {
            Intent::Usage
        } else if lower.contains("hello") || lower.contains("hi") || lower.contains("hey") || lower.contains("ola") || lower.contains("oi") {
            Intent::Greeting
        } else {
            Intent::Chat
        }
    }
}

#[derive(Debug)]
pub enum Intent {
    SystemStatus, Echo, HardwareInfo, HardwareIdentify, TrustAllow, TrustDeny,
    Network, HttpFetch, Help, Conversation, Usage, Greeting, Chat,
}

// ── M2: Consciência — Métricas Cognitivas ─────────────────────
// 10 métricas que medem a saúde do sistema nervoso do AIOS.
// Cada metrica tem valor 0-10000, target, e evolucao percentual.
// CortexAgent atualiza a cada N ticks. Se alguma cai abaixo do
// threshold, HermesAgent e informado para auto-recuperacao.

#[derive(Debug, Clone, Copy)]
pub struct CognitiveMetric {
    pub value: u16,         // 0..10000
    pub previous: u16,
    pub target: u16,        // valor ideal (ex: 8000 = 80%)
    pub evolution: i16,     // percentual * 100 (ex: +250 = +2.5%)
}

impl CognitiveMetric {
    pub const fn new(target: u16) -> Self {
        CognitiveMetric { value: 5000, previous: 5000, target, evolution: 0 }
    }

    pub fn update(&mut self, new_value: u16) {
        self.previous = self.value;
        self.value = new_value.min(10000);
        if self.previous > 0 {
            let diff = (self.value as i32 - self.previous as i32) * 10000 / self.previous as i32;
            self.evolution = diff as i16;
        }
    }

    pub fn health(&self) -> f32 {
        self.value as f32 / self.target as f32
    }
}

pub struct Consciousness {
    pub metrics: [CognitiveMetric; 10],
    pub tick_interval: u64,
    pub last_tick: u64,
}

impl Consciousness {
    pub fn new() -> Self {
        Consciousness {
            metrics: [
                CognitiveMetric::new(9000), // 0: cognitive_coherence
                CognitiveMetric::new(7000), // 1: learning_rate
                CognitiveMetric::new(8500), // 2: error_resolution_rate
                CognitiveMetric::new(6000), // 3: response_latency (invertido: menor=melhor)
                CognitiveMetric::new(8000), // 4: tool_utilization
                CognitiveMetric::new(7500), // 5: memory_cohesion
                CognitiveMetric::new(9000), // 6: anomaly_detection_rate
                CognitiveMetric::new(9500), // 7: boot_stability
                CognitiveMetric::new(8500), // 8: skill_success_rate
                CognitiveMetric::new(9000), // 9: agent_health
            ],
            tick_interval: 200,
            last_tick: 0,
        }
    }

    /// Atualiza metricas baseadas em dados do sistema.
    /// Chamado pelo CortexAgent a cada tick_interval ticks.
    pub fn tick(&mut self, tick: u64, skills_ok: u64, skills_total: u64,
                agents_active: usize, agents_total: usize,
                errors_recent: u64, errors_resolved: u64,
                memories_total: usize, anomaly_count: u64, boot_ok: bool) {
        if tick - self.last_tick < self.tick_interval { return; }
        self.last_tick = tick;

        // 0: cognitive_coherence — consistencia das decisoes do Cortex
        // Quanto menos mudancas de intent entre ticks similares, melhor
        // (medido externamente pelo HermesAgent, aqui usamos proxy)
        // 1: learning_rate — novos padroes por janela
        // Proxy: skills_total crescendo
        let learning = if skills_total > 0 { skills_ok * 10000 / skills_total } else { 5000 };
        self.metrics[1].update(learning as u16);

        // 2: error_resolution_rate — auto-recuperacao
        if errors_recent > 0 {
            let rate = errors_resolved * 10000 / errors_recent;
            self.metrics[2].update(rate as u16);
        }

        // 3: response_latency — ticks entre intent e resposta
        // Medido pelo HermesAgent externamente, proxy aqui

        // 4: tool_utilization — diversidade de skills usadas
        let util = if skills_total > 0 { skills_ok.min(skills_total as u64) as u16 } else { 0 };
        self.metrics[4].update(util.min(10000));

        // 5: memory_cohesion — quantas memorias tem conexoes
        // Proxy: quanto mais memorias, mais coeso (ate um limite)
        let cohesion = (memories_total.min(100) as u16) * 100;
        self.metrics[5].update(cohesion);

        // 6: anomaly_detection_rate — seguranca
        // Proxy: anomalias detectadas (invertido: muitas anomalias = bom)
        let anomaly_val = if anomaly_count > 100 { 10000u16 } else { (anomaly_count as u16) * 100 };
        self.metrics[6].update(anomaly_val);

        // 7: boot_stability — fases de boot completas
        self.metrics[7].update(if boot_ok { 10000 } else { 1000 });

        // 8: skill_success_rate — skills que completam sem erro
        if skills_total > 0 {
            let rate = skills_ok * 10000 / skills_total;
            self.metrics[8].update(rate as u16);
        }

        // 9: agent_health — agentes ativos vs total
        if agents_total > 0 {
            let health = agents_active * 10000 / agents_total;
            self.metrics[9].update(health as u16);
        }
    }

    /// Retorna metricas que estao abaixo do threshold (saude < 0.5)
    pub fn critical_metrics(&self) -> Vec<usize> {
        let mut critical = Vec::new();
        for (i, m) in self.metrics.iter().enumerate() {
            if m.health() < 0.5 {
                critical.push(i);
            }
        }
        critical
    }

    pub fn report(&self) -> alloc::string::String {
        use alloc::format;
        let names = [
            "coherence", "learning", "error_resolution", "latency",
            "tool_util", "memory", "anomaly", "boot", "skill_success", "agent_health",
        ];
        let mut r = alloc::string::String::from("[CONSCIOUSNESS] Metrics:\n");
        for (i, m) in self.metrics.iter().enumerate() {
            let pct = m.value as f32 / 100.0;
            let evo = if m.evolution >= 0 {
                format!("+{:.2}%", m.evolution as f32 / 100.0)
            } else {
                format!("{:.2}%", m.evolution as f32 / 100.0)
            };
            r.push_str(&format!("  {}: {:.1}% ({}) target={}%\n",
                names[i], pct, evo, m.target as f32 / 100.0));
        }
        r
    }
}

// ── M3: Self-Improvement Loop ──────────────────────────────────
// Ciclo ativo de auto-melhoria do HermesAgent.
// Depois do ReAct::Learn, se detecta oportunidade, inicia:
// Research → Create → Improve → Verify

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SilPhase {
    Idle,
    Research,
    Create,
    Improve,
    Verify,
}

pub struct SelfImprovementLoop {
    pub phase: SilPhase,
    pub retries: u8,
    pub max_retries: u8,
    pub cooldown_ticks: u64,
    pub last_run: u64,
    pub improvements: u32,
}

impl SelfImprovementLoop {
    pub fn new() -> Self {
        SelfImprovementLoop {
            phase: SilPhase::Idle,
            retries: 0,
            max_retries: 3,
            cooldown_ticks: 500,
            last_run: 0,
            improvements: 0,
        }
    }

    /// Inicia o ciclo. Retorna true se comecou.
    pub fn start(&mut self, tick: u64) -> bool {
        if self.phase != SilPhase::Idle { return false; }
        if tick - self.last_run < self.cooldown_ticks { return false; }
        self.phase = SilPhase::Research;
        self.retries = 0;
        true
    }

    /// Avanca o ciclo. Retorna true se terminou.
    /// `research_found`: o Cortex identificou padrao de melhoria?
    /// `create_success`: a nova skill foi criada?
    /// `improve_success`: a melhoria foi aplicada?
    /// `verify_success`: a verificacao passou?
    pub fn advance(&mut self, success: bool) -> bool {
        match self.phase {
            SilPhase::Research if success => { self.phase = SilPhase::Create; false }
            SilPhase::Research => { self.phase = SilPhase::Idle; true } // nada a melhorar
            SilPhase::Create if success => { self.phase = SilPhase::Improve; false }
            SilPhase::Create => { self.phase = SilPhase::Idle; true } // falhou criar
            SilPhase::Improve if success => { self.phase = SilPhase::Verify; false }
            SilPhase::Improve => { self.retries += 1;
                if self.retries >= self.max_retries { self.phase = SilPhase::Idle; true }
                else { self.phase = SilPhase::Create; false }
            }
            SilPhase::Verify if success => {
                self.improvements += 1;
                self.phase = SilPhase::Idle;
                self.last_run = 0; // reseta cooldown
                true
            }
            SilPhase::Verify => { self.phase = SilPhase::Idle; true }
            SilPhase::Idle => true,
        }
    }

    pub fn is_active(&self) -> bool {
        self.phase != SilPhase::Idle
    }

    pub fn needs_research(&self) -> bool { self.phase == SilPhase::Research }
    pub fn needs_create(&self) -> bool { self.phase == SilPhase::Create }
    pub fn needs_improve(&self) -> bool { self.phase == SilPhase::Improve }
    pub fn needs_verify(&self) -> bool { self.phase == SilPhase::Verify }
}

impl Intent {
    pub fn skill_name(&self) -> &'static str {
        match self {
            Intent::SystemStatus => "system_status",
            Intent::Echo => "echo",
            Intent::HardwareInfo => "hardware_info",
            Intent::HardwareIdentify => "hw_identify",
            Intent::TrustAllow => "trust_allow",
            Intent::TrustDeny => "trust_deny",
            Intent::Network => "net_diag",
            Intent::HttpFetch => "http_fetch",
            Intent::Help => "help",
            Intent::Conversation => "conversation",
            Intent::Usage => "usage",
            Intent::Greeting => "greeting",
            Intent::Chat => "chat",
        }
    }
}
