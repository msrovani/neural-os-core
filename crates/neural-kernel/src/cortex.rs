use alloc::vec;
use alloc::vec::Vec;
use alloc::string::String;
use alloc::boxed::Box;
use core::f32::NEG_INFINITY;

pub const TOPIC_LLM_REQUEST: &str = "LLM_REQUEST";
pub const TOPIC_LLM_RESPONSE: &str = "LLM_RESPONSE";
pub const TOPIC_KERNEL_ERROR: &str = "KERNEL_ERROR";
pub const TOPIC_MODEL_UPDATE: &str = "MODEL_UPDATE";
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

pub struct LayerWeights {
    pub rms_attn: f32,
    pub q: PackedTernaryTensor,
    pub k: PackedTernaryTensor,
    pub v: PackedTernaryTensor,
    pub o: PackedTernaryTensor,
    pub rms_ffn: f32,
    pub gate: PackedTernaryTensor,
    pub up: PackedTernaryTensor,
    pub down: PackedTernaryTensor,
}

pub struct TransformerModel {
    pub embed: Tensor,
    pub layers: Vec<LayerWeights>,
    pub rms_final: f32,
    pub unembed: PackedTernaryTensor,
    pub medusa_heads: Vec<MedusaHead>,
}

const MEDUSA_HEADS: usize = 3;

pub struct MedusaHead {
    pub w: PackedTernaryTensor,
}

impl MedusaHead {
    pub fn new_random(seed: &mut u32) -> Self {
        MedusaHead { w: random_ternary(seed, HIDDEN, VOCAB_SIZE as usize) }
    }

    pub fn forward(&self, hidden: &Tensor) -> Tensor {
        self.w.matmul_hybrid(hidden).unwrap()
    }
}

fn random_ternary(seed: &mut u32, rows: usize, cols: usize) -> PackedTernaryTensor {
    let mut vals = Vec::with_capacity(rows * cols);
    for _ in 0..rows * cols {
        *seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        let r = (*seed % 3) as i8;
        vals.push(if r == 2 { -1 } else { r });
    }
    let packed = PackedTernaryTensor::pack_weights(&vals);
    PackedTernaryTensor { shape: (rows, cols), packed_data: packed }
}

fn random_embed(seed: &mut u32, rows: usize, cols: usize) -> Tensor {
    let mut data = Vec::with_capacity(rows * cols);
    for _ in 0..rows * cols {
        *seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        let v = (*seed % 2001) as f32 / 1000.0 - 1.0;
        data.push(v);
    }
    Tensor::from_row_major((rows, cols), data).unwrap()
}

impl TransformerModel {
    pub fn new() -> Self {
        let mut seed: u32 = 42;
        let mut layers = Vec::with_capacity(NUM_LAYERS);
        for _ in 0..NUM_LAYERS {
            layers.push(LayerWeights {
                rms_attn: 1.0,
                q: random_ternary(&mut seed, HIDDEN, HIDDEN),
                k: random_ternary(&mut seed, HIDDEN, HIDDEN),
                v: random_ternary(&mut seed, HIDDEN, HIDDEN),
                o: random_ternary(&mut seed, HIDDEN, HIDDEN),
                rms_ffn: 1.0,
                gate: random_ternary(&mut seed, HIDDEN, FFN_DIM),
                up: random_ternary(&mut seed, HIDDEN, FFN_DIM),
                down: random_ternary(&mut seed, FFN_DIM, HIDDEN),
            });
        }
        let medusa_heads = (0..MEDUSA_HEADS).map(|_| MedusaHead::new_random(&mut seed)).collect();
        TransformerModel {
            embed: random_embed(&mut seed, VOCAB_SIZE as usize, HIDDEN),
            layers,
            rms_final: 1.0,
            unembed: random_ternary(&mut seed, HIDDEN, VOCAB_SIZE as usize),
            medusa_heads,
        }
    }

    fn embed_lookup(&self, token: u16) -> Tensor {
        let idx = (token as usize).min(self.embed.shape.0 - 1);
        let start = idx * HIDDEN;
        let data = self.embed.data[start..start + HIDDEN].to_vec();
        Tensor::from_row_major((1, HIDDEN), data).unwrap()
    }

    fn rms_norm_tensor(&self, x: &Tensor, weight: f32) -> Tensor {
        let mut t = Tensor::from_row_major(x.shape, x.data.clone()).unwrap();
        rms_norm(&mut t, weight, 1e-6);
        t
    }

    pub fn forward_hidden(&self, tokens: &[u16]) -> (Tensor, Tensor) {
        let seq_len = tokens.len().min(MAX_SEQ);
        let mut x = Tensor::new((seq_len, HIDDEN));
        for (i, &t) in tokens.iter().enumerate().take(seq_len) {
            let emb = self.embed_lookup(t);
            for j in 0..HIDDEN {
                x.data[i * HIDDEN + j] = emb.data[j];
            }
        }

        let mut mask_data = vec![0.0f32; seq_len * seq_len];
        for i in 0..seq_len {
            for j in (i + 1)..seq_len {
                mask_data[i * seq_len + j] = NEG_INFINITY;
            }
        }
        let mask = Tensor::from_row_major((seq_len, seq_len), mask_data).unwrap();

        for layer in &self.layers {
            let norm = self.rms_norm_tensor(&x, layer.rms_attn);

            let q = layer.q.matmul_hybrid(&norm).unwrap();
            let k = layer.k.matmul_hybrid(&norm).unwrap();
            let v = layer.v.matmul_hybrid(&norm).unwrap();

            let k_t = k.transposed();
            let mut scores = q.matmul(&k_t).unwrap();
            let scale = 1.0 / libm::sqrtf(HEAD_DIM as f32);
            for s in scores.data.iter_mut() { *s *= scale; }
            for i in 0..seq_len {
                for j in 0..seq_len {
                    scores.data[i * seq_len + j] += mask.data[i * seq_len + j];
                }
            }
            for i in 0..seq_len {
                let start = i * seq_len;
                softmax_inplace(&mut scores.data[start..start + seq_len]);
            }
            let attn_out = scores.matmul(&v).unwrap();
            let proj = layer.o.matmul_hybrid(&attn_out).unwrap();
            x = x.add(&proj).unwrap();

            let norm2 = self.rms_norm_tensor(&x, layer.rms_ffn);
            let gate = layer.gate.matmul_hybrid(&norm2).unwrap();
            let mut gate_act = Tensor::from_row_major(gate.shape, gate.data.clone()).unwrap();
            for g in gate_act.data.iter_mut() { *g = silu(*g); }
            let up = layer.up.matmul_hybrid(&norm2).unwrap();
            let gated = gate_act.element_mul(&up).unwrap();
            let down = layer.down.matmul_hybrid(&gated).unwrap();
            x = x.add(&down).unwrap();
        }

        let final_norm = self.rms_norm_tensor(&x, self.rms_final);
        let last_hidden = Tensor::from_row_major((1, HIDDEN),
            final_norm.data[(seq_len - 1) * HIDDEN..seq_len * HIDDEN].to_vec()).unwrap();
        let logits = self.unembed.matmul_hybrid(&last_hidden).unwrap();
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

pub fn load_model(data: &[u8]) -> Option<TransformerModel> {
    let mut off = 0;
    let magic = read_u32(data, &mut off)?;
    if magic != 0xBE11BE11 { return None; }
    let _version = read_u16(data, &mut off)?;
    let _num_params = read_u32(data, &mut off)?;
    let _hidden = read_u16(data, &mut off)? as usize;
    let _layers = read_u16(data, &mut off)? as usize;
    let _heads = read_u16(data, &mut off)? as usize;
    let _vocab = read_u16(data, &mut off)? as usize;
    let _max_seq = read_u16(data, &mut off)?;
    let mut num_medusa = 0usize;
    if off + 4 > data.len() { return None; }
    num_medusa = data[off] as usize; off += 4;

    let embed = read_f32_tensor(data, &mut off, _vocab, _hidden)?;
    let mut layers = Vec::with_capacity(_layers);
    for _ in 0.._layers {
        layers.push(LayerWeights {
            rms_attn: read_f32(data, &mut off)?,
            q: read_ternary_tensor(data, &mut off, _hidden, _hidden)?,
            k: read_ternary_tensor(data, &mut off, _hidden, _hidden)?,
            v: read_ternary_tensor(data, &mut off, _hidden, _hidden)?,
            o: read_ternary_tensor(data, &mut off, _hidden, _hidden)?,
            rms_ffn: read_f32(data, &mut off)?,
            gate: read_ternary_tensor(data, &mut off, _hidden, _hidden * 2)?,
            up: read_ternary_tensor(data, &mut off, _hidden, _hidden * 2)?,
            down: read_ternary_tensor(data, &mut off, _hidden * 2, _hidden)?,
        });
    }
    let unembed = read_ternary_tensor(data, &mut off, _hidden, _vocab)?;

    let mut medusa_heads = Vec::with_capacity(num_medusa);
    for _ in 0..num_medusa {
        let w = read_ternary_tensor(data, &mut off, _hidden, _vocab)?;
        medusa_heads.push(MedusaHead { w });
    }
    if medusa_heads.is_empty() {
        let mut seed: u32 = 42;
        medusa_heads = (0..MEDUSA_HEADS).map(|_| MedusaHead::new_random(&mut seed)).collect();
    }

    Some(TransformerModel { embed, layers, rms_final: 1.0, unembed, medusa_heads })
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
    if best >= VOCAB_SIZE { EOS } else { best }
}

pub fn generate_speculative(model: &TransformerModel, prompt: &str) -> alloc::string::String {
    let mut tokens = Tokenizer::encode(prompt);
    for _ in 0..32 {
        if tokens.len() >= MAX_SEQ { break; }
        let (last_hidden, _) = model.forward_hidden(&tokens);

        let mut draft = Vec::with_capacity(MEDUSA_HEADS);
        for head in &model.medusa_heads {
            let logits = head.forward(&last_hidden);
            let tok = argmax_row(&logits, 0);
            draft.push(tok);
        }

        let mut candidates = tokens.clone();
        for &d in &draft {
            candidates.push(d);
            if candidates.len() >= MAX_SEQ { break; }
        }

        let cand_logits = model.forward(&candidates);
        let base_pos = tokens.len() - 1;
        let mut accept = 0usize;
        for (i, &d) in draft.iter().enumerate() {
            let pos = base_pos + 1 + i;
            if pos >= cand_logits.shape.0 { break; }
            if argmax_row(&cand_logits, pos) == d { accept += 1; } else { break; }
        }

        tokens.push(argmax_row(&cand_logits, base_pos));
        for &d in draft.iter().take(accept) { tokens.push(d); }

        let new_tok = if accept < MEDUSA_HEADS {
            let pos = base_pos + 1 + accept;
            if pos < cand_logits.shape.0 { Some(argmax_row(&cand_logits, pos)) } else { None }
        } else { None };
        if let Some(t) = new_tok { tokens.push(t); }

        if *tokens.last().unwrap_or(&0) == EOS { break; }
    }
    let gen = &tokens[Tokenizer::encode(prompt).len()..];
    Tokenizer::decode(gen)
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
    fn vocab_size(&self) -> u16;
    fn max_seq(&self) -> usize;
}

static CURRENT_MODEL: spin::Mutex<Option<Box<dyn Model>>> = spin::Mutex::new(None);

pub fn set_model(model: Box<dyn Model>) {
    *CURRENT_MODEL.lock() = Some(model);
    crate::serial_println!("[CORTEX] Model swapped.");
}

pub fn generate_via_model(prompt: &str) -> String {
    let guard = CURRENT_MODEL.lock();
    match guard.as_ref() {
        Some(m) => m.generate(prompt),
        None => String::from("[CORTEX] No model loaded"),
    }
}

// Wrap TransformerModel as Model
impl Model for TransformerModel {
    fn generate(&self, prompt: &str) -> String { generate_text(self, prompt) }
    fn embed_dim(&self) -> usize { HIDDEN }
    fn vocab_size(&self) -> u16 { VOCAB_SIZE }
    fn max_seq(&self) -> usize { MAX_SEQ }
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
