use alloc::vec;
use alloc::vec::Vec;
use alloc::string::String;
use alloc::boxed::Box;
use core::f32::NEG_INFINITY;
use core::cell::UnsafeCell;
use crate::ngram_spec::{NgramSpeculator, verify_draft, record_spec_hit, record_spec_bonus_forward, record_spec_tokens, record_classic_step};
// Structured decoder types from the real module (JSON/Shell/Skill grammar FSMs)
pub use crate::structured_decode::{StructuredDecoder, DecodeMode, OutputGrammar};

// ponytail: threads &mut StructuredDecoder through Model trait boundary without changing the trait.
pub(crate) struct DecoderCell(UnsafeCell<Option<*mut StructuredDecoder>>);
unsafe impl Send for DecoderCell {}
unsafe impl Sync for DecoderCell {}
impl DecoderCell {
    const fn new() -> Self { Self(UnsafeCell::new(None)) }
    pub(crate) fn set(&self, ptr: *mut StructuredDecoder) { unsafe { *self.0.get() = Some(ptr); } }
    pub(crate) fn take(&self) -> Option<*mut StructuredDecoder> { unsafe { (*self.0.get()).take() } }
}
pub(crate) static DECODER_CELL: DecoderCell = DecoderCell::new();

pub const TOPIC_LLM_REQUEST: &str = "LLM_REQUEST";
pub const TOPIC_LLM_RESPONSE: &str = "LLM_RESPONSE";
pub const TOPIC_KERNEL_ERROR: &str = "KERNEL_ERROR";
pub const TOPIC_MODEL_UPDATE: &str = "MODEL_UPDATE";

pub static GLOBAL_MODEL_PARAMS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
use crate::nn::{silu, relu2, rms_norm};
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
    // Protecao contra NaN: se max for NaN, distribui uniformemente
    if max.is_nan() {
        let inv = 1.0 / logits.len() as f32;
        for v in logits.iter_mut() { *v = inv; }
        return;
    }
    let mut sum = 0.0;
    for v in logits.iter_mut() {
        *v = libm::expf(*v - max);
        sum += *v;
    }
    if sum.is_nan() || sum == 0.0 {
        let inv = 1.0 / logits.len() as f32;
        for v in logits.iter_mut() { *v = inv; }
        return;
    }
    let inv = 1.0 / sum;
    for v in logits.iter_mut() { *v *= inv; }
}

pub fn rope_precompute(max_seq: usize, head_dim: usize, theta: f32) -> (Vec<f32>, Vec<f32>) {
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
    if cos.is_empty() || sin.is_empty() || head_dim < 2 {
        return;
    }
    let half = head_dim / 2;
    for s in 0..seq_len {
        let pos = start_pos + s;
        let base = s * num_heads * head_dim;
        let rope_off = pos * half;
        if rope_off + half > cos.len() || rope_off + half > sin.len() {
            return;
        }
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
    pub q_scale: f32,
    pub k: PackedTernaryTensor,
    pub k_scale: f32,
    pub v: PackedTernaryTensor,
    pub v_scale: f32,
    pub o: PackedTernaryTensor,
    pub o_scale: f32,
    pub rms_ffn: Vec<f32>,
    pub rms_inner_attn: Vec<f32>,
    pub rms_ffn_norm: Vec<f32>,
    pub gate: PackedTernaryTensor,
    pub gate_scale: f32,
    pub up: PackedTernaryTensor,
    pub up_scale: f32,
    pub down: PackedTernaryTensor,
    pub down_scale: f32,
    // GQA fields
    pub kv_dim: usize,
    pub num_kv_heads: usize,
    // BitFFN fields
    pub intermediate_size: usize,
    pub ffn_group_size: usize,
}

pub struct TransformerModel {
    pub embed: PackedTernaryTensor,
    pub embed_scale: f32,
    pub layers: Vec<LayerWeights>,
    pub rms_final: Vec<f32>,
    pub unembed: PackedTernaryTensor,
    pub unembed_scale: f32,
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
    // Ativação FFN (ADR-0085 header): 0=silu, 1=relu2 (2B4T)
    pub act_type: u8,
    // Tipo do embedding (ADR-0085): 0=ternary-packed, 1=Q6_K, 2=BF16
    pub embed_type: u8,
    // Embedding Q6_K bruto (210B/256 pesos) quando embed_type==1 — decode row-wise
    // no embed_lookup (evita materializar ~1.31GB f32 do vocab 128256).
    pub embed_q6k: Option<Vec<u8>>,
    // RoPE
    pub rope_theta: f32,
    pub rope_cos: Vec<f32>,
    pub rope_sin: Vec<f32>,
}

/// HW Expert v4 — classificador multi-head (5 heads) em vez de texto livre.
/// Reusa o backbone transformer do v3, mas substitui `unembed` por 5 heads
/// de classificação: family, fw, agent, caps, next_action.
pub struct HwExpertV4Model {
    pub hidden: usize,
    pub num_layers: usize,
    pub embed: PackedTernaryTensor,
    pub embed_scale: f32,
    pub layers: Vec<LayerWeights>,
    pub rms_final: Vec<f32>,
    // 5 heads multi-head (em vez de unembed)
    pub family_head: PackedTernaryTensor,  // (hidden, 17)
    pub fw_head: PackedTernaryTensor,      // (hidden, 8)
    pub agent_head: PackedTernaryTensor,   // (hidden, 9)
    pub caps_head: PackedTernaryTensor,    // (hidden, 10)
    pub next_head: PackedTernaryTensor,    // (hidden, 9)
    pub q_dim: usize,
    pub ff_dim: usize,
}

// ─── HW Expert v5 multi-head ─────────────────────────────────────────

/// Lê tensor ternário no formato export_v4 (tools/train_hw_expert_v4.py):
///   u32 len + u32 scale + packed (rows*cols+3)/4 bytes.
/// O campo scale do arquivo é sempre 0 (vestigial) — os pesos ternários
/// já são ±1/0 absolutos, então usamos 1.0 como scale efetiva.
fn read_prefixed_ternary(data: &[u8], offset: &mut usize, rows: usize, cols: usize) -> Option<(PackedTernaryTensor, f32)> {
    let _len = read_u32(data, offset)?;
    let _scale_raw = read_u32(data, offset)?;
    let packed = read_ternary_tensor(data, offset, rows, cols)?;
    Some((packed, 1.0))
}

/// Lê vec f32 no formato export_v4: u32 len prefix + n f32.
fn read_prefixed_f32_vec(data: &[u8], offset: &mut usize, n: usize) -> Option<Vec<f32>> {
    let _len = read_u32(data, offset)?;
    read_f32_vec(data, offset, n)
}

/// Carrega modelo v5 multi-head (.bitnet version 5).
/// Lê o backbone (embed + layers) igual ao v4, depois lê 5 heads pequenos.
/// Formato real do arquivo (export_v4): num_params u32; tensores com
/// prefixo u32 len + u32 scale; shapes q,k,v,o = (h,h); g,u = (h,ff);
/// d = (ff,h); rope (16 f32) por layer; 5 heads prefixed.
pub fn load_hwexpert_v5(data: &[u8]) -> Option<HwExpertV4Model> {
    let mut off = 0;
    let magic = read_u32(data, &mut off)?;
    if magic != 0xBE11BE11 { return None; }
    let version = read_u16(data, &mut off)?;
    if version < 5 { return None; }  // v5 required for multi-head
    let _num_params = read_u32(data, &mut off)? as u64;

    let hidden = read_u16(data, &mut off)? as usize;
    let num_layers = read_u16(data, &mut off)? as usize;

    // Auto-expand heap
    {
        let _nh = read_u16(data, &mut off)? as usize;
        let _vs = read_u32(data, &mut off)? as usize;
        let _ms = read_u16(data, &mut off)? as usize;
        let _isize = read_u16(data, &mut off)? as usize;
        let _nkv = read_u16(data, &mut off)? as usize;
        let _qd = read_u16(data, &mut off)? as usize;
        let _medusa = read_u32(data, &mut off)? as usize;
        off += 4; // tie_flag
        off += 1; // tok_type
        let _tok_len = read_u32(data, &mut off).unwrap_or(0) as usize;
        // off = off.saturating_add(tok_len); // not used, overwritten below
        // off += 1; // layer_features // not used, overwritten below
        // now off is past the header
    }

    // Full re-parse from offset 4+2+4+2+2 = 14
    off = 4 + 2 + 4 + 2 + 2;  // magic(4) + version(2) + num_params(4) + hidden(2) + layers(2)
    let num_heads = read_u16(data, &mut off)? as usize;
    let _vocab_size = read_u32(data, &mut off)?;
    let _max_seq = read_u16(data, &mut off)?;
    let intermediate_size = read_u16(data, &mut off)? as usize;
    let num_kv_heads = read_u16(data, &mut off)? as usize;
    let q_dim = read_u16(data, &mut off)? as usize;
    let _num_medusa = read_u32(data, &mut off)? as usize;

    // tie_flag
    let tie = if off + 4 <= data.len() { &data[off..off+4] == b"MH\x00\x00" } else { false };
    off += 4;
    let _tok_type = if off < data.len() { data[off] } else { 0 }; off += 1;
    let tok_len = read_u32(data, &mut off).unwrap_or(0) as usize;
    off = off.saturating_add(tok_len);
    let layer_features = if version >= 4 && off < data.len() { data[off] } else { 0u8 }; off += 1;
    let _has_inner_attn_ln = (layer_features & 0x01) != 0;
    let _has_ffn_layernorm = (layer_features & 0x02) != 0;
    let _has_rope = (layer_features & 0x04) != 0;

    // Multi-head sanity: tok_type should be 5
    if _tok_type != 5 && !tie {
        k_nano::slog_cortex!("HWEXPERT", "warn", "v5 model without multi-head marker (tok_type={})", _tok_type);
    }

    let kv_head_dim = q_dim / num_heads.max(1);
    let ffn_group = intermediate_size * q_dim / hidden.max(1);

    // Embed
    let (embed, embed_scale) = read_prefixed_ternary(data, &mut off, hidden, _vocab_size as usize)?;

    // Layers
    let mut layers = Vec::with_capacity(num_layers);
    for _i in 0..num_layers {
        let rms_attn = read_prefixed_f32_vec(data, &mut off, hidden)?;
        let rms_ffn = read_prefixed_f32_vec(data, &mut off, hidden)?;
        let rms_inner_attn = if _has_inner_attn_ln { read_prefixed_f32_vec(data, &mut off, hidden)? }
                              else { vec![1.0; kv_head_dim * num_heads] };
        let rms_ffn_norm = if _has_ffn_layernorm { read_prefixed_f32_vec(data, &mut off, intermediate_size)? }
                            else { vec![1.0; intermediate_size] };

        // Export v4 grava q,k,v,o como (h,h) (q/k nao usados no forward),
        // g,u como (h,ff), d como (ff,h), e um vetor RoPE de 16 f32 por layer.
        let (q, q_scale) = read_prefixed_ternary(data, &mut off, hidden, hidden)?;
        let (k, k_scale) = read_prefixed_ternary(data, &mut off, hidden, hidden)?;
        let (v, v_scale) = read_prefixed_ternary(data, &mut off, hidden, hidden)?;
        let (o, o_scale) = read_prefixed_ternary(data, &mut off, hidden, hidden)?;
        let (gate, gate_scale) = read_prefixed_ternary(data, &mut off, hidden, intermediate_size)?;
        let (up, up_scale) = read_prefixed_ternary(data, &mut off, hidden, intermediate_size)?;
        let (down, down_scale) = read_prefixed_ternary(data, &mut off, intermediate_size, hidden)?;
        let _rope = read_prefixed_f32_vec(data, &mut off, 16)?;

        layers.push(LayerWeights {
            rms_attn, q, q_scale, k, k_scale, v, v_scale, o, o_scale,
            rms_ffn, rms_inner_attn, rms_ffn_norm,
            gate, gate_scale, up, up_scale, down, down_scale,
            kv_dim: q_dim, num_kv_heads, intermediate_size, ffn_group_size: ffn_group,
        });
    }

    let rms_final = read_prefixed_f32_vec(data, &mut off, hidden)?;

    // 5 heads (em vez de unembed + medusa)
    let (family_head, _) = read_prefixed_ternary(data, &mut off, hidden, 17)?;
    let (fw_head, _) = read_prefixed_ternary(data, &mut off, hidden, 8)?;
    let (agent_head, _) = read_prefixed_ternary(data, &mut off, hidden, 9)?;
    let (caps_head, _) = read_prefixed_ternary(data, &mut off, hidden, 10)?;
    let (next_head, _) = read_prefixed_ternary(data, &mut off, hidden, 9)?;

    k_nano::slog_cortex!("HWEXPERT", "info", "v5 multi-head loaded: hidden={} layers={} heads=[17,8,9,10,9] {}KB",
        hidden, num_layers, data.len() / 1024);
    GLOBAL_MODEL_PARAMS.store(data.len() as u64, core::sync::atomic::Ordering::Relaxed);

    Some(HwExpertV4Model {
        hidden, num_layers, embed, embed_scale, layers, rms_final,
        family_head, fw_head, agent_head, caps_head, next_head,
        q_dim, ff_dim: intermediate_size,
    })
}

/// Carrega modelo HW Expert no formato canônico v6 (ADR-0085 §3.2, model_type=1).
/// Body sem prefixos (rms = f32 puro; tensor = packed + f32 scale), shapes
/// fixos hwexpert: q/k/v/o=(h,h), g/u=(h,ff), d=(ff,h) — mesmos do
/// load_hwexpert_v5, sem rope (forward não usa). feat bits do header v6:
/// bit0=rms_inner_attn, bit1=rms_ffn_norm (o v6 não tem rope/theta p/ hwexpert).
/// q_dim do header é preservado no modelo: o forward (predict_hw_v4) usa
/// model.q_dim para truncar a atenção (modelo treinado com qd = h/heads).
pub fn load_hwexpert_v6(data: &[u8]) -> Option<HwExpertV4Model> {
    let mut off = 0;
    let magic = read_u32(data, &mut off)?;
    if magic != 0xBE11BE11 { return None; }
    let version = read_u16(data, &mut off)?;
    if version != 6 { return None; }
    let _num_params = read_u64(data, &mut off)?;
    let model_type = read_u8(data, &mut off)?;
    if model_type != 1 { return None; } // HWExpert
    if off + 3 > data.len() { return None; }
    if data[off] != 0 || data[off + 1] != 0 || data[off + 2] != 0 { return None; }
    off += 3;

    // Bloco transformer (ADR-0085 §2, offset 18)
    let hidden = read_u16(data, &mut off)? as usize;
    let num_layers = read_u16(data, &mut off)? as usize;
    let num_heads = read_u16(data, &mut off)? as usize;
    let vocab_size = read_u32(data, &mut off)? as usize;
    let _max_seq = read_u16(data, &mut off)? as usize;
    let intermediate_size = read_u16(data, &mut off)? as usize;
    let _num_kv_heads = read_u16(data, &mut off)? as usize;
    let q_dim = read_u16(data, &mut off)? as usize;
    let _num_medusa = read_u32(data, &mut off)? as usize;
    off += 4; // tie_flag
    let _tok_type = read_u8(data, &mut off)?;
    let tok_len = read_u32(data, &mut off)? as usize;
    off = off.saturating_add(tok_len);
    let _act_type = read_u8(data, &mut off)?;   // não usado p/ hwexpert
    let _embed_type = read_u8(data, &mut off)?; // 0 = ternary
    let feat = read_u8(data, &mut off)?;
    if feat & 0xF8 != 0 { return None; }
    let has_inner = (feat & 0x01) != 0;
    let has_ffn = (feat & 0x02) != 0;
    let kv_head_dim = q_dim / num_heads.max(1);

    k_nano::slog_cortex!("HWEXPERT", "info",
        "v6 multi-head h={} L={} q_dim={} vocab={} ff={} feat=0x{:02x}",
        hidden, num_layers, q_dim, vocab_size, intermediate_size, feat);

    // Embed (packed + scale — sem prefixo, ADR-0085 D1)
    let (embed, embed_scale) = read_ternary_tensor_with_scale(data, &mut off, hidden, vocab_size)?;

    // Layers — shapes fixos hwexpert (q/k/v/o=(h,h), g/u=(h,ff), d=(ff,h))
    let mut layers = Vec::with_capacity(num_layers);
    for _i in 0..num_layers {
        let rms_attn = read_f32_vec(data, &mut off, hidden)?;
        let rms_ffn = read_f32_vec(data, &mut off, hidden)?;
        let rms_inner_attn = if has_inner { read_f32_vec(data, &mut off, hidden)? }
                             else { vec![1.0; kv_head_dim * num_heads] };
        let rms_ffn_norm = if has_ffn { read_f32_vec(data, &mut off, intermediate_size)? }
                           else { vec![1.0; intermediate_size] };
        let (q, q_scale) = read_ternary_tensor_with_scale(data, &mut off, hidden, hidden)?;
        let (k, k_scale) = read_ternary_tensor_with_scale(data, &mut off, hidden, hidden)?;
        let (v, v_scale) = read_ternary_tensor_with_scale(data, &mut off, hidden, hidden)?;
        let (o, o_scale) = read_ternary_tensor_with_scale(data, &mut off, hidden, hidden)?;
        let (gate, gate_scale) = read_ternary_tensor_with_scale(data, &mut off, hidden, intermediate_size)?;
        let (up, up_scale) = read_ternary_tensor_with_scale(data, &mut off, hidden, intermediate_size)?;
        let (down, down_scale) = read_ternary_tensor_with_scale(data, &mut off, intermediate_size, hidden)?;
        layers.push(LayerWeights {
            rms_attn, q, q_scale, k, k_scale, v, v_scale, o, o_scale,
            rms_ffn, rms_inner_attn, rms_ffn_norm,
            gate, gate_scale, up, up_scale, down, down_scale,
            kv_dim: q_dim, num_kv_heads: num_heads, intermediate_size,
            ffn_group_size: intermediate_size * q_dim / hidden.max(1),
        });
    }

    let rms_final = read_f32_vec(data, &mut off, hidden)?;

    // 5 heads (substitui unembed/medusa — ADR-0085 §3.2)
    let (family_head, _) = read_ternary_tensor_with_scale(data, &mut off, hidden, 17)?;
    let (fw_head, _) = read_ternary_tensor_with_scale(data, &mut off, hidden, 8)?;
    let (agent_head, _) = read_ternary_tensor_with_scale(data, &mut off, hidden, 9)?;
    let (caps_head, _) = read_ternary_tensor_with_scale(data, &mut off, hidden, 10)?;
    let (next_head, _) = read_ternary_tensor_with_scale(data, &mut off, hidden, 9)?;

    k_nano::slog_cortex!("HWEXPERT", "info",
        "v6 multi-head loaded: hidden={} layers={} heads=[17,8,9,10,9] {}KB",
        hidden, num_layers, data.len() / 1024);
    GLOBAL_MODEL_PARAMS.store(data.len() as u64, core::sync::atomic::Ordering::Relaxed);

    Some(HwExpertV4Model {
        hidden, num_layers, embed, embed_scale, layers, rms_final,
        family_head, fw_head, agent_head, caps_head, next_head,
        q_dim, ff_dim: intermediate_size,
    })
}

/// Pack 4 tokens from VID/DID (same packing as Python pack_vid_did)
fn pack_vid_did(vid: u16, did: u16, vocab: u32) -> [u32; 4] {
    let v = vocab as u16;
    [
        ((vid >> 8) % v) as u32,
        (vid % v) as u32,
        ((did >> 8) % v) as u32,
        (did % v) as u32,
    ]
}

/// RMS Normalization inline (evita depender do decoder)
fn rms_norm_hw(x: &mut [f32], weight: &[f32]) {
    let n = x.len();
    let ss: f32 = x.iter().map(|v| v * v).sum::<f32>() / n as f32;
    let rms = libm::sqrtf(ss) + 1e-6;
    for i in 0..n {
        x[i] = x[i] / rms * weight[i.min(weight.len().saturating_sub(1))];
    }
}

/// SwiGLU activation: out = gate * sigmoid(gate) * up
fn swiglu(gate: &[f32], up: &[f32]) -> Vec<f32> {
    gate.iter().zip(up.iter()).map(|(&g, &u)| {
        let sig = 1.0 / (1.0 + libm::expf(-g));
        g * sig * u
    }).collect()
}

/// HW Expert v4: predict structured card from VID/DID.
pub fn predict_hw_v4(model: &HwExpertV4Model, vid: u16, did: u16) -> crate::tensor::HwPrediction {
    use crate::tensor::Tensor;

    let h = model.hidden;
    let tokens = pack_vid_did(vid, did, 64);
    let seq = 4;

    // Embed tokens: [seq, 1] → matmul com [hidden, vocab] → [seq, hidden]
    let mut hidden_vec = vec![0.0f32; seq * h];
    for (ti, &tok) in tokens.iter().enumerate() {
        if (tok as usize) < model.embed.shape.1 {
            // Lookup column da matriz embed
            let col_offset = tok as usize;
            for row in 0..h {
                let idx = col_offset * h + row; // column-major na verdade row-major
                // embed é (hidden, vocab) — cada coluna é um embedding
                let byte_idx = idx / 4;
                let bit_shift = (idx % 4) * 2;
                let packed = model.embed.packed_data.get(byte_idx).copied().unwrap_or(0);
                let sign = ((packed >> bit_shift) & 0x03) as i8;
                let val = match sign {
                    0b01 => 1.0,
                    0b10 => -1.0,
                    _ => 0.0,
                };
                hidden_vec[ti * h + row] = val * model.embed_scale;
            }
        }
    }

    // Transformer layers
    for layer in &model.layers {
        // RMS norm pre-attention
        for pos in 0..seq {
            let start = pos * h;
            rms_norm_hw(&mut hidden_vec[start..start + h], &layer.rms_attn);
        }

        // QKV projections + output (simplificado: projeta cada pos)
        let mut attn_out = vec![0.0f32; seq * model.q_dim];
        for pos in 0..seq {
            let inp_start = pos * h;
            let inp = &hidden_vec[inp_start..inp_start + h];
            let inp_t = Tensor::from_row_major((1, h), inp.to_vec()).unwrap();
            let out_t = layer.o.matmul_hybrid(&layer.v.matmul_hybrid(&inp_t).unwrap()).unwrap();
            let out_start = pos * model.q_dim;
            for j in 0..model.q_dim {
                attn_out[out_start + j] = out_t.data[j];
            }
        }

        // Residual add + RMS norm pre-FFN
        for pos in 0..seq {
            let h_start = pos * h;
            let a_start = pos * model.q_dim;
            for j in 0..h.min(model.q_dim) {
                hidden_vec[h_start + j] += attn_out[a_start + j];
            }
            rms_norm_hw(&mut hidden_vec[h_start..h_start + h], &layer.rms_ffn);
        }

        // SwiGLU FFN
        let mut ffn_out = vec![0.0f32; seq * h];
        for pos in 0..seq {
            let inp_start = pos * h;
            let inp = &hidden_vec[inp_start..inp_start + h];
            let inp_t = Tensor::from_row_major((1, h), inp.to_vec()).unwrap();

            let gate_t = layer.gate.matmul_hybrid(&inp_t).unwrap();
            let up_t = layer.up.matmul_hybrid(&inp_t).unwrap();
            let sw = swiglu(&gate_t.data, &up_t.data);
            let sw_t = Tensor::from_row_major((1, layer.intermediate_size), sw).unwrap();
            let down_t = layer.down.matmul_hybrid(&sw_t).unwrap();

            let out_start = pos * h;
            for j in 0..h.min(down_t.data.len()) {
                ffn_out[out_start + j] = down_t.data[j];
            }
        }

        // Residual add
        for pos in 0..seq {
            let h_start = pos * h;
            for j in 0..h {
                hidden_vec[h_start + j] += ffn_out[h_start + j];
            }
        }
    }

    // Final RMS norm
    for pos in 0..seq {
        let start = pos * h;
        rms_norm_hw(&mut hidden_vec[start..start + h], &model.rms_final);
    }

    // Mean pool sobre seq
    let mut pooled = vec![0.0f32; h];
    for pos in 0..seq {
        let start = pos * h;
        for j in 0..h {
            pooled[j] += hidden_vec[start + j];
        }
    }
    for j in 0..h {
        pooled[j] /= seq as f32;
    }

    // Apply heads (matmul_hybrid)
    let h_t = Tensor::from_row_major((1, h), pooled).unwrap();

    let family_logits = model.family_head.matmul_hybrid(&h_t).unwrap();
    let fw_logits = model.fw_head.matmul_hybrid(&h_t).unwrap();
    let agent_logits = model.agent_head.matmul_hybrid(&h_t).unwrap();
    let caps_logits = model.caps_head.matmul_hybrid(&h_t).unwrap();
    let next_logits = model.next_head.matmul_hybrid(&h_t).unwrap();

    fn argmax(v: &[f32]) -> usize {
        v.iter().enumerate().max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap()).map(|(i, _)| i).unwrap_or(0)
    }

    let family_id = argmax(&family_logits.data) as u8;
    let fw_id = argmax(&fw_logits.data) as u8;
    let agent_id = argmax(&agent_logits.data) as u8;
    let next_action = argmax(&next_logits.data) as u8;

    // Caps: binary threshold at 0.0 (logits, sem sigmoid)
    let mut caps_bits: u32 = 0;
    for k in 0..10 {
        if k < caps_logits.data.len() && caps_logits.data[k] > 0.0 {
            caps_bits |= 1 << k;
        }
    }

    k_nano::slog_cortex!("HWEXPERT", "info", "predict {:04x}:{:04x} → family={} fw={} agent={} caps={:#x} next={}", 
        vid, did, family_id, fw_id, agent_id, caps_bits, next_action);

    crate::tensor::HwPrediction { family_id, fw_id, agent_id, caps_bits, next_action }
}

/// Static para o modelo v5 carregado
pub static HWEXPERT_V4_MODEL: spin::Mutex<Option<HwExpertV4Model>> = spin::Mutex::new(None);

pub fn hwexpert_v4_is_loaded() -> bool {
    HWEXPERT_V4_MODEL.lock().is_some()
}

pub fn set_hwexpert_v4_model(model: HwExpertV4Model) {
    *HWEXPERT_V4_MODEL.lock() = Some(model);
    k_nano::slog_cortex!("HWEXPERT", "info", "HW Expert v4 model loaded (multi-head).");
}

/// Predict HW card from VID/DID using loaded v4 model. Returns None if model not loaded.
pub fn hwexpert_v4_predict(vid: u16, did: u16) -> Option<crate::tensor::HwPrediction> {
    let guard = HWEXPERT_V4_MODEL.lock();
    match guard.as_ref() {
        Some(model) => Some(predict_hw_v4(model, vid, did)),
        None => None,
    }
}

// ─── Fim HW Expert v5 ─────────────────────────────────────────────────

/// Cache de Key/Value para geracao autoregressiva.
/// Armazena K e V por layer, evitando reprocessar tokens anteriores.
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

    pub fn k_dim(&self) -> usize { self.k_dim }

    pub fn advance(&mut self, n: usize) {
        self.len += n;
    }

    pub fn k_all(&self, layer: usize, seq_len: usize) -> Tensor {
        let data = self.k[layer].clone();
        let expected = seq_len * self.k_dim;
        if data.len() != expected {
            k_nano::slog_cortex!("KV", "info", "k_all mismatch: layer={} data.len={} expected={} (seq={} k_dim={})", layer, data.len(), expected, seq_len, self.k_dim);
        }
        Tensor::from_row_major((seq_len, self.k_dim), data).unwrap()
    }

    pub fn v_all(&self, layer: usize, seq_len: usize) -> Tensor {
        let data = self.v[layer].clone();
        let expected = seq_len * self.k_dim;
        if data.len() != expected {
            k_nano::slog_cortex!("KV", "info", "v_all mismatch: layer={} data.len={} expected={} (seq={} k_dim={})", layer, data.len(), expected, seq_len, self.k_dim);
        }
        Tensor::from_row_major((seq_len, self.k_dim), data).unwrap()
    }

    pub fn len(&self) -> usize { self.len }
}

const MEDUSA_HEADS: usize = 3;

pub struct MedusaHead {
    pub w: PackedTernaryTensor,
    pub w_scale: f32,
}

impl MedusaHead {
    pub fn new_random(seed: &mut u32, hidden: usize, vocab: usize) -> Self {
        MedusaHead { w: random_ternary(seed, hidden, vocab), w_scale: 1.0 }
    }

    pub fn forward(&self, hidden: &Tensor) -> Tensor {
        let mut out = self.w.matmul_hybrid(hidden).unwrap();
        out.mul_scalar(self.w_scale);
        out
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
                q_scale: 1.0,
                k: random_ternary(&mut seed, HIDDEN, HIDDEN),
                k_scale: 1.0,
                v: random_ternary(&mut seed, HIDDEN, HIDDEN),
                v_scale: 1.0,
                o: random_ternary(&mut seed, HIDDEN, HIDDEN),
                o_scale: 1.0,
                rms_ffn: rms_default.clone(),
                rms_inner_attn: rms_default.clone(),
                rms_ffn_norm: vec![1.0; FFN_DIM * 2],
                gate: random_ternary(&mut seed, HIDDEN, FFN_DIM),
                gate_scale: 1.0,
                up: random_ternary(&mut seed, HIDDEN, FFN_DIM),
                up_scale: 1.0,
                down: random_ternary(&mut seed, FFN_DIM, HIDDEN),
                down_scale: 1.0,
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
            embed_scale: 1.0,
            layers,
            rms_final: rms_default,
            unembed: random_ternary(&mut seed, HIDDEN, VOCAB_SIZE as usize),
            unembed_scale: 1.0,
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
            act_type: 0,
            embed_type: 0,
            embed_q6k: None,
            rope_theta: 10000.0,
            rope_cos,
            rope_sin,
        }
    }

    fn embed_lookup(&self, token: u32) -> Tensor {
        let t = (token as usize).min(self.embed.shape.1.saturating_sub(1));
        let mut data = Vec::with_capacity(self.hidden);
        if self.embed_type == 1 {
            // Q6_K row-wise (ADR-0085 D6): decode elemento por elemento, sem bulk
            if let Some(q6k) = &self.embed_q6k {
                for row in 0..self.hidden {
                    data.push(crate::gguf::q6k_get(q6k, row * self.embed.shape.1 + t) * self.embed_scale);
                }
            } else {
                return Tensor::zero((1, self.hidden));
            }
        } else {
            for row in 0..self.hidden {
                let idx = row * self.embed.shape.1 + t;
                data.push((self.embed.get_weight(idx) as f32) * self.embed_scale);
            }
        }
        Tensor::from_row_major((1, self.hidden), data).unwrap()
    }

    fn rms_norm_tensor(&self, x: &Tensor, weight: &[f32]) -> Tensor {
        let mut t = Tensor::from_row_major(x.shape, x.data.clone()).unwrap();
        // M2 (ADR-0084 §11.2): 2B4T usa eps 1e-5 (era 1e-6)
        rms_norm(&mut t, weight, 1e-5);
        t
    }

    /// M1 (ADR-0084): ativação FFN por arquivo — act_type 1 = relu2 (2B4T), 0 = silu.
    fn ffn_act(&self, x: f32) -> f32 {
        if self.act_type == 1 { relu2(x) } else { silu(x) }
    }

    /// Logits de unembed (tied usa embed). Q6_K tied → q6k_matmul_row (ADR-0085 D6).
    fn unembed_logits(&self, hidden: &Tensor, vocab: usize) -> Tensor {
        if self.tie_embeddings && self.embed_type == 1 {
            if let Some(q6k) = &self.embed_q6k {
                let data = crate::gguf::q6k_matmul_row(q6k, self.hidden, vocab, &hidden.data);
                let mut t = Tensor::from_row_major((1, vocab), data)
                    .unwrap_or_else(|| Tensor::zero((1, vocab)));
                t.mul_scalar(self.embed_scale);
                return t;
            }
        }
        let mut logits = if self.tie_embeddings {
            self.embed.matmul_hybrid(hidden).unwrap_or_else(|| Tensor::zero((1, vocab)))
        } else {
            self.unembed.matmul_hybrid(hidden).unwrap_or_else(|| Tensor::zero((1, vocab)))
        };
        logits.mul_scalar(if self.tie_embeddings { self.embed_scale } else { self.unembed_scale });
        logits
    }

    pub fn forward_with_kv(&self, tokens: &[u32], cache: &mut KvCache) -> (Tensor, Tensor) {
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
        let mask = Tensor::from_row_major((new_len, total_seq), mask_data).unwrap_or_else(|| Tensor::zero((new_len, total_seq)));

        let _layer_count = self.layers.len();
        // Soft-float 2B: stride=3 (~⅓ layers) libera budget p/ chat frame 8 toks + gen.
        let soft_stride: usize = if self.hidden >= 2048 { 3 } else { 1 };
        if is_first_pass && soft_stride > 1 {
            k_nano::slog_cortex!("FWD", "info", "soft_stride={} layers≈{}/{}",
                soft_stride,
                (_layer_count + soft_stride - 1) / soft_stride,
                _layer_count);
        }
        for (layer_idx, layer) in self.layers.iter().enumerate() {
            if soft_stride > 1 && (layer_idx % soft_stride) != 0 {
                continue;
            }
            if is_first_pass && (layer_idx % 5 == 0 || layer_idx + 1 == _layer_count) {
                k_nano::slog_cortex!("FWD", "info", "layer {}/{}", layer_idx, _layer_count);
            }
            let norm = self.rms_norm_tensor(&x, &layer.rms_attn);

            // QKV for new tokens — fallback silencioso se matmul falhar
            let mut q = layer.q.matmul_hybrid(&norm).unwrap_or_else(|| Tensor::new((new_len, self.kv_dim)));
            q.mul_scalar(layer.q_scale);
            let mut k = layer.k.matmul_hybrid(&norm).unwrap_or_else(|| Tensor::new((new_len, layer.kv_dim)));
            k.mul_scalar(layer.k_scale);
            let mut v = layer.v.matmul_hybrid(&norm).unwrap_or_else(|| Tensor::new((new_len, layer.kv_dim)));
            v.mul_scalar(layer.v_scale);

            // RoPE on Q and K before cache storage
            let qk_head_dim = self.kv_dim / self.num_heads;
            rope_apply_heads(&mut q.data, new_len, self.num_heads, qk_head_dim,
                &self.rope_cos, &self.rope_sin, start_pos);
            rope_apply_heads(&mut k.data, new_len, self.num_kv_heads, qk_head_dim,
                &self.rope_cos, &self.rope_sin, start_pos);

            // Append new K,V to cache (K is RoPE-rotated)
            cache.append(layer_idx, &k, &v);

            // Full K,V from cache for attention
            let total_k = cache.k_all(layer_idx, total_seq);
            let total_v = cache.v_all(layer_idx, total_seq);

            // GQA attention com FlashAttention tiling (#414)
            // Processa atenção em blocos que cabem no cache L1/L2, evitando
            // a matriz de scores completa (new_len × total_seq) que causa
            // cache misses severos para sequências >256 tokens.
            let num_heads = self.num_heads;
            let num_kv_heads = self.num_kv_heads;
            let kv_dim = self.kv_dim;
            let q_group_size = num_heads / num_kv_heads;
            let k_dim = total_k.shape.1;
            let v_dim = total_v.shape.1;
            let mut attn_out_data = vec![0.0f32; new_len * kv_dim];

            // Block size adaptativo: quantos tokens cabem no cache L1/L2
            let block_size = crate::tensor::optimal_attention_block(qk_head_dim);

            for kv_g in 0..num_kv_heads {
                let kv_start = kv_g * qk_head_dim;
                // Extrai K/V heads sob demanda (streaming-friendly)
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

                    // FlashAttention: processa query em blocos que cabem no L1
                    for qb in (0..new_len).step_by(block_size) {
                        let qb_end = (qb + block_size).min(new_len);
                        let qb_len = qb_end - qb;

                        // Carrega Q_block (qb_len × head_dim) — cabe no L1!
                        let mut q_block = Tensor::new((qb_len, qk_head_dim));
                        for s in 0..qb_len {
                            for d in 0..qk_head_dim {
                                q_block.data[s * qk_head_dim + d] =
                                    q.data[(qb + s) * kv_dim + head_start + d];
                            }
                        }

                        // Processa K/V em blocos (streaming da cache)
                        for kb in (0..total_seq).step_by(block_size) {
                            let kb_end = (kb + block_size).min(total_seq);
                            let kb_len = kb_end - kb;

                            // scores = Q_block @ K_block^T (qb_len × kb_len) — cabe no L1!
                            let mut k_block = Tensor::new((kb_len, qk_head_dim));
                            for s in 0..kb_len {
                                for d in 0..qk_head_dim {
                                    k_block.data[s * qk_head_dim + d] =
                                        k_g.data[(kb + s) * qk_head_dim + d];
                                }
                            }
                            let k_block_t = k_block.transposed();
                            let mut scores = q_block.matmul(&k_block_t).unwrap_or_else(|| Tensor::zero((qb_len, kb_len)));
                            let scale = 1.0 / libm::sqrtf(qk_head_dim as f32);

                            // Scale + causal mask
                            let mask_row_start = (qb) * total_seq + kb;
                            for si in 0..qb_len {
                                for sj in 0..kb_len {
                                    let idx = si * kb_len + sj;
                                    scores.data[idx] *= scale;
                                    scores.data[idx] += mask.data[mask_row_start + si * total_seq + sj];
                                }
                            }

                            // Softmax online: streaming softmax sobre blocos
                            // Para simplificar, softmax sobre o bloco com mascara causal
                            for si in 0..qb_len {
                                let start = si * kb_len;
                                let end = start + kb_len;
                                // Mascara causal: tokens futuros = -inf
                                for sj in 0..kb_len {
                                    if (qb + si) < (kb + sj) {
                                        scores.data[start + sj] = -1e9;
                                    }
                                }
                                softmax_inplace(&mut scores.data[start..end]);
                            }

                            // attn_block = scores @ V_block — acumula
                            let mut v_block = Tensor::new((kb_len, qk_head_dim));
                            for s in 0..kb_len {
                                for d in 0..qk_head_dim {
                                    v_block.data[s * qk_head_dim + d] =
                                        v_g.data[(kb + s) * qk_head_dim + d];
                                }
                            }
                            let attn_block = scores.matmul(&v_block).unwrap_or_else(|| Tensor::zero((qb_len, qk_head_dim)));

                            // Acumula no output
                            for s in 0..qb_len {
                                for d in 0..qk_head_dim {
                                    attn_out_data[(qb + s) * kv_dim + head_start + d] +=
                                        attn_block.data[s * qk_head_dim + d];
                                }
                            }
                        }
                    }
                }
            }

            let attn_out = Tensor::from_row_major((new_len, kv_dim), attn_out_data).unwrap_or_else(|| Tensor::zero((new_len, kv_dim)));
            let attn_out_norm = self.rms_norm_tensor(&attn_out, &layer.rms_inner_attn);
            let mut proj = layer.o.matmul_hybrid(&attn_out_norm).unwrap_or_else(|| Tensor::zero((new_len, self.hidden)));
            proj.mul_scalar(layer.o_scale);
            x = x.add(&proj).unwrap_or_else(|| Tensor::zero(x.shape));

            // BitFFN
            let norm2 = self.rms_norm_tensor(&x, &layer.rms_ffn);
            let mut gate = layer.gate.matmul_hybrid(&norm2).unwrap_or_else(|| Tensor::zero((new_len, layer.ffn_group_size)));
            gate.mul_scalar(layer.gate_scale);
            let mut up = layer.up.matmul_hybrid(&norm2).unwrap_or_else(|| Tensor::zero((new_len, layer.ffn_group_size)));
            up.mul_scalar(layer.up_scale);
            let ffn_group = gate.shape.1;
            let mut gated = Tensor::from_row_major(gate.shape, gate.data.clone()).unwrap_or_else(|| Tensor::zero(gate.shape));
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
            let mut down = layer.down.matmul_hybrid(&gated_norm).unwrap_or_else(|| Tensor::zero((new_len, layer.down.shape.1)));
            down.mul_scalar(layer.down_scale);
            for s in 0..new_len {
                for d in 0..down_out.min(self.hidden) {
                    x.data[s * self.hidden + d] += down.data[s * down_out + d];
                }
            }
        }
        // Advance uma vez após layers ativas (compatível com soft_stride)
        cache.advance(new_len);

        let final_norm = self.rms_norm_tensor(&x, &self.rms_final);
        let last_hidden = Tensor::from_row_major((1, self.hidden),
            final_norm.data[(new_len - 1) * self.hidden..new_len * self.hidden].to_vec())
            .unwrap_or_else(|| {
                let start = ((new_len - 1) * self.hidden).min(final_norm.data.len().saturating_sub(1));
                let end = (new_len * self.hidden).min(final_norm.data.len());
                let mut padded = vec![0.0f32; self.hidden];
                for (i, &v) in final_norm.data[start..end].iter().enumerate() {
                    if i < self.hidden { padded[i] = v; }
                }
                Tensor { shape: (1, self.hidden), data: padded }
            });
        let logits = self.unembed_logits(&last_hidden, self.vocab_size as usize);
        (last_hidden, logits)
    }

    /// Forward new tokens with KV cache; returns logits [new_len × vocab] (one row per input token).
    pub fn forward_with_kv_all_logits(&self, tokens: &[u32], cache: &mut KvCache) -> Tensor {
        let seq_len = tokens.len();
        let is_first_pass = cache.len == 0;
        let new_len = if is_first_pass { seq_len.min(self.max_seq) } else { seq_len };
        let total_seq = if is_first_pass { new_len } else { cache.len + seq_len };

        let start_pos = if is_first_pass { 0 } else { cache.len };
        let mut x = Tensor::new((new_len, self.hidden));
        for (i, &t) in tokens.iter().enumerate().take(new_len) {
            let emb = self.embed_lookup(t);
            for j in 0..self.hidden {
                x.data[i * self.hidden + j] = emb.data[j];
            }
        }

        let mut mask_data = vec![0.0f32; new_len * total_seq];
        for i in 0..new_len {
            let global_i = start_pos + i;
            for j in (global_i + 1)..total_seq {
                mask_data[i * total_seq + j] = NEG_INFINITY;
            }
        }
        let mask = Tensor::from_row_major((new_len, total_seq), mask_data).unwrap_or_else(|| Tensor::zero((new_len, total_seq)));

        for (layer_idx, layer) in self.layers.iter().enumerate() {
            let norm = self.rms_norm_tensor(&x, &layer.rms_attn);
            let mut q = layer.q.matmul_hybrid(&norm).unwrap_or_else(|| Tensor::new((new_len, self.kv_dim)));
            q.mul_scalar(layer.q_scale);
            let mut k = layer.k.matmul_hybrid(&norm).unwrap_or_else(|| Tensor::new((new_len, layer.kv_dim)));
            k.mul_scalar(layer.k_scale);
            let mut v = layer.v.matmul_hybrid(&norm).unwrap_or_else(|| Tensor::new((new_len, layer.kv_dim)));
            v.mul_scalar(layer.v_scale);
            let qk_head_dim = self.kv_dim / self.num_heads;
            rope_apply_heads(&mut q.data, new_len, self.num_heads, qk_head_dim,
                &self.rope_cos, &self.rope_sin, start_pos);
            rope_apply_heads(&mut k.data, new_len, self.num_kv_heads, qk_head_dim,
                &self.rope_cos, &self.rope_sin, start_pos);
            cache.append(layer_idx, &k, &v);
            if layer_idx + 1 == self.layers.len() {
                cache.advance(new_len);
            }
            let total_k = cache.k_all(layer_idx, total_seq);
            let total_v = cache.v_all(layer_idx, total_seq);
            let num_heads = self.num_heads;
            let num_kv_heads = self.num_kv_heads;
            let kv_dim = self.kv_dim;
            let q_group_size = num_heads / num_kv_heads;
            let k_dim = total_k.shape.1;
            let v_dim = total_v.shape.1;
            let mut attn_out_data = vec![0.0f32; new_len * kv_dim];
            let block_size = crate::tensor::optimal_attention_block(qk_head_dim);

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
                    for qb in (0..new_len).step_by(block_size) {
                        let qb_end = (qb + block_size).min(new_len);
                        let qb_len = qb_end - qb;
                        let mut q_block = Tensor::new((qb_len, qk_head_dim));
                        for s in 0..qb_len {
                            for d in 0..qk_head_dim {
                                q_block.data[s * qk_head_dim + d] =
                                    q.data[(qb + s) * kv_dim + head_start + d];
                            }
                        }
                        for kb in (0..total_seq).step_by(block_size) {
                            let kb_end = (kb + block_size).min(total_seq);
                            let kb_len = kb_end - kb;
                            let mut k_block = Tensor::new((kb_len, qk_head_dim));
                            for s in 0..kb_len {
                                for d in 0..qk_head_dim {
                                    k_block.data[s * qk_head_dim + d] =
                                        k_g.data[(kb + s) * qk_head_dim + d];
                                }
                            }
                            let k_block_t = k_block.transposed();
                            let mut scores = q_block.matmul(&k_block_t).unwrap_or_else(|| Tensor::zero((qb_len, kb_len)));
                            let scale = 1.0 / libm::sqrtf(qk_head_dim as f32);
                            let mask_row_start = (qb) * total_seq + kb;
                            for si in 0..qb_len {
                                for sj in 0..kb_len {
                                    let idx = si * kb_len + sj;
                                    scores.data[idx] *= scale;
                                    scores.data[idx] += mask.data[mask_row_start + si * total_seq + sj];
                                }
                            }
                            for si in 0..qb_len {
                                let start = si * kb_len;
                                let end = start + kb_len;
                                for sj in 0..kb_len {
                                    if (qb + si) < (kb + sj) {
                                        scores.data[start + sj] = -1e9;
                                    }
                                }
                                softmax_inplace(&mut scores.data[start..end]);
                            }
                            let mut v_block = Tensor::new((kb_len, qk_head_dim));
                            for s in 0..kb_len {
                                for d in 0..qk_head_dim {
                                    v_block.data[s * qk_head_dim + d] =
                                        v_g.data[(kb + s) * qk_head_dim + d];
                                }
                            }
                            let attn_block = scores.matmul(&v_block).unwrap_or_else(|| Tensor::zero((qb_len, qk_head_dim)));
                            for s in 0..qb_len {
                                for d in 0..qk_head_dim {
                                    attn_out_data[(qb + s) * kv_dim + head_start + d] +=
                                        attn_block.data[s * qk_head_dim + d];
                                }
                            }
                        }
                    }
                }
            }

            let attn_out = Tensor::from_row_major((new_len, kv_dim), attn_out_data).unwrap_or_else(|| Tensor::zero((new_len, kv_dim)));
            let attn_out_norm = self.rms_norm_tensor(&attn_out, &layer.rms_inner_attn);
            let mut proj = layer.o.matmul_hybrid(&attn_out_norm).unwrap_or_else(|| Tensor::zero((new_len, self.hidden)));
            proj.mul_scalar(layer.o_scale);
            x = x.add(&proj).unwrap_or_else(|| Tensor::zero(x.shape));
            let norm2 = self.rms_norm_tensor(&x, &layer.rms_ffn);
            let mut gate = layer.gate.matmul_hybrid(&norm2).unwrap_or_else(|| Tensor::zero((new_len, layer.ffn_group_size)));
            gate.mul_scalar(layer.gate_scale);
            let mut up = layer.up.matmul_hybrid(&norm2).unwrap_or_else(|| Tensor::zero((new_len, layer.ffn_group_size)));
            up.mul_scalar(layer.up_scale);
            let ffn_group = gate.shape.1;
            let mut gated = Tensor::from_row_major(gate.shape, gate.data.clone()).unwrap_or_else(|| Tensor::zero(gate.shape));
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
            let mut down = layer.down.matmul_hybrid(&gated_norm).unwrap_or_else(|| Tensor::zero((new_len, layer.down.shape.1)));
            down.mul_scalar(layer.down_scale);
            for s in 0..new_len {
                for d in 0..down_out.min(self.hidden) {
                    x.data[s * self.hidden + d] += down.data[s * down_out + d];
                }
            }
        }

        let final_norm = self.rms_norm_tensor(&x, &self.rms_final);
        let vocab_size = self.vocab_size as usize;
        let mut all_logits = vec![0.0f32; new_len * vocab_size];
        for i in 0..new_len {
            let hidden = Tensor::from_row_major((1, self.hidden),
                final_norm.data[i * self.hidden..(i + 1) * self.hidden].to_vec())
                .unwrap_or_else(|| Tensor::zero((1, self.hidden)));
            let logits = self.unembed_logits(&hidden, vocab_size);
            for j in 0..vocab_size {
                all_logits[i * vocab_size + j] = logits.data[j];
            }
        }
        Tensor::from_row_major((new_len, vocab_size), all_logits).unwrap_or_else(|| Tensor::zero((new_len, vocab_size)))
    }

    /// AirLLM: apply one transformer layer then return; caller drops weights.
    /// Uses the same attention/FFN path as forward_with_kv (no soft_stride skip).
    pub fn apply_one_layer(
        &self,
        layer_idx: usize,
        layer: &LayerWeights,
        x: &mut Tensor,
        cache: &mut KvCache,
        start_pos: usize,
        new_len: usize,
        total_seq: usize,
        mask: &Tensor,
    ) {
        let norm = self.rms_norm_tensor(x, &layer.rms_attn);

        let mut q = layer.q.matmul_hybrid(&norm).unwrap_or_else(|| Tensor::new((new_len, self.kv_dim)));
        q.mul_scalar(layer.q_scale);
        let mut k = layer.k.matmul_hybrid(&norm).unwrap_or_else(|| Tensor::new((new_len, layer.kv_dim)));
        k.mul_scalar(layer.k_scale);
        let mut v = layer.v.matmul_hybrid(&norm).unwrap_or_else(|| Tensor::new((new_len, layer.kv_dim)));
        v.mul_scalar(layer.v_scale);

        let qk_head_dim = self.kv_dim / self.num_heads.max(1);
        rope_apply_heads(&mut q.data, new_len, self.num_heads, qk_head_dim,
            &self.rope_cos, &self.rope_sin, start_pos);
        rope_apply_heads(&mut k.data, new_len, self.num_kv_heads, qk_head_dim,
            &self.rope_cos, &self.rope_sin, start_pos);

        cache.append(layer_idx, &k, &v);

        let total_k = cache.k_all(layer_idx, total_seq);
        let total_v = cache.v_all(layer_idx, total_seq);

        let num_heads = self.num_heads.max(1);
        let num_kv_heads = self.num_kv_heads.max(1);
        let kv_dim = self.kv_dim;
        let q_group_size = (num_heads / num_kv_heads).max(1);
        let k_dim = total_k.shape.1;
        let v_dim = total_v.shape.1;
        let mut attn_out_data = vec![0.0f32; new_len * kv_dim];
        let block_size = crate::tensor::optimal_attention_block(qk_head_dim);

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

                for qb in (0..new_len).step_by(block_size) {
                    let qb_end = (qb + block_size).min(new_len);
                    let qb_len = qb_end - qb;
                    let mut q_block = Tensor::new((qb_len, qk_head_dim));
                    for s in 0..qb_len {
                        for d in 0..qk_head_dim {
                            q_block.data[s * qk_head_dim + d] =
                                q.data[(qb + s) * kv_dim + head_start + d];
                        }
                    }

                    for kb in (0..total_seq).step_by(block_size) {
                        let kb_end = (kb + block_size).min(total_seq);
                        let kb_len = kb_end - kb;
                        let mut k_block = Tensor::new((kb_len, qk_head_dim));
                        for s in 0..kb_len {
                            for d in 0..qk_head_dim {
                                k_block.data[s * qk_head_dim + d] =
                                    k_g.data[(kb + s) * qk_head_dim + d];
                            }
                        }
                        let k_block_t = k_block.transposed();
                        let mut scores = q_block.matmul(&k_block_t).unwrap();
                        let scale = 1.0 / libm::sqrtf(qk_head_dim as f32);
                        let mask_row_start = qb * total_seq + kb;
                        for si in 0..qb_len {
                            for sj in 0..kb_len {
                                let idx = si * kb_len + sj;
                                scores.data[idx] *= scale;
                                scores.data[idx] += mask.data[mask_row_start + si * total_seq + sj];
                            }
                        }
                        for si in 0..qb_len {
                            let start = si * kb_len;
                            let end = start + kb_len;
                            for sj in 0..kb_len {
                                if (qb + si) < (kb + sj) {
                                    scores.data[start + sj] = -1e9;
                                }
                            }
                            softmax_inplace(&mut scores.data[start..end]);
                        }
                        let mut v_block = Tensor::new((kb_len, qk_head_dim));
                        for s in 0..kb_len {
                            for d in 0..qk_head_dim {
                                v_block.data[s * qk_head_dim + d] =
                                    v_g.data[(kb + s) * qk_head_dim + d];
                            }
                        }
                        let attn_block = scores.matmul(&v_block).unwrap();
                        for s in 0..qb_len {
                            for d in 0..qk_head_dim {
                                attn_out_data[(qb + s) * kv_dim + head_start + d] +=
                                    attn_block.data[s * qk_head_dim + d];
                            }
                        }
                    }
                }
            }
        }

        let attn_out = Tensor::from_row_major((new_len, kv_dim), attn_out_data).unwrap();
        let attn_out_norm = self.rms_norm_tensor(&attn_out, &layer.rms_inner_attn);
        let mut proj = layer.o.matmul_hybrid(&attn_out_norm).unwrap_or_else(|| Tensor::new((new_len, self.hidden)));
        proj.mul_scalar(layer.o_scale);
        if let Some(summed) = x.add(&proj) {
            *x = summed;
        }

        let norm2 = self.rms_norm_tensor(x, &layer.rms_ffn);
        let mut gate = layer.gate.matmul_hybrid(&norm2).unwrap_or_else(|| Tensor::new((new_len, layer.ffn_group_size)));
        gate.mul_scalar(layer.gate_scale);
        let mut up = layer.up.matmul_hybrid(&norm2).unwrap_or_else(|| Tensor::new((new_len, layer.ffn_group_size)));
        up.mul_scalar(layer.up_scale);
        let ffn_group = gate.shape.1.max(1);
        let mut gated = Tensor::from_row_major(gate.shape, gate.data.clone()).unwrap();
        for (i, g) in gated.data.iter_mut().enumerate() {
            *g = self.ffn_act(*g) * up.data.get(i).copied().unwrap_or(0.0);
        }

        let intermediate_size = layer.intermediate_size.max(ffn_group);
        let down_out = layer.down.shape.1;
        let num_groups = (intermediate_size / ffn_group).max(1);
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
        let mut down = layer.down.matmul_hybrid(&gated_norm)
            .unwrap_or_else(|| Tensor::new((new_len, down_out.max(1))));
        down.mul_scalar(layer.down_scale);
        for s in 0..new_len {
            for d in 0..down_out.min(self.hidden) {
                x.data[s * self.hidden + d] += down.data[s * down_out + d];
            }
        }
    }

    /// Embed new tokens for a KV forward pass (AirLLM helper).
    pub fn embed_for_kv(&self, tokens: &[u32], cache: &KvCache) -> (Tensor, Tensor, usize, usize, usize) {
        let seq_len = tokens.len();
        let is_first_pass = cache.len == 0;
        let new_len = if is_first_pass { seq_len.min(self.max_seq) } else { seq_len };
        let total_seq = if is_first_pass { new_len } else { cache.len + seq_len };
        let start_pos = if is_first_pass { 0 } else { cache.len };

        let mut x = Tensor::new((new_len, self.hidden));
        for (i, &t) in tokens.iter().enumerate().take(new_len) {
            let emb = self.embed_lookup(t);
            for j in 0..self.hidden {
                x.data[i * self.hidden + j] = emb.data[j];
            }
        }

        let mut mask_data = vec![0.0f32; new_len * total_seq];
        for i in 0..new_len {
            let global_i = start_pos + i;
            for j in (global_i + 1)..total_seq {
                mask_data[i * total_seq + j] = NEG_INFINITY;
            }
        }
        let mask = Tensor::from_row_major((new_len, total_seq), mask_data).unwrap();
        (x, mask, start_pos, new_len, total_seq)
    }

    /// Final RMS + unembed after all layers (AirLLM helper).
    pub fn finalize_logits(&self, x: &Tensor, new_len: usize) -> (Tensor, Tensor) {
        let final_norm = self.rms_norm_tensor(x, &self.rms_final);
        let last_hidden = Tensor::from_row_major(
            (1, self.hidden),
            final_norm.data[(new_len - 1) * self.hidden..new_len * self.hidden].to_vec(),
        ).unwrap();
        let logits = self.unembed_logits(&last_hidden, self.vocab_size as usize);
        (last_hidden, logits)
    }

    pub fn forward_hidden(&self, tokens: &[u32]) -> (Tensor, Tensor) {
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
        let mask = Tensor::from_row_major((seq_len, seq_len), mask_data).unwrap_or_else(|| Tensor::zero((seq_len, seq_len)));

        let layer_count = self.layers.len();
        for (layer_idx, layer) in self.layers.iter().enumerate() {
            let lt0 = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            let norm = self.rms_norm_tensor(&x, &layer.rms_attn);

            // QKV projections with GQA dimensions
            let t_q0 = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            let mut q = layer.q.matmul_hybrid(&norm).unwrap_or_else(|| Tensor::zero((seq_len, self.kv_dim)));  // (seq, kv_dim)
            q.mul_scalar(layer.q_scale);
            let t_q1 = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            let mut k = layer.k.matmul_hybrid(&norm).unwrap_or_else(|| Tensor::zero((seq_len, layer.kv_dim)));  // (seq, k_dim)
            k.mul_scalar(layer.k_scale);
            let _t_k1 = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            let mut v = layer.v.matmul_hybrid(&norm).unwrap_or_else(|| Tensor::zero((seq_len, layer.kv_dim)));  // (seq, k_dim)
            v.mul_scalar(layer.v_scale);
            let t_v1 = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);

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

                // FlashAttention tiling adaptativo (#414)
                let block_size = crate::tensor::optimal_attention_block(qk_head_dim);
                for qh in 0..q_group_size {
                    let head_idx = kv_g * q_group_size + qh;
                    let head_start = head_idx * qk_head_dim;

                    for qb in (0..seq_len).step_by(block_size) {
                        let qb_end = (qb + block_size).min(seq_len);
                        let qb_len = qb_end - qb;

                        let mut q_block = Tensor::new((qb_len, qk_head_dim));
                        for s in 0..qb_len {
                            for d in 0..qk_head_dim {
                                q_block.data[s * qk_head_dim + d] =
                                    q.data[(qb + s) * kv_dim + head_start + d];
                            }
                        }

                        for kb in (0..seq_len).step_by(block_size) {
                            let kb_end = (kb + block_size).min(seq_len);
                            let kb_len = kb_end - kb;

                            let mut k_block = Tensor::new((kb_len, qk_head_dim));
                            for s in 0..kb_len {
                                for d in 0..qk_head_dim {
                                    k_block.data[s * qk_head_dim + d] =
                                        k_g.data[(kb + s) * qk_head_dim + d];
                                }
                            }
                            let k_block_t = k_block.transposed();
                            let mut scores = q_block.matmul(&k_block_t).unwrap_or_else(|| Tensor::zero((qb_len, kb_len)));
                            let scale = 1.0 / libm::sqrtf(qk_head_dim as f32);

                            for si in 0..qb_len {
                                for sj in 0..kb_len {
                                    let idx = si * kb_len + sj;
                                    scores.data[idx] *= scale;
                                    scores.data[idx] += mask.data[(qb + si) * seq_len + kb + sj];
                                }
                            }

                            for si in 0..qb_len {
                                let start = si * kb_len;
                                for sj in 0..kb_len {
                                    if (qb + si) < (kb + sj) {
                                        scores.data[start + sj] = -1e9;
                                    }
                                }
                                softmax_inplace(&mut scores.data[start..start + kb_len]);
                            }

                            let mut v_block = Tensor::new((kb_len, qk_head_dim));
                            for s in 0..kb_len {
                                for d in 0..qk_head_dim {
                                    v_block.data[s * qk_head_dim + d] =
                                        v_g.data[(kb + s) * qk_head_dim + d];
                                }
                            }
                            let attn_block = scores.matmul(&v_block).unwrap_or_else(|| Tensor::zero((qb_len, qk_head_dim)));

                            for s in 0..qb_len {
                                for d in 0..qk_head_dim {
                                    attn_out_data[(qb + s) * kv_dim + head_start + d] +=
                                        attn_block.data[s * qk_head_dim + d];
                                }
                            }
                        }
                    }
                }
            }

            let t_attn1 = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);

            let attn_out = Tensor::from_row_major((seq_len, kv_dim), attn_out_data).unwrap_or_else(|| Tensor::zero((seq_len, kv_dim)));
            let attn_out_norm = self.rms_norm_tensor(&attn_out, &layer.rms_inner_attn);
            let mut proj = layer.o.matmul_hybrid(&attn_out_norm).unwrap_or_else(|| Tensor::zero((seq_len, self.hidden)));
            proj.mul_scalar(layer.o_scale);
            x = x.add(&proj).unwrap_or_else(|| Tensor::zero(x.shape));
            let t_proj1 = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);

            // BitFFN
            let norm2 = self.rms_norm_tensor(&x, &layer.rms_ffn);
            let mut gate = layer.gate.matmul_hybrid(&norm2).unwrap_or_else(|| Tensor::zero((seq_len, layer.ffn_group_size)));
            gate.mul_scalar(layer.gate_scale);
            let mut up = layer.up.matmul_hybrid(&norm2).unwrap_or_else(|| Tensor::zero((seq_len, layer.ffn_group_size)));
            up.mul_scalar(layer.up_scale);
            let t_ffn1 = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            let ffn_group = gate.shape.1;
            let mut gated = Tensor::from_row_major(gate.shape, gate.data.clone()).unwrap_or_else(|| Tensor::zero(gate.shape));
            for (i, g) in gated.data.iter_mut().enumerate() {
                *g = self.ffn_act(*g) * up.data[i];
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
            let mut down = layer.down.matmul_hybrid(&gated_norm).unwrap_or_else(|| Tensor::zero((seq_len, layer.down.shape.1)));
            down.mul_scalar(layer.down_scale);

            // Add FFN output to residual (first down_out dims)
            for s in 0..seq_len {
                for d in 0..down_out.min(self.hidden) {
                    x.data[s * self.hidden + d] += down.data[s * down_out + d];
                }
            }

            let lt1 = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            if layer_idx == 0 {
                k_nano::slog_cortex!("FWD", "info", "L0 qkv:{} attn:{} proj:{} ffn_gateup:{} down:{} total:{}",
                    t_q1 - t_q0, t_attn1 - t_v1, t_proj1 - t_attn1, t_ffn1 - t_proj1, lt1 - t_ffn1, lt1 - lt0);
            }
            if lt1 - lt0 > 5 || layer_idx == 0 || layer_idx + 1 == layer_count {
                k_nano::slog_cortex!("FWD", "info", "layer {}/{}: {} ticks", layer_idx + 1, layer_count, lt1 - lt0);
            }
        }

        let final_norm = self.rms_norm_tensor(&x, &self.rms_final);
        let last_hidden = Tensor::from_row_major((1, self.hidden),
            final_norm.data[(seq_len - 1) * self.hidden..seq_len * self.hidden].to_vec())
            .unwrap_or_else(|| {
                let start = ((seq_len - 1) * self.hidden).min(final_norm.data.len().saturating_sub(1));
                let end = (seq_len * self.hidden).min(final_norm.data.len());
                let mut padded = vec![0.0f32; self.hidden];
                for (i, &v) in final_norm.data[start..end].iter().enumerate() {
                    if i < self.hidden { padded[i] = v; }
                }
                Tensor { shape: (1, self.hidden), data: padded }
            });
        let t_unembed0 = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
        let logits = self.unembed_logits(&last_hidden, self.vocab_size as usize);
        let t_unembed1 = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
        if t_unembed1 - t_unembed0 > 10 {
            k_nano::slog_cortex!("FWD", "info", "unembed: {} ticks", t_unembed1 - t_unembed0);
        }
        (last_hidden, logits)
    }

    pub fn forward(&self, tokens: &[u32]) -> Tensor {
        self.forward_hidden(tokens).1
    }

pub fn generate_next(&self, tokens: &[u32]) -> u32 {
    let logits = self.forward(tokens);
    argmax_row(&logits, 0)
}

pub fn sample(&self, tokens: &[u32], top_k: usize, temperature: f32) -> u32 {
    let logits = self.forward(tokens);
    let mut probs: Vec<(usize, f32)> = logits.data.iter().enumerate()
        .map(|(i, &v)| (i, v / temperature.max(0.01))).collect();

    if top_k > 0 && top_k < probs.len() {
        probs.select_nth_unstable_by(top_k - 1, |a, b| {
            if b.1 > a.1 { core::cmp::Ordering::Less }
            else if b.1 < a.1 { core::cmp::Ordering::Greater }
            else { core::cmp::Ordering::Equal }
        });
        probs.truncate(top_k);
    }
    let max_logit = probs.iter().map(|(_, v)| *v).fold(NEG_INFINITY, |a, b| a.max(b));
    let mut sum = 0.0f32;
    for (_, v) in probs.iter_mut() { *v = libm::expf(*v - max_logit); sum += *v; }
    let mut r = (sum * 0.5 + 0.5).max(0.0).min(sum); // deterministic for no_std
    for &(idx, prob) in &probs {
        let p = prob / sum;
        r -= p;
        if r <= 0.0 { return idx as u32; }
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

fn read_u64(data: &[u8], offset: &mut usize) -> Option<u64> {
    if *offset + 8 > data.len() { return None; }
    let v = u64::from_le_bytes(data[*offset..*offset + 8].try_into().ok()?);
    *offset += 8;
    Some(v)
}

fn read_u32(data: &[u8], offset: &mut usize) -> Option<u32> {
    if *offset + 4 > data.len() { return None; }
    let bytes = data[*offset..*offset + 4].try_into().ok()?;
    *offset += 4;
    Some(u32::from_le_bytes(bytes))
}

fn read_ternary_tensor(data: &[u8], offset: &mut usize, rows: usize, cols: usize) -> Option<PackedTernaryTensor> {
    let product = rows.checked_mul(cols).unwrap_or(0);
    if product == 0 { return None; }
    let count = (product + 3) / 4;
    if *offset + count > data.len() { return None; }
    let packed = data[*offset..*offset + count].to_vec();
    *offset += count;
    Some(PackedTernaryTensor { shape: (rows, cols), packed_data: packed })
}

fn read_ternary_tensor_with_scale(data: &[u8], offset: &mut usize, rows: usize, cols: usize) -> Option<(PackedTernaryTensor, f32)> {
    let packed = read_ternary_tensor(data, offset, rows, cols)?;
    let scale = read_f32(data, offset)?;
    Some((packed, scale))
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
    // v4: u32 num_params; v5+: u64 num_params
    let _num_params = if version >= 5 {
        read_u64(data, &mut off)? as u64
    } else {
        read_u32(data, &mut off)? as u64
    };
    let hidden = read_u16(data, &mut off)? as usize;
    let num_layers = read_u16(data, &mut off)? as usize;
    // Auto-expand heap based on header (before main parsing)
    {
        let _nh = read_u16(data, &mut off)? as usize;
        let vs = read_u32(data, &mut off)? as usize;
        let _ms = read_u16(data, &mut off)? as usize;
        let isize = read_u16(data, &mut off)? as usize;
        let embed_bytes = (hidden * vs / 4) as u64;
        // Naive (v1 dense) — superestima GQA/BitFFN v3+ e forçava resize 900MB+
        // (= ~100k map_page no TCG → hang sem log). v3+: pesos packed ≈ arquivo.
        let layer_bytes = (4u64 * hidden as u64 * hidden as u64 / 4
            + 3u64 * hidden as u64 * isize as u64 / 4)
            * num_layers as u64;
        let unembed_bytes = (hidden as u64 * vs as u64 / 4) as u64;
        let naive_mb = ((embed_bytes + layer_bytes + unembed_bytes) / (1024 * 1024)) as usize;
        let file_mb = (data.len() + 1024 * 1024 - 1) / (1024 * 1024);
        let estimated = if version >= 3 {
            // Ternário packed + headroom. Para modelos grandes (BITNET2B:
            // intermediate=12800, h=2560, L=30 → ~861MB de tensores), o
            // file_mb+64 era insuficiente (OOM → Vec null → #PF CR2=0).
            // Dobrar cobre BITNET2B (2×577=1154 > 861 com margem).
            // LLAMA8B (1915MB) continua excluído (cap 2048MB, total seria
            // 1915+3830+64=5809 > cap — precisa AirLLM/GGUF streaming).
            file_mb.saturating_add(file_mb)
        } else {
            naive_mb
        };
        let cur_mb = k_nano::allocator::CURRENT_HEAP_MB.load(core::sync::atomic::Ordering::Relaxed);
        k_nano::slog_cortex!("LLM", "info", "load_model ver={} h={} L={} file={}MB est={}MB heap={}MB", version, hidden, num_layers, file_mb, estimated, cur_mb);
        // estimated = quanto load_model precisa alocar (tensors). Mas o arquivo
        // ja esta no heap (file_mb). Total real = file_mb + estimated.
        let total_needed = file_mb + estimated + 64;
        if total_needed > cur_mb {
            let total_mb = total_needed.min(4096); // cap 4GB (llama8b: 1915MB×2≈3.9GB)
            k_nano::slog_cortex!("LLM", "info", "resize_heap {} → {} MB (file={} est={})...", cur_mb, total_mb, file_mb, estimated);
            k_nano::allocator::resize_heap_to_mb(total_mb);
            k_nano::slog_cortex!("LLM", "info", "resize_heap done");
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
    let num_medusa;
    let mut tie_embeddings = false;

    if version >= 3 {
        intermediate_size = read_u16(data, &mut off)? as usize;
        num_kv_heads = read_u16(data, &mut off)? as usize;
        let mut q_dim = read_u16(data, &mut off)? as usize;  // Q projection output dim
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
            k_nano::slog_cortex!("BPE", "info", "Tokenizer data: {} bytes, starts {:02x?}", tok_len, first);
            // BPE tokenizer skipped for v3 (large tokenizer needs proper JSON parser)
        }
        off += tok_len;

        // v4: layer_features byte (bit 0 = inner_attn_ln, bit 1 = ffn_layernorm, bit 2 = RoPE)
        let layer_features = if version >= 4 { read_u8(data, &mut off)? } else { 0u8 };
        let has_inner_attn_ln = (layer_features & 0x01) != 0;
        let has_ffn_layernorm = (layer_features & 0x02) != 0;
        let has_rope = (layer_features & 0x04) != 0;

        // BitNet-b1.58-2B-4T: HF packed shape (out/4,in) → q_dim=2560 (head_dim=128).
        // Dump legado ~203MB (q_dim header 2560 mas pesos 640) → corrigir só se ficheiro cabe.
        {
            let k_try = num_kv_heads * (q_dim / num_heads.max(1));
            let ffn_try = intermediate_size * q_dim / hidden.max(1);
            let tern_try = (hidden * q_dim + 3) / 4
                + 2 * ((hidden * k_try + 3) / 4)
                + (q_dim * hidden + 3) / 4
                + 2 * ((hidden * ffn_try + 3) / 4)
                + (intermediate_size * q_dim + 3) / 4;
            let need = (hidden * vocab_size as usize + 3) / 4 + tern_try * num_layers;
            if need > data.len().saturating_add(data.len() / 8)
                && hidden == 2560
                && num_heads == 20
                && num_kv_heads == 5
                && q_dim == hidden
            {
                k_nano::slog_cortex!("LLM", "info", "q_dim header {} → 640 (legacy dump ~203MB; need~{}MB)",
                    q_dim,
                    need / (1024 * 1024));
                q_dim = 640;
            }
        }

        let (embed, embed_scale) = read_ternary_tensor_with_scale(data, &mut off, hidden, vocab_size as usize)?;

        // GQA/BitFFN dimensions from header (2B: q_dim=2560, head_dim=128, k_dim=640)
        let kv_head_dim = q_dim / num_heads.max(1);
        let k_dim = num_kv_heads * kv_head_dim;
        let ffn_group = intermediate_size * q_dim / hidden.max(1);
        let down_out = q_dim;

        // Alguns dumps omitem vetores RMS f32; escolhe layout que fecha no ficheiro.
        // (sem closures — soft-float / LLVM "offset not multiple of 16")
        let tern_per = (hidden * q_dim + 3) / 4
            + 2 * ((hidden * k_dim + 3) / 4)
            + (q_dim * hidden + 3) / 4
            + 2 * ((hidden * ffn_group + 3) / 4)
            + (intermediate_size * down_out + 3) / 4;
        let rem = data.len().saturating_sub(off);
        // v4 + prepare_extra_models: SEMPRE grava input/post RMS; feat bits = sub-norms
        // em tamanho `hidden` (ones). Heuristica rem/need preferia rms=0 → #PF no FWD.
        let (has_basic_rms, best_d) = if version >= 4 {
            (true, 0usize)
        } else {
            let mut best_basic = true;
            let mut best_d = usize::MAX;
            let mut bi = 0u8;
            while bi < 2 {
                let basic = bi == 0;
                let mut per = tern_per;
                if basic {
                    per = per.saturating_add(hidden.saturating_mul(8));
                }
                if has_inner_attn_ln {
                    per = per.saturating_add(hidden.saturating_mul(4));
                }
                if has_ffn_layernorm {
                    per = per.saturating_add(hidden.saturating_mul(4));
                }
                let need = per.saturating_mul(num_layers);
                let d = if rem > need { rem - need } else { need - rem };
                if d < best_d {
                    best_d = d;
                    best_basic = basic;
                }
                bi += 1;
            }
            (best_basic, best_d)
        };
        // Nao sobrescrever feat bits com heuristica (inner/ffn).
        k_nano::slog_cortex!("LLM", "info", "q_dim={} head_dim={} k_dim={} ffn_g={} layout rms={} inner={} ffn_ln={} rem={}KB d={}KB",
            q_dim,
            kv_head_dim,
            k_dim,
            ffn_group,
            has_basic_rms as u8,
            has_inner_attn_ln as u8,
            has_ffn_layernorm as u8,
            rem / 1024,
            best_d / 1024);

        let mut layers = Vec::with_capacity(num_layers);
        for li in 0..num_layers {
            if li % 5 == 0 || li + 1 == num_layers {
                k_nano::slog_cortex!("LLM", "info", "loading layer {}/{} off={}KB", li, num_layers, off / 1024);
            }
            let rms_attn = if has_basic_rms {
                read_f32_vec(data, &mut off, hidden)?
            } else {
                vec![1.0; hidden]
            };
            let rms_ffn = if has_basic_rms {
                read_f32_vec(data, &mut off, hidden)?
            } else {
                vec![1.0; hidden]
            };
            let rms_inner_attn = if has_inner_attn_ln {
                // prepare_extra_models grava `hidden` (nao so kv*heads quando diverge)
                read_f32_vec(data, &mut off, hidden)?
            } else {
                vec![1.0; kv_head_dim * num_heads]
            };
            let rms_ffn_norm = if has_ffn_layernorm {
                // Blobs atuais: ones(hidden). Forward precisa intermediate — pad.
                let v = read_f32_vec(data, &mut off, hidden)?;
                if v.len() == intermediate_size {
                    v
                } else {
                    let mut out = vec![1.0f32; intermediate_size];
                    let n = core::cmp::min(v.len(), out.len());
                    out[..n].copy_from_slice(&v[..n]);
                    out
                }
            } else {
                vec![1.0; intermediate_size]
            };
            let (q, q_scale) = read_ternary_tensor_with_scale(data, &mut off, hidden, q_dim)?;
            let (k, k_scale) = read_ternary_tensor_with_scale(data, &mut off, hidden, k_dim)?;
            let (v, v_scale) = read_ternary_tensor_with_scale(data, &mut off, hidden, k_dim)?;
            let (o, o_scale) = read_ternary_tensor_with_scale(data, &mut off, q_dim, hidden)?;
            let (gate, gate_scale) = read_ternary_tensor_with_scale(data, &mut off, hidden, ffn_group)?;
            let (up, up_scale) = read_ternary_tensor_with_scale(data, &mut off, hidden, ffn_group)?;
            let (down, down_scale) = read_ternary_tensor_with_scale(data, &mut off, intermediate_size, down_out)?;
            layers.push(LayerWeights {
                rms_attn,
                q, q_scale,
                k, k_scale,
                v, v_scale,
                o, o_scale,
                rms_ffn,
                rms_inner_attn,
                rms_ffn_norm,
                gate, gate_scale,
                up, up_scale,
                down, down_scale,
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
        let (unembed, unembed_scale) = if !tie_embeddings && off + expected <= data.len() {
            // Check first 16 bytes are non-zero (zero = past file end = tied)
            let is_zeroed = data[off..(off + 16).min(data.len())].iter().all(|&b| b == 0);
            if is_zeroed {
                tie_embeddings = true;
                (PackedTernaryTensor { shape: (hidden, vocab_size as usize), packed_data: vec![0u8; expected] }, 1.0)
            } else {
                read_ternary_tensor_with_scale(data, &mut off, hidden, vocab_size as usize)?
            }
        } else {
            tie_embeddings = true;
            (PackedTernaryTensor { shape: (hidden, vocab_size as usize), packed_data: vec![0u8; expected] }, 1.0)
        };

        let mut medusa_heads = Vec::with_capacity(num_medusa);
        if num_medusa > 0 {
            for _ in 0..num_medusa {
                let (w, w_scale) = read_ternary_tensor_with_scale(data, &mut off, hidden, vocab_size as usize)?;
                medusa_heads.push(MedusaHead { w, w_scale });
            }
        }

        // BitNet attn precisa RoPE. feat bit2 = theta no EOF; senão default 10000.
        // Nunca confiar em theta<=1 (lixo pós-pesos / soft-float print edge).
        let rope_seq = (max_seq as usize).min(2048).max(64);
        let mut theta = 10000.0f32;
        if has_rope && off + 4 <= data.len() {
            if let Some(t) = read_f32(data, &mut off) {
                if t > 1.0 {
                    theta = t;
                }
            }
        }
        k_nano::slog_cortex!("LLM", "info", "RoPE precompute seq={} theta={} feat_rope={}", rope_seq, theta as u32, has_rope as u8);
        let (rope_cos, rope_sin) = rope_precompute(rope_seq, kv_head_dim, theta);

        k_nano::slog_cortex!("LLM", "info", "model OK layers={} q_dim={} tied={} off={}KB", num_layers, q_dim, tie_embeddings as u8, off / 1024);

        let model = TransformerModel {
            embed, embed_scale, layers, rms_final, unembed, unembed_scale, medusa_heads,
            vocab_size, hidden, num_layers, max_seq: max_seq as usize,
            num_heads, num_kv_heads, head_dim: kv_head_dim, kv_dim: q_dim,
            intermediate_size,
            ffn_group_size: ffn_group,
            tie_embeddings,
            act_type: 0,
            embed_type: 0,
            embed_q6k: None,
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
            if off + 4 > data.len() { return None; }
            let v = data[off] as usize; off += 4;
            v
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
                q_scale: 1.0,
                k: read_ternary_tensor(data, &mut off, hidden, k_dim)?,
                k_scale: 1.0,
                v: read_ternary_tensor(data, &mut off, hidden, k_dim)?,
                v_scale: 1.0,
                o: read_ternary_tensor(data, &mut off, q_dim, hidden)?,
                o_scale: 1.0,
                rms_ffn,
                rms_inner_attn,
                rms_ffn_norm,
                gate: read_ternary_tensor(data, &mut off, hidden, ffn_group)?,
                gate_scale: 1.0,
                up: read_ternary_tensor(data, &mut off, hidden, ffn_group)?,
                up_scale: 1.0,
                down: read_ternary_tensor(data, &mut off, intermediate_size, down_out)?,
                down_scale: 1.0,
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
                q_scale: 1.0,
                k: read_ternary_tensor(data, &mut off, hidden, hidden)?,
                k_scale: 1.0,
                v: read_ternary_tensor(data, &mut off, hidden, hidden)?,
                v_scale: 1.0,
                o: read_ternary_tensor(data, &mut off, hidden, hidden)?,
                o_scale: 1.0,
                rms_ffn,
                rms_inner_attn,
                rms_ffn_norm,
                gate: read_ternary_tensor(data, &mut off, ffn_dim, hidden)?,
                gate_scale: 1.0,
                up: read_ternary_tensor(data, &mut off, ffn_dim, hidden)?,
                up_scale: 1.0,
                down: read_ternary_tensor(data, &mut off, hidden, ffn_dim)?,
                down_scale: 1.0,
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
            medusa_heads.push(MedusaHead { w, w_scale: 1.0 });
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
        embed, embed_scale: 1.0, layers, rms_final, unembed, unembed_scale: 1.0, medusa_heads,
        vocab_size, hidden, num_layers, max_seq: max_seq as usize,
        num_heads, num_kv_heads, head_dim, kv_dim: q_dim,
        intermediate_size,
        ffn_group_size: intermediate_size / 4,
        tie_embeddings,
        act_type: 0,
        embed_type: 0,
        embed_q6k: None,
        rope_theta: 10000.0,
        rope_cos: vec![],
        rope_sin: vec![],
    };
    GLOBAL_MODEL_PARAMS.store(_num_params as u64, core::sync::atomic::Ordering::Relaxed);
    Some(model)
}

// ── Serialização inversa de load_model (formato .bitnet v4) ────────────────
// save_model grava SEMPRE version=4: o loader (cortex.rs:1771) fixa
// has_basic_rms=true para v4, eliminando a heurística de layout RMS do v3
// (rms_attn/rms_ffn sempre presentes). num_params é u32 em v4 — o reset de
// offset do loader (off = 4+2+4+2+2) só fecha para esse layout.
fn write_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn write_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn write_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn write_f32(out: &mut Vec<u8>, v: f32) {
    out.extend_from_slice(&v.to_le_bytes());
}
/// Grava exatamente `n` f32s (o loader lê `n` fixos por campo); padding 1.0
/// se o vetor for mais curto (nunca acontece p/ modelos vindos de load_model).
fn write_f32_vec_clamped(out: &mut Vec<u8>, v: &[f32], n: usize) {
    for i in 0..n {
        write_f32(out, v.get(i).copied().unwrap_or(1.0));
    }
}
fn tern_packed_len(rows: usize, cols: usize) -> usize {
    (rows * cols + 3) / 4
}

/// Serializa o modelo no formato .bitnet v4 — inverso exato de `load_model`.
/// Round-trip garantido: `load_model(&save_model(m))` reproduz os mesmos
/// tensores (packed_data + shape), escalas, RMS e flags.
pub fn save_model(model: &TransformerModel) -> Option<Vec<u8>> {
    const U16MAX: usize = u16::MAX as usize;
    if model.hidden == 0 || model.num_heads == 0 || model.vocab_size == 0
        || model.num_layers == 0 || model.intermediate_size == 0 || model.num_kv_heads == 0
        || model.hidden > U16MAX || model.num_layers > U16MAX || model.num_heads > U16MAX
        || model.max_seq > U16MAX || model.intermediate_size > U16MAX
        || model.num_kv_heads > U16MAX || model.kv_dim > U16MAX
        || model.layers.len() != model.num_layers
    {
        return None;
    }

    // Dimensões derivadas — espelham o loader (cortex.rs:1756-1759).
    let q_dim = model.kv_dim.max(1);
    let kv_head_dim = q_dim / model.num_heads;
    let k_dim = model.num_kv_heads * kv_head_dim;
    let ffn_group = model.intermediate_size * q_dim / model.hidden;
    let down_out = q_dim;
    let hidden = model.hidden;
    let vocab = model.vocab_size as usize;

    // Sub-norms: bit set só quando o vetor NÃO é o default que o loader
    // sintetiza sem o bit (ones com o comprimento derivado). Assim o arquivo
    // grava exatamente o que o loader lê de volta, sem ambiguidade.
    let l0 = model.layers.first()?;
    let inner_default_len = kv_head_dim * model.num_heads;
    let has_inner_attn_ln = !(l0.rms_inner_attn.len() == inner_default_len
        && l0.rms_inner_attn.iter().all(|&x| x == 1.0));
    let has_ffn_layernorm = !(l0.rms_ffn_norm.len() == model.intermediate_size
        && l0.rms_ffn_norm.iter().all(|&x| x == 1.0));
    let has_rope = true; // loader pré-computa RoPE sempre; theta >1.0 no EOF

    let mut out = Vec::new();
    // header (v4)
    write_u32(&mut out, 0xBE11BE11);
    write_u16(&mut out, 4); // version
    write_u32(&mut out, 0); // num_params (u32 em v4)
    write_u16(&mut out, hidden as u16);
    write_u16(&mut out, model.num_layers as u16);
    write_u16(&mut out, model.num_heads as u16);
    write_u32(&mut out, model.vocab_size);
    write_u16(&mut out, model.max_seq as u16);
    write_u16(&mut out, model.intermediate_size as u16);
    write_u16(&mut out, model.num_kv_heads as u16);
    write_u16(&mut out, q_dim as u16);
    write_u32(&mut out, model.medusa_heads.len() as u32);
    // tie flag (4 bytes) + tokenizer vazio + layer_features
    if model.tie_embeddings {
        out.extend_from_slice(b"TIED");
    } else {
        out.extend_from_slice(&[0, 0, 0, 0]);
    }
    out.push(0); // tok_type
    write_u32(&mut out, 0); // tok_len
    let feat = ((has_inner_attn_ln as u8) << 0) | ((has_ffn_layernorm as u8) << 1) | ((has_rope as u8) << 2);
    out.push(feat);

    // embed (hidden, vocab) + escala — antes dos layers (cortex.rs:1753)
    if model.embed.packed_data.len() != tern_packed_len(hidden, vocab) {
        return None;
    }
    out.extend_from_slice(&model.embed.packed_data);
    write_f32(&mut out, model.embed_scale);

    // layers — ordem e tamanhos idênticos ao loader (cortex.rs:1816-1852)
    for l in &model.layers {
        write_f32_vec_clamped(&mut out, &l.rms_attn, hidden);       // rms_attn (v4: sempre)
        write_f32_vec_clamped(&mut out, &l.rms_ffn, hidden);        // rms_ffn (v4: sempre)
        if has_inner_attn_ln {
            write_f32_vec_clamped(&mut out, &l.rms_inner_attn, hidden);
        }
        if has_ffn_layernorm {
            // loader lê `hidden` e faz pad p/ intermediate_size — gravar hidden
            write_f32_vec_clamped(&mut out, &l.rms_ffn_norm, hidden);
        }
        let tensors = [
            (&l.q, hidden, q_dim, l.q_scale),
            (&l.k, hidden, k_dim, l.k_scale),
            (&l.v, hidden, k_dim, l.v_scale),
            (&l.o, q_dim, hidden, l.o_scale),
            (&l.gate, hidden, ffn_group, l.gate_scale),
            (&l.up, hidden, ffn_group, l.up_scale),
            (&l.down, model.intermediate_size, down_out, l.down_scale),
        ];
        // Ordem interleaved tensor+escala — espelha read_ternary_tensor_with_scale
        for (t, rows, cols, scale) in tensors {
            if t.packed_data.len() != tern_packed_len(rows, cols) {
                return None;
            }
            out.extend_from_slice(&t.packed_data);
            write_f32(&mut out, scale);
        }
    }

    // rms_final (v4 sempre grava; loader cai p/ ones se ausente)
    write_f32_vec_clamped(&mut out, &model.rms_final, hidden);

    // unembed — só quando não-tied; loader grava zeros p/ tied (cortex.rs:1884)
    if !model.tie_embeddings {
        if model.unembed.packed_data.len() != tern_packed_len(hidden, vocab) {
            return None;
        }
        out.extend_from_slice(&model.unembed.packed_data);
        write_f32(&mut out, model.unembed_scale);
    }

    // medusa heads
    for h in &model.medusa_heads {
        if h.w.packed_data.len() != tern_packed_len(hidden, vocab) {
            return None;
        }
        out.extend_from_slice(&h.w.packed_data);
        write_f32(&mut out, h.w_scale);
    }

    // theta RoPE no EOF (loader: só usa se >1.0)
    write_f32(&mut out, if model.rope_theta > 1.0 { model.rope_theta } else { 10000.0 });
    Some(out)
}

fn tern_eq(a: &PackedTernaryTensor, b: &PackedTernaryTensor) -> bool {
    a.shape == b.shape && a.packed_data == b.packed_data
}


// ── Loader .bitnet v6 (ADR-0085) ─────────────────────────────────────
// load_model_v6: parse estrito, dispatch por model_type. Legacy (3..=5): fallback WARN.

/// Carrega modelo .bitnet v6. Retorna None em erro de parse.
pub fn load_model_v6(data: &[u8]) -> Option<TransformerModel> {
    let mut off = 0;
    let magic = read_u32(data, &mut off)?;
    if magic != 0xBE11BE11 { return None; }
    let version = read_u16(data, &mut off)?;
    if version < 6 {
        k_nano::slog_cortex!("LLM", "warn",
            "v{} legacy format — use migrate_bitnet_v6.py", version);
        if version == 5 { return None; } // HWExpert v5 handled separately
        return load_model(data); // v3/v4 fallback
    }
    if version > 6 { return None; }
    // v6 preamble
    let _num_params = read_u64(data, &mut off)?;
    let model_type = read_u8(data, &mut off)?;
    if off + 3 > data.len() { return None; }
    if data[off] != 0 || data[off+1] != 0 || data[off+2] != 0 { return None; }
    off += 3;
    match model_type {
        0 => load_llm_v6(data, &mut off),
        1 => { k_nano::slog_cortex!("LLM", "info", "v6 HWExpert"); None }
        2 => { k_nano::slog_cortex!("LLM", "info", "v6 Router"); None }
        _ => None
    }
}

/// Parse LLM body (model_type=0) from v6 format.
fn load_llm_v6(data: &[u8], off: &mut usize) -> Option<TransformerModel> {
    let hidden = read_u16(data, off)? as usize;
    let num_layers = read_u16(data, off)? as usize;
    let num_heads = read_u16(data, off)? as usize;
    let vocab_size = read_u32(data, off)?;
    let max_seq = read_u16(data, off)? as usize;
    let intermediate_size = read_u16(data, off)? as usize;
    let num_kv_heads = read_u16(data, off)? as usize;
    let q_dim = read_u16(data, off)? as usize;
    let num_medusa = read_u32(data, off)? as usize;
    let tie_embeddings = *off + 4 <= data.len() && &data[*off..*off+4] == b"TIED";
    *off += 4;
    let _tok_type = read_u8(data, off)?;
    let tok_len = read_u32(data, off)? as usize;
    *off = off.saturating_add(tok_len);
    let act_type = read_u8(data, off)?;
    let embed_type = read_u8(data, off)?;
    let feat = read_u8(data, off)?;
    if feat & 0xF8 != 0 { return None; } // bits 3-7 must be 0
    let has_inner = (feat & 0x01) != 0;
    let has_ffn = (feat & 0x02) != 0;
    let has_theta = (feat & 0x04) != 0;
    if act_type > 1 || embed_type > 2 { return None; }

    let kv_head_dim = q_dim / num_heads.max(1);
    let k_dim = num_kv_heads * kv_head_dim;
    let ffn_group = intermediate_size * q_dim / hidden.max(1);
    let down_out = q_dim;

    k_nano::slog_cortex!("LLM", "info",
        "v6 LLM h={} L={} q_dim={} vocab={} act={} emb={} feat=0x{:02x}",
        hidden, num_layers, q_dim, vocab_size, act_type, embed_type, feat);

    // Heap: NÃO estimar/resize aqui (premissa AIOS — self-adapting heap).
    // O bump allocator global cresce sozinho via grow_bump_auto quando a
    // alocação atinge HEAP_LIMIT (k_nano::allocator). Estimativas hardcoded
    // (file*2, etc.) eram o bug: estendiam o TALC (que não é o global
    // allocator) e o extend falhando chamava o handler OOM → hlt no 2B v6.

    // Embed (always has scale — ADR-0085 D1). embed_type: 0=ternary, 1=Q6_K, 2=BF16
    let (embed, embed_scale, embed_q6k) = match embed_type {
        0 => {
            let (t, s) = read_ternary_tensor_with_scale(data, off, hidden, vocab_size as usize)?;
            (t, s, None)
        }
        1 => {
            // Q6_K bruto: 210 bytes por super-bloco de 256 pesos. Decode row-wise
            // no embed_lookup (ADR-0085 D6) — evita materializar 1.31GB f32.
            let n_elems = hidden * vocab_size as usize;
            let n_blocks = (n_elems + 255) / 256;
            let expected = n_blocks * crate::gguf::Q6_K_BLOCK_BYTES;
            if *off + expected > data.len() {
                k_nano::slog_cortex!("LLM", "warn", "v6 Q6_K embed truncated: need {}B have {}B", expected, data.len().saturating_sub(*off));
                return None;
            }
            let raw = data[*off..*off + expected].to_vec();
            *off += expected;
            let scale = read_f32(data, off)?;
            (PackedTernaryTensor { shape: (hidden, vocab_size as usize), packed_data: vec![] }, scale, Some(raw))
        }
        _ => {
            k_nano::slog_cortex!("LLM", "warn", "v6 embed_type={} not yet supported", embed_type);
            return None;
        }
    };

    // Layers
    let mut layers = Vec::with_capacity(num_layers);
    for li in 0..num_layers {
        let rms_attn = read_f32_vec(data, off, hidden)?;
        let rms_ffn = read_f32_vec(data, off, hidden)?;
        let rms_inner_attn = if has_inner { read_f32_vec(data, off, hidden)? }
                             else { vec![1.0; kv_head_dim * num_heads] };
        let rms_ffn_norm = if has_ffn {
            read_f32_vec(data, off, intermediate_size)? // CANÔNICO (ADR-0085 D2)
        } else {
            vec![1.0; intermediate_size]
        };
        let (q, q_scale) = read_ternary_tensor_with_scale(data, off, hidden, q_dim)?;
        let (k, k_scale) = read_ternary_tensor_with_scale(data, off, hidden, k_dim)?;
        let (v, v_scale) = read_ternary_tensor_with_scale(data, off, hidden, k_dim)?;
        let (o, o_scale) = read_ternary_tensor_with_scale(data, off, q_dim, hidden)?;
        let (gate, gate_scale) = read_ternary_tensor_with_scale(data, off, hidden, ffn_group)?;
        let (up, up_scale) = read_ternary_tensor_with_scale(data, off, hidden, ffn_group)?;
        let (down, down_scale) = read_ternary_tensor_with_scale(data, off, intermediate_size, down_out)?;
        layers.push(LayerWeights {
            rms_attn, q, q_scale, k, k_scale, v, v_scale, o, o_scale,
            rms_ffn, rms_inner_attn, rms_ffn_norm,
            gate, gate_scale, up, up_scale, down, down_scale,
            kv_dim: q_dim, num_kv_heads, intermediate_size,
            ffn_group_size: ffn_group,
        });
        if li % 10 == 0 || li + 1 == num_layers {
            k_nano::slog_cortex!("LLM", "info", "v6 layer {}/{} off={}KB", li, num_layers, *off/1024);
        }
    }

    let rms_final = read_f32_vec(data, off, hidden)?;

    // Unembed — tied ⇒ ZERO bytes (ADR-0085 D3)
    let (unembed, unembed_scale) = if !tie_embeddings {
        read_ternary_tensor_with_scale(data, off, hidden, vocab_size as usize)?
    } else {
        (PackedTernaryTensor {
            shape: (hidden, vocab_size as usize),
            packed_data: vec![0u8; (hidden * vocab_size as usize + 3) / 4],
        }, 1.0)
    };

    // Medusa heads
    let mut medusa_heads = Vec::with_capacity(num_medusa);
    for _ in 0..num_medusa {
        let (w, w_scale) = read_ternary_tensor_with_scale(data, off, hidden, vocab_size as usize)?;
        medusa_heads.push(MedusaHead { w, w_scale });
    }

    // Theta (only if feat bit2 — ADR-0085 D3)
    let mut theta = 10000.0f32;
    if has_theta {
        if let Some(t) = read_f32(data, off) {
            if t > 1.0 { theta = t; }
        }
    }
    let rope_seq = (max_seq as usize).min(2048).max(64);
    let (rope_cos, rope_sin) = rope_precompute(rope_seq, kv_head_dim, theta);

    let model = TransformerModel {
        embed, embed_scale, embed_q6k, layers, rms_final, unembed, unembed_scale, medusa_heads,
        vocab_size, hidden, num_layers, max_seq,
        num_heads, num_kv_heads, head_dim: kv_head_dim, kv_dim: q_dim,
        intermediate_size, ffn_group_size: ffn_group,
        tie_embeddings,
        act_type, embed_type,
        rope_theta: theta, rope_cos, rope_sin,
    };
    k_nano::slog_cortex!("LLM", "info", "v6 model OK L={} {}KB", num_layers, data.len()/1024);
    Some(model)
}

// ── Serialização .bitnet v6 (ADR-0085) ─────────────────────────────────
// save_model_v6: formato canônico v6. Diferenças do v4:
// - num_params u64, model_type u8, reserved 3B, act_type/embed_type depois do tokenizer
// - rms_ffn_norm CANÔNICO = intermediate_size (não hidden)
// - tied ⇒ NENHUM byte de unembed (nem zeros)
// - theta só se feat bit2; feat computado do que foi escrito
// - scales SEMPRE presentes em todo tensor quantizado

/// Serializa o modelo no formato .bitnet v6 — byte-exato com bitnet_writer.py.
pub fn save_model_v6(model: &TransformerModel) -> Option<Vec<u8>> {
    const U16MAX: usize = u16::MAX as usize;
    if model.hidden == 0 || model.num_heads == 0 || model.vocab_size == 0
        || model.num_layers == 0 || model.intermediate_size == 0 || model.num_kv_heads == 0
        || model.hidden > U16MAX || model.num_layers > U16MAX || model.num_heads > U16MAX
        || model.max_seq > U16MAX || model.intermediate_size > U16MAX
        || model.num_kv_heads > U16MAX || model.kv_dim > U16MAX
        || model.layers.len() != model.num_layers
    {
        return None;
    }

    let q_dim = model.kv_dim.max(1);
    let kv_head_dim = q_dim / model.num_heads;
    let k_dim = model.num_kv_heads * kv_head_dim;
    let ffn_group = model.intermediate_size * q_dim / model.hidden;
    let down_out = q_dim;
    let hidden = model.hidden;
    let vocab = model.vocab_size as usize;

    // feat computado do que é efetivamente escrito (ADR-0085 D5)
    let l0 = model.layers.first()?;
    let inner_default_len = kv_head_dim * model.num_heads;
    let has_inner_attn_ln = !(l0.rms_inner_attn.len() == inner_default_len
        && l0.rms_inner_attn.iter().all(|&x| x == 1.0));
    let has_ffn_layernorm = !(l0.rms_ffn_norm.len() == model.intermediate_size
        && l0.rms_ffn_norm.iter().all(|&x| x == 1.0));
    let has_theta = model.rope_theta > 1.0;

    // num_params: soma de todos os elementos de todos os tensores (informativo)
    let num_params = {
        let mut n: u64 = (hidden * vocab) as u64; // embed
        n += (model.intermediate_size * model.num_layers) as u64; // rms_ffn_norm × intermediate
        let per_layer = (hidden * q_dim) as u64       // q
            + (hidden * k_dim) as u64 * 2             // k + v
            + (q_dim * hidden) as u64                 // o
            + (hidden * ffn_group) as u64 * 2         // gate + up
            + (model.intermediate_size * down_out) as u64; // down
        n += per_layer * model.num_layers as u64;
        if !model.tie_embeddings {
            n += (hidden * vocab) as u64; // unembed
        }
        for _ in &model.medusa_heads {
            n += (hidden * vocab) as u64;
        }
        n
    };

    let feat = compute_feat(has_inner_attn_ln, has_ffn_layernorm, has_theta);

    let mut out = Vec::new();
    // header v6
    write_u32(&mut out, 0xBE11BE11);           // 0: magic
    write_u16(&mut out, 6);                    // 4: version
    write_u64(&mut out, num_params);           // 6: num_params u64
    out.push(0);                               // 14: model_type (0=LLM)
    out.extend_from_slice(&[0, 0, 0]);         // 15: reserved
    write_u16(&mut out, hidden as u16);        // 18
    write_u16(&mut out, model.num_layers as u16);
    write_u16(&mut out, model.num_heads as u16);
    write_u32(&mut out, model.vocab_size);
    write_u16(&mut out, model.max_seq as u16);
    write_u16(&mut out, model.intermediate_size as u16);
    write_u16(&mut out, model.num_kv_heads as u16);
    write_u16(&mut out, q_dim as u16);
    write_u32(&mut out, model.medusa_heads.len() as u32);
    // tie_flag
    if model.tie_embeddings {
        out.extend_from_slice(b"TIED");
    } else {
        out.extend_from_slice(&[0, 0, 0, 0]);
    }
    out.push(0);                               // tok_type
    write_u32(&mut out, 0);                    // tok_len (vazio)
    out.push(model.act_type.min(1));           // act_type (0=silu, 1=relu2)
    out.push(model.embed_type.min(2));         // embed_type (0=ternary, 1=Q6_K, 2=BF16)
    out.push(feat);

    // embed (hidden, vocab) + f32 scale — embed_type 1 = Q6_K raw bytes
    if model.embed_type == 1 {
        let q6k = match &model.embed_q6k {
            Some(q) => q,
            None => return None,
        };
        let n_elems = hidden * vocab;
        let expected = ((n_elems + 255) / 256) * crate::gguf::Q6_K_BLOCK_BYTES;
        if q6k.len() != expected { return None; }
        out.extend_from_slice(q6k);
        write_f32(&mut out, model.embed_scale);
    } else {
        if model.embed.packed_data.len() != tern_packed_len(hidden, vocab) {
            return None;
        }
        out.extend_from_slice(&model.embed.packed_data);
        write_f32(&mut out, model.embed_scale);
    }

    // layers
    for l in &model.layers {
        write_f32_vec_clamped(&mut out, &l.rms_attn, hidden);
        write_f32_vec_clamped(&mut out, &l.rms_ffn, hidden);
        if has_inner_attn_ln {
            write_f32_vec_clamped(&mut out, &l.rms_inner_attn, hidden);
        }
        if has_ffn_layernorm {
            // CANÔNICO: intermediate_size (ADR-0085 D2)
            write_f32_vec_clamped(&mut out, &l.rms_ffn_norm, model.intermediate_size);
        }
        let tensors = [
            (&l.q, hidden, q_dim, l.q_scale),
            (&l.k, hidden, k_dim, l.k_scale),
            (&l.v, hidden, k_dim, l.v_scale),
            (&l.o, q_dim, hidden, l.o_scale),
            (&l.gate, hidden, ffn_group, l.gate_scale),
            (&l.up, hidden, ffn_group, l.up_scale),
            (&l.down, model.intermediate_size, down_out, l.down_scale),
        ];
        for (t, rows, cols, scale) in tensors {
            if t.packed_data.len() != tern_packed_len(rows, cols) {
                return None;
            }
            out.extend_from_slice(&t.packed_data);
            write_f32(&mut out, scale);
        }
    }

    // rms_final
    write_f32_vec_clamped(&mut out, &model.rms_final, hidden);

    // unembed — tied ⇒ ZERO bytes (ADR-0085 D3)
    if !model.tie_embeddings {
        if model.unembed.packed_data.len() != tern_packed_len(hidden, vocab) {
            return None;
        }
        out.extend_from_slice(&model.unembed.packed_data);
        write_f32(&mut out, model.unembed_scale);
    }

    // medusa heads
    for h in &model.medusa_heads {
        if h.w.packed_data.len() != tern_packed_len(hidden, vocab) {
            return None;
        }
        out.extend_from_slice(&h.w.packed_data);
        write_f32(&mut out, h.w_scale);
    }

    // theta (só se feat bit2)
    if has_theta {
        write_f32(&mut out, model.rope_theta);
    }
    Some(out)
}

fn compute_feat(has_inner: bool, has_ffn: bool, has_theta: bool) -> u8 {
    ((has_inner as u8) << 0) | ((has_ffn as u8) << 1) | ((has_theta as u8) << 2)
}

/// Self-test determinístico de round-trip save→load. Constrói um modelo
/// sintético mínimo (2 layers, shapes pequenos, GQA/BitFFN reais), salva,
/// recarrega e compara tensores/escalas/RMS. Retorna true se idêntico.
pub fn model_save_roundtrip_self_test() -> bool {
    let hidden = 16usize;
    let num_layers = 2usize;
    let num_heads = 2usize;
    let vocab_size = 32u32;
    let max_seq = 64usize;
    let intermediate_size = 32usize;
    let num_kv_heads = 1usize;
    let q_dim = 16usize; // kv_dim (divisível por num_heads)
    let kv_head_dim = q_dim / num_heads; // 8
    let k_dim = num_kv_heads * kv_head_dim; // 8
    let ffn_group = intermediate_size * q_dim / hidden; // 32
    let down_out = q_dim;

    fn tern(rows: usize, cols: usize, seed: u32) -> PackedTernaryTensor {
        let mut vals = Vec::with_capacity(rows * cols);
        let mut x = seed;
        for _ in 0..rows * cols {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            vals.push(match x % 3 { 0 => 1i8, 1 => -1i8, _ => 0i8 });
        }
        PackedTernaryTensor { shape: (rows, cols), packed_data: PackedTernaryTensor::pack_weights(&vals) }
    }
    fn rms(seed: u32, n: usize) -> Vec<f32> {
        let mut x = seed;
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            v.push(0.5 + (x % 100) as f32 / 100.0);
        }
        v
    }

    // bit0 (inner_attn_ln) set com valores reais; bit1 (ffn_layernorm) set com
    // vetor real + padding ones além de `hidden` (como o loader re-pad faz).
    let rms_inner = rms(101, hidden);
    let mut rms_ffn_norm = rms(202, hidden);
    rms_ffn_norm.resize(intermediate_size, 1.0);

    let mut layers = Vec::with_capacity(num_layers);
    for li in 0..num_layers {
        layers.push(LayerWeights {
            rms_attn: rms(300 + li as u32, hidden),
            q: tern(hidden, q_dim, 10 + li as u32),
            q_scale: 0.5 + li as f32 * 0.25,
            k: tern(hidden, k_dim, 20 + li as u32),
            k_scale: 0.75,
            v: tern(hidden, k_dim, 30 + li as u32),
            v_scale: 1.25,
            o: tern(q_dim, hidden, 40 + li as u32),
            o_scale: 0.9,
            rms_ffn: rms(400 + li as u32, hidden),
            rms_inner_attn: rms_inner.clone(),
            rms_ffn_norm: rms_ffn_norm.clone(),
            gate: tern(hidden, ffn_group, 50 + li as u32),
            gate_scale: 1.1,
            up: tern(hidden, ffn_group, 60 + li as u32),
            up_scale: 0.8,
            down: tern(intermediate_size, down_out, 70 + li as u32),
            down_scale: 1.05,
            kv_dim: q_dim,
            num_kv_heads,
            intermediate_size,
            ffn_group_size: ffn_group,
        });
    }

    let model = TransformerModel {
        embed: tern(hidden, vocab_size as usize, 1),
        embed_scale: 1.5,
        layers,
        rms_final: rms(99, hidden),
        unembed: tern(hidden, vocab_size as usize, 2),
        unembed_scale: 0.6,
        medusa_heads: alloc::vec![MedusaHead { w: tern(hidden, vocab_size as usize, 3), w_scale: 1.3 }],
        vocab_size,
        hidden,
        num_layers,
        max_seq,
        num_heads,
        num_kv_heads,
        head_dim: kv_head_dim,
        kv_dim: q_dim,
        intermediate_size,
        ffn_group_size: ffn_group,
        tie_embeddings: false,
        act_type: 0,
        embed_type: 0,
        embed_q6k: None,
        rope_theta: 10000.0,
        rope_cos: alloc::vec![0.0; max_seq * kv_head_dim / 2],
        rope_sin: alloc::vec![0.0; max_seq * kv_head_dim / 2],
    };

    let bytes = match save_model(&model) {
        Some(b) => b,
        None => {
            k_nano::slog_cortex!("LLM", "warn", "model save/load roundtrip self-test FAIL (save returned None)");
            return false;
        }
    };
    let loaded = match load_model(&bytes) {
        Some(m) => m,
        None => {
            k_nano::slog_cortex!("LLM", "warn", "model save/load roundtrip self-test FAIL (load returned None, {} bytes)", bytes.len());
            return false;
        }
    };

    let dims_ok = loaded.vocab_size == model.vocab_size
        && loaded.hidden == model.hidden
        && loaded.num_layers == model.num_layers
        && loaded.max_seq == model.max_seq
        && loaded.num_heads == model.num_heads
        && loaded.num_kv_heads == model.num_kv_heads
        && loaded.head_dim == model.head_dim
        && loaded.kv_dim == model.kv_dim
        && loaded.intermediate_size == model.intermediate_size
        && loaded.ffn_group_size == model.ffn_group_size
        && loaded.tie_embeddings == model.tie_embeddings;

    let mut tensors_ok = dims_ok
        && tern_eq(&loaded.embed, &model.embed)
        && loaded.embed_scale == model.embed_scale
        && tern_eq(&loaded.unembed, &model.unembed)
        && loaded.unembed_scale == model.unembed_scale
        && loaded.rms_final == model.rms_final
        && loaded.medusa_heads.len() == model.medusa_heads.len();

    if tensors_ok {
        for (lh, mh) in loaded.medusa_heads.iter().zip(model.medusa_heads.iter()) {
            if !tern_eq(&lh.w, &mh.w) || lh.w_scale != mh.w_scale {
                tensors_ok = false;
                break;
            }
        }
    }
    if tensors_ok && loaded.layers.len() == model.layers.len() {
        for (ll, ml) in loaded.layers.iter().zip(model.layers.iter()) {
            if ll.rms_attn != ml.rms_attn
                || ll.rms_ffn != ml.rms_ffn
                || ll.rms_inner_attn != ml.rms_inner_attn
                || ll.rms_ffn_norm != ml.rms_ffn_norm
                || ll.kv_dim != ml.kv_dim
                || ll.num_kv_heads != ml.num_kv_heads
                || ll.intermediate_size != ml.intermediate_size
                || ll.ffn_group_size != ml.ffn_group_size
                || !tern_eq(&ll.q, &ml.q) || ll.q_scale != ml.q_scale
                || !tern_eq(&ll.k, &ml.k) || ll.k_scale != ml.k_scale
                || !tern_eq(&ll.v, &ml.v) || ll.v_scale != ml.v_scale
                || !tern_eq(&ll.o, &ml.o) || ll.o_scale != ml.o_scale
                || !tern_eq(&ll.gate, &ml.gate) || ll.gate_scale != ml.gate_scale
                || !tern_eq(&ll.up, &ml.up) || ll.up_scale != ml.up_scale
                || !tern_eq(&ll.down, &ml.down) || ll.down_scale != ml.down_scale
            {
                tensors_ok = false;
                break;
            }
        }
    } else {
        tensors_ok = false;
    }

    if tensors_ok {
        k_nano::slog_cortex!("LLM", "info", "model save/load roundtrip self-test PASS ({} bytes, L={})", bytes.len(), num_layers);
    } else {
        k_nano::slog_cortex!("LLM", "warn", "model save/load roundtrip self-test FAIL ({} bytes, L={})", bytes.len(), num_layers);
    }
    tensors_ok
}

/// v6 writer parity: compara save_model_v6 byte-a-byte com golden_v6.bin
/// gerado por tools/bitnet_writer.py --self-check (mesmo LCG, mesma spec).
#[cfg(test)]
#[test]
fn v6_writer_parity() {
    // Mesma spec do Python self_check + golden_v6.bin
    let hidden = 16usize;
    let num_layers = 2usize;
    let num_heads = 2usize;
    let vocab_size = 32u32;
    let max_seq = 64usize;
    let intermediate_size = 32usize;
    let num_kv_heads = 1usize;
    let q_dim = 16usize;
    let num_medusa = 1usize;

    let kv_head_dim = q_dim / num_heads; // 8
    let k_dim = num_kv_heads * kv_head_dim; // 8
    let ffn_group = intermediate_size * q_dim / hidden; // 32
    let down_out = q_dim; // 16

    // LCG idêntico ao Python: x = (x * 1103515245 + 12345) & 0x7FFFFFFF, seed=42
    // Python usa precisão arbitrária → Rust precisa de u64 p/ evitar truncamento precoce
    let lcg_state: core::cell::RefCell<u64> = core::cell::RefCell::new(42);
    let lcg = || -> u32 {
        let mut s = lcg_state.borrow_mut();
        *s = (s.wrapping_mul(1103515245).wrapping_add(12345)) & 0x7FFFFFFF;
        *s as u32
    };

    // Ordem de consumo de LCG idêntica ao Python self_check
    let tern_i8 = |rows: usize, cols: usize| -> PackedTernaryTensor {
        let mut vals = Vec::with_capacity(rows * cols);
        for _ in 0..rows * cols {
            let r = lcg() % 3;
            vals.push(match r { 0 => 1i8, 1 => -1i8, _ => 0i8 });
        }
        PackedTernaryTensor {
            shape: (rows, cols),
            packed_data: PackedTernaryTensor::pack_weights(&vals),
        }
    };
    let rms_vec = |n: usize| -> Vec<f32> {
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            // Python: 0.5 + (lcg()%100) / 100.0  em f64, depois cast p/ f32
            // Rust: f64 intermediate then cast to match Python rounding
            v.push((0.5f64 + (lcg() % 100) as f64 / 100.0) as f32);
        }
        v
    };

    // Embed
    let embed = tern_i8(hidden, vocab_size as usize);
    let embed_scale = 1.5f32;

    // Layers
    let mut layers = Vec::with_capacity(num_layers);
    for li in 0..num_layers {
        let rms_attn = rms_vec(hidden);
        let rms_ffn = rms_vec(hidden);
        let rms_inner_attn = rms_vec(hidden);
        let rms_ffn_norm = rms_vec(intermediate_size);
        let q = tern_i8(hidden, q_dim);
        let k = tern_i8(hidden, k_dim);
        let v = tern_i8(hidden, k_dim);
        let o = tern_i8(q_dim, hidden);
        let gate = tern_i8(hidden, ffn_group);
        let up = tern_i8(hidden, ffn_group);
        let down = tern_i8(intermediate_size, down_out);
        layers.push(LayerWeights {
            rms_attn,
            q,
            q_scale: 0.5 + li as f32 * 0.25,
            k,
            k_scale: 0.75,
            v,
            v_scale: 1.25,
            o,
            o_scale: 0.9,
            rms_ffn,
            rms_inner_attn,
            rms_ffn_norm,
            gate,
            gate_scale: 1.1,
            up,
            up_scale: 0.8,
            down,
            down_scale: 1.05,
            kv_dim: q_dim,
            num_kv_heads,
            intermediate_size,
            ffn_group_size: ffn_group,
        });
    }

    let rms_final = rms_vec(hidden);
    let unembed = tern_i8(hidden, vocab_size as usize);
    let unembed_scale = 0.6f32;

    let medusa_head = MedusaHead {
        w: tern_i8(hidden, vocab_size as usize),
        w_scale: 1.3f32,
    };

    let model = TransformerModel {
        embed,
        embed_scale,
        layers,
        rms_final,
        unembed,
        unembed_scale,
        medusa_heads: alloc::vec![medusa_head],
        vocab_size,
        hidden,
        num_layers,
        max_seq,
        num_heads,
        num_kv_heads,
        head_dim: kv_head_dim,
        kv_dim: q_dim,
        intermediate_size,
        ffn_group_size: ffn_group,
        tie_embeddings: false,
        act_type: 0,
        embed_type: 0,
        embed_q6k: None,
        rope_theta: 10000.0,
        rope_cos: alloc::vec![0.0; max_seq * kv_head_dim / 2],
        rope_sin: alloc::vec![0.0; max_seq * kv_head_dim / 2],
    };

    let bytes = save_model_v6(&model).expect("save_model_v6 returned None");
    let golden = include_bytes!("../../../tools/golden_v6.bin");

    if bytes.as_slice() != golden.as_slice() {
        let diff = bytes.iter().zip(golden.iter())
            .position(|(a, b)| a != b);
        let off = diff.unwrap_or(0);
        let ctx = if off >= 8 { off - 8 } else { off };
        panic!(
            "v6 writer parity FAIL: {} bytes Rust vs {} bytes golden. First diff at offset {:?}.\n\
             Rust[{}..{}]={:02x?}\n\
             Gold[{}..{}]={:02x?}",
            bytes.len(), golden.len(), diff,
            ctx, (ctx+32).min(bytes.len()),
            &bytes[ctx..(ctx+32).min(bytes.len())],
            ctx, (ctx+32).min(golden.len()),
            &golden[ctx..(ctx+32).min(golden.len())],
        );
    }
    // Pass: no panic
}

/// Round-trip v6 em host: save_model_v6 → load_model_v6 → comparação.
/// Valida o pipeline completo (writer canônico ↔ loader estrito) sem
/// depender do modelo 2B — desrisca o boot QEMU v6 (ADR-0085 F2/F4).
#[cfg(test)]
#[test]
fn v6_roundtrip_load() {
    let hidden = 16usize;
    let num_layers = 2usize;
    let num_heads = 2usize;
    let vocab_size = 32u32;
    let max_seq = 64usize;
    let intermediate_size = 32usize;
    let num_kv_heads = 1usize;
    let q_dim = 16usize;
    let kv_head_dim = q_dim / num_heads;
    let k_dim = num_kv_heads * kv_head_dim;
    let ffn_group = intermediate_size * q_dim / hidden;
    let down_out = q_dim;

    // LCG idêntico ao Python (u64 p/ paridade byte-exact)
    let lcg_state: core::cell::RefCell<u64> = core::cell::RefCell::new(42);
    let lcg = || -> u32 {
        let mut s = lcg_state.borrow_mut();
        *s = (s.wrapping_mul(1103515245).wrapping_add(12345)) & 0x7FFFFFFF;
        *s as u32
    };
    let tern = |rows: usize, cols: usize| -> PackedTernaryTensor {
        let mut vals = Vec::with_capacity(rows * cols);
        for _ in 0..rows * cols {
            let r = lcg() % 3;
            vals.push(match r { 0 => 1i8, 1 => -1i8, _ => 0i8 });
        }
        PackedTernaryTensor { shape: (rows, cols), packed_data: PackedTernaryTensor::pack_weights(&vals) }
    };
    let rms = |n: usize| -> Vec<f32> {
        (0..n).map(|_| 0.5 + (lcg() % 100) as f32 / 100.0).collect()
    };

    let embed = tern(hidden, vocab_size as usize);
    let embed_scale = 1.5f32;
    let mut layers = Vec::with_capacity(num_layers);
    for li in 0..num_layers {
        layers.push(LayerWeights {
            rms_attn: rms(hidden), q: tern(hidden, q_dim), q_scale: 0.5 + li as f32 * 0.25,
            k: tern(hidden, k_dim), k_scale: 0.75, v: tern(hidden, k_dim), v_scale: 1.25,
            o: tern(q_dim, hidden), o_scale: 0.9, rms_ffn: rms(hidden),
            rms_inner_attn: rms(hidden), rms_ffn_norm: rms(intermediate_size),
            gate: tern(hidden, ffn_group), gate_scale: 1.1,
            up: tern(hidden, ffn_group), up_scale: 0.8,
            down: tern(intermediate_size, down_out), down_scale: 1.05,
            kv_dim: q_dim, num_kv_heads, intermediate_size, ffn_group_size: ffn_group,
        });
    }
    let model = TransformerModel {
        embed, embed_scale, layers,
        rms_final: rms(hidden),
        unembed: tern(hidden, vocab_size as usize), unembed_scale: 0.6,
        medusa_heads: alloc::vec![MedusaHead { w: tern(hidden, vocab_size as usize), w_scale: 1.3 }],
        vocab_size, hidden, num_layers, max_seq,
        num_heads, num_kv_heads, head_dim: kv_head_dim, kv_dim: q_dim,
        intermediate_size, ffn_group_size: ffn_group,
        tie_embeddings: false,
        act_type: 0, embed_type: 0, embed_q6k: None,
        rope_theta: 10000.0,
        rope_cos: alloc::vec![0.0; max_seq * kv_head_dim / 2],
        rope_sin: alloc::vec![0.0; max_seq * kv_head_dim / 2],
    };

    let bytes = save_model_v6(&model).expect("save_model_v6 None");
    let view = crate::model::load_model_v6(&bytes).expect("load_model_v6 None");
    let m = view.as_llm().expect("ModelView não é LLM");

    assert_eq!(m.hidden, hidden);
    assert_eq!(m.num_layers, num_layers);
    assert_eq!(m.vocab_size, vocab_size);
    assert_eq!(m.num_kv_heads, num_kv_heads);
    assert_eq!(m.intermediate_size, intermediate_size);
    assert_eq!(m.tie_embeddings, false);
    assert_eq!(m.act_type, 0, "act_type v6 round-trip");
    assert_eq!(m.embed_type, 0, "embed_type v6 round-trip");
    assert_eq!(m.rope_theta, 10000.0);
    // rms_ffn_norm must be exact intermediate_size (D2 — sem pad)
    assert_eq!(m.layers[0].rms_ffn_norm.len(), intermediate_size);
    assert!(tern_eq(&m.embed, &model.embed));
    assert!(tern_eq(&m.unembed, &model.unembed));
    assert!(m.embed_scale == 1.5 && m.unembed_scale == 0.6);
}

/// HW Expert v6 (ADR-0085 §3.2): o arquivo convertido tools/target/hw_expert_v6.bitnet
/// (mt=1) deve carregar e produzir predições IDÊNTICAS ao v5 legado
/// (models/hw_expert/hw_expert_v4.bitnet) nos devices canônicos — prova de que
/// a conversão é fiel e o loader v6 lê o mesmo modelo (F1b).
#[cfg(test)]
#[test]
fn hwexpert_v6_matches_v5_predictions() {
    let v5_bytes = include_bytes!("../../../legacy/hw_expert_v4.bitnet");
    let v6_bytes = include_bytes!("../../../target1/hw_expert_v6.bitnet");

    let m5 = load_hwexpert_v5(v5_bytes).expect("v5 load falhou");
    let m6 = load_hwexpert_v6(v6_bytes).expect("v6 load falhou");

    assert_eq!(m6.hidden, 128);
    assert_eq!(m6.num_layers, 6);
    assert_eq!(m6.q_dim, 32, "q_dim preservado (forward trunca atenção)");
    assert_eq!(m6.ff_dim, 256);

    let devices = [
        (0x8086u16, 0x100Eu16),
        (0x1234u16, 0x1111u16),
        (0x8086u16, 0x1237u16),
        (0x8086u16, 0x7000u16),
        (0x8086u16, 0x7010u16),
        (0x8086u16, 0x7113u16),
        (0x1AF4u16, 0x1000u16),
        (0x8086u16, 0x2723u16),
        (0x168Cu16, 0x003Eu16),
        (0x10ECu16, 0x8139u16),
    ];
    for (vid, did) in devices {
        let p5 = predict_hw_v4(&m5, vid, did);
        let p6 = predict_hw_v4(&m6, vid, did);
        assert_eq!(
            p5.family_id, p6.family_id,
            "family_id {vid:04x}:{did:04x} v5={} v6={}",
            p5.family_id, p6.family_id
        );
        assert_eq!(
            p5.fw_id, p6.fw_id,
            "fw_id {vid:04x}:{did:04x} v5={} v6={}",
            p5.fw_id, p6.fw_id
        );
        assert_eq!(
            p5.agent_id, p6.agent_id,
            "agent_id {vid:04x}:{did:04x} v5={} v6={}",
            p5.agent_id, p6.agent_id
        );
        assert_eq!(
            p5.caps_bits, p6.caps_bits,
            "caps {vid:04x}:{did:04x} v5={:#x} v6={:#x}",
            p5.caps_bits, p6.caps_bits
        );
        assert_eq!(
            p5.next_action, p6.next_action,
            "next {vid:04x}:{did:04x} v5={} v6={}",
            p5.next_action, p6.next_action
        );
    }
}

pub fn argmax_row(logits: &Tensor, row: usize) -> u32 {
    let cols = logits.shape.1;
    let start = row * cols;
    let mut best = 0u32;
    let mut best_val = NEG_INFINITY;
    for j in 0..cols {
        let v = logits.data[start + j];
        if v > best_val { best_val = v; best = j as u32; }
    }
    best
}

// ── F0: structured logits dump for parity ──
pub fn dump_logits_top(logits: &Tensor, n: usize) {
    let cols = logits.shape.1;
    let mut top: Vec<(u32, f32)> = (0..cols.min(128000) as u32)
        .map(|i| (i, logits.data[i as usize]))
        .collect();
    top.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(core::cmp::Ordering::Equal));
    top.truncate(n);
    let ids: Vec<u32> = top.iter().map(|(id, _)| *id).collect();
    let bits: Vec<i32> = top.iter().map(|(_, v)| (v * 64.0) as i32).collect();
    k_nano::slog_cortex!("FWD", "info", "logits_top_n={} ids={:?} logits_bits={:?}", n, ids, bits);
}

// ── F1–F3: Coherence buffer (temperature + top-k + repetition penalty + Gumbel-max) ──

pub static COHERENCE_ENABLED: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);
pub static COHERENCE_TEMP: core::sync::atomic::AtomicU32 =
    core::sync::atomic::AtomicU32::new(f32::to_bits(0.7));
pub static COHERENCE_TOP_K: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(16);
pub static COHERENCE_REPEAT: core::sync::atomic::AtomicU32 =
    core::sync::atomic::AtomicU32::new(f32::to_bits(1.2));

pub fn set_coherence(enabled: bool, temp: f32, top_k: usize, repeat: f32) {
    COHERENCE_ENABLED.store(enabled, core::sync::atomic::Ordering::Relaxed);
    COHERENCE_TEMP.store(f32::to_bits(temp), core::sync::atomic::Ordering::Relaxed);
    COHERENCE_TOP_K.store(top_k, core::sync::atomic::Ordering::Relaxed);
    COHERENCE_REPEAT.store(f32::to_bits(repeat), core::sync::atomic::Ordering::Relaxed);
    k_nano::slog_cortex!("GEN", "info",
        "coherence set enabled={} temp={} top_k={} repeat={}",
        enabled as u8, temp, top_k, repeat);
}

struct SampleRng(u32);
impl SampleRng {
    fn seed() -> Self {
        let s = k_nano::hw_rng::HardwareRandom::next_u64_retry(4).unwrap_or(0xDEAD_BEEF) as u32;
        Self(s)
    }
    fn next_u32(&mut self) -> u32 {
        self.0 ^= self.0 << 13; self.0 ^= self.0 >> 17; self.0 ^= self.0 << 5; self.0
    }
    fn uniform(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 * 1.0 / (1u64 << 24) as f32
    }
    fn gumbel(&mut self) -> f32 {
        let u = self.uniform().max(core::f32::EPSILON).min(1.0 - core::f32::EPSILON);
        -libm::logf(-libm::logf(u))
    }
}

/// Sample token with configurable temperature, top-k, repetition penalty (Gumbel-max).
pub fn sample_token_coherence(logits: &Tensor, row: usize, recent: &[u16]) -> u32 {
    let cols = logits.shape.1;
    let start = row * cols;
    let hi = cols.min(128000);
    let temp = f32::from_bits(COHERENCE_TEMP.load(core::sync::atomic::Ordering::Relaxed));
    let top_k = COHERENCE_TOP_K.load(core::sync::atomic::Ordering::Relaxed);
    let repeat = f32::from_bits(COHERENCE_REPEAT.load(core::sync::atomic::Ordering::Relaxed));

    let mut cand: [(u32, f32); 64] = [(0, NEG_INFINITY); 64];
    let mut n = 0usize;
    for j in 0..hi {
        let id = j as u32;
        if recent.iter().any(|&p| p as usize == j) { continue; }
        if crate::bpe::is_special_id(id) { continue; }
        let v = logits.data[start + j];
        if v.is_nan() { continue; }
        if n < 64 { cand[n] = (id, v); n += 1; }
        else {
            let mut worst = 0usize;
            for i in 1..64 { if cand[i].1 < cand[worst].1 { worst = i; } }
            if v > cand[worst].1 { cand[worst] = (id, v); }
        }
    }
    if n == 0 { return crate::bpe::eos_id(); }

    if (repeat - 1.0).abs() > 0.001 {
        for i in 0..n {
            if recent.iter().any(|&p| u32::from(p) == cand[i].0) {
                let v = cand[i].1;
                cand[i].1 = if v >= 0.0 { v / repeat } else { v * repeat };
            }
        }
    }
    let t = if temp < 0.001 { 0.7 } else { temp };
    for i in 0..n { cand[i].1 /= t; }
    cand[..n].sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(core::cmp::Ordering::Equal));
    let k = n.min(if top_k > 0 { top_k } else { n });

    let mut rng = SampleRng::seed();
    let mut best = cand[0].0;
    let mut best_val = NEG_INFINITY;
    for i in 0..k {
        let noisy = cand[i].1 + rng.gumbel();
        if noisy > best_val { best_val = noisy; best = cand[i].0; }
    }
    k_nano::slog_cortex!("GEN", "info", "coherence temp={} top_k={} best={} n_cand={}", t, k, best, n);
    best
}

/// Argmax sobre HF vocab: top-64 brutos → re-score com BPE.
pub fn argmax_row_hf_vocab(logits: &Tensor, row: usize, recent: &[u16]) -> u32 {
    let cols = logits.shape.1;
    let start = row * cols;
    let hi = cols.min(128000);
    let mut top: [(u32, f32); 64] = [(0, NEG_INFINITY); 64];
    let mut filled = 0usize;
    for j in 0..hi {
        let id = j as u32;
        if recent.iter().any(|&p| p as u32 == id) { continue; }
        if crate::bpe::is_special_id(id) { continue; }
        let v = logits.data[start + j];
        if v.is_nan() { continue; }
        if filled < 64 { top[filled] = (id, v); filled += 1; }
        else {
            let mut worst = 0usize;
            for i in 1..64 { if top[i].1 < top[worst].1 { worst = i; } }
            if v > top[worst].1 { top[worst] = (id, v); }
        }
    }
    let weather = crate::bpe::weather_candidate_ids();
    let mut wx: [(u32, f32); 24] = [(0, NEG_INFINITY); 24];
    let mut wx_n = 0usize;
    for &id in weather.iter() {
        if (id as usize) >= hi { continue; }
        if recent.iter().any(|&p| p as u32 == id) { continue; }
        let v = logits.data[start + id as usize];
        if v.is_nan() { continue; }
        if wx_n < 24 { wx[wx_n] = (id, v); wx_n += 1; }
    }
    if filled == 0 && wx_n == 0 { return crate::bpe::eos_id(); }

    let mut best = if filled > 0 { top[0].0 } else { wx[0].0 };
    let mut best_val = NEG_INFINITY;
    for i in 0..filled { let s = top[i].1 + crate::bpe::score_piece(top[i].0); if s > best_val { best_val = s; best = top[i].0; } }
    for i in 0..wx_n { let s = wx[i].1 + crate::bpe::score_piece(wx[i].0); if s > best_val { best_val = s; best = wx[i].0; } }
    best
}

/// Constrained weather token selection.
pub fn argmax_row_weather_only(logits: &Tensor, row: usize, recent: &[u16]) -> u32 {
    let cols = logits.shape.1;
    let start = row * cols;
    let hi = cols.min(128000);
    let step = recent.len().saturating_sub(1);
    let prev = recent.last().copied().map(|p| p as u32);
    let masked = crate::bpe::weather_step_candidates(step, prev);
    let weather = if masked.is_empty() { crate::bpe::weather_candidate_ids() } else { masked };
    let mut best = weather[0];
    let mut best_val = NEG_INFINITY;
    let mut any = false;
    for &id in weather.iter() {
        if (id as usize) >= hi { continue; }
        if recent.iter().any(|&p| u32::from(p) == id) { continue; }
        if crate::bpe::weather_same_stem(prev, id) { continue; }
        let v = logits.data[start + id as usize];
        if v.is_nan() { continue; }
        let mut s = v + crate::bpe::score_piece(id);
        s += crate::bpe::weather_position_bias(id, step);
        s += crate::bpe::weather_bigram_bias(prev, id);
        if crate::bpe::weather_is_en_loan(id) { s -= 2.0; }
        if !any || s > best_val { best_val = s; best = id; any = true; }
    }
    if any { best } else {
        let fb = crate::bpe::weather_candidate_ids();
        let mut best2 = fb[0]; let mut best2_val = NEG_INFINITY;
        for &id in fb.iter() {
            if (id as usize) >= hi { continue; }
            let v = logits.data[start + id as usize];
            if v.is_nan() { continue; }
            let s = v + crate::bpe::score_piece(id);
            if s > best2_val { best2_val = s; best2 = id; }
        }
        best2
    }
}

/// Argmax char-level vocab (fallback when no BPE).
pub fn argmax_row_char_vocab(logits: &Tensor, row: usize, prev: Option<u16>) -> u32 {
    let cols = logits.shape.1;
    let start = row * cols;
    let hi = (VOCAB_SIZE as usize).min(cols);
    let lo = CHAR_OFFSET as usize;
    let mut best = EOS as u32;
    let mut best_val = NEG_INFINITY;
    for j in lo..hi {
        let id = j as u32;
        if let Some(p) = prev { if id as u16 == p && id < 140 { continue; } }
        let v = logits.data[start + j];
        if v.is_nan() { continue; }
        if v > best_val { best_val = v; best = id; }
    }
    if best_val == NEG_INFINITY { EOS as u32 } else { best }
}

/// Slim prompt for heavy models (soft-float 2B): keep only last few tokens.
pub fn slim_prompt_tokens_for_heavy(tokens: &[u32], use_bpe: bool) -> Vec<u32> {
    let mut t: Vec<u32> = tokens.to_vec();
    if use_bpe {
        const MAX_CHAT: usize = 8;
        if t.len() > MAX_CHAT { t.truncate(MAX_CHAT); }
        return t;
    }
    if t.last() == Some(&(EOS as u32)) { t.pop(); }
    const KEEP: usize = 1;
    if t.is_empty() { return vec![BOS as u32]; }
    if t[0] != BOS as u32 { t.insert(0, BOS as u32); }
    if t.len() > KEEP + 1 {
        let mut slim = vec![BOS as u32];
        let from = t.len() - KEEP;
        slim.extend_from_slice(&t[from..]);
        slim
    } else { t }
}

pub fn generate_speculative(model: &TransformerModel, prompt: &str, mut decoder: Option<&mut StructuredDecoder>) -> alloc::string::String {
    let max_seq = model.max_seq.min(64);
    let use_bpe = crate::bpe::is_loaded();
    let eos: u32 = if use_bpe { crate::bpe::eos_id() as u32 } else { EOS as u32 };
    let eot: u32 = if use_bpe { crate::bpe::eot_id() as u32 } else { EOS as u32 };
    let eos_u16 = eos as u16;
    let _eot_u16 = eot as u16;
    let mut tokens: Vec<u32> = if use_bpe {
        crate::bpe::encode(prompt)
    } else {
        Tokenizer::encode(prompt).into_iter().map(|t| t as u32).collect()
    };
    // Guarda: IDs fora do vocab → OOB no embed.
    let vs = model.vocab_size;
    tokens.retain(|&t| t < vs);
    if tokens.is_empty() {
        tokens.push(if use_bpe { crate::bpe::bos_id().min(vs.saturating_sub(1)) } else { BOS as u32 });
    }
    let raw_len = tokens.len();
    // Heavy model: slim prompt
    if model.hidden >= 2048 && tokens.len() > 1 {
        tokens = slim_prompt_tokens_for_heavy(&tokens, use_bpe);
    }
    let prompt_len = tokens.len();
    k_nano::slog_cortex!("GEN", "info",
        "prompt_len={} (raw={}) max_seq={} h={} L={} bpe={} first={} last={}",
        prompt_len, raw_len, max_seq,
        model.hidden, model.num_layers,
        use_bpe as u8,
        tokens.first().copied().unwrap_or(0xFFFF),
        tokens.last().copied().unwrap_or(0xFFFF));

    let kv_dim = model.kv_dim;
    let k_dim = if model.layers.is_empty() { kv_dim } else {
        model.layers[0].k.shape.1
    };
    let mut cache = KvCache::new(model.layers.len(), k_dim, kv_dim);

    let t0 = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
    let (mut last_hidden, mut last_logits) = model.forward_with_kv(&tokens, &mut cache);
    let t1 = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
    k_nano::slog_cortex!("GEN", "info", "prompt fwd: {} ticks", t1 - t0);

    let is_greeting = crate::bpe::prompt_is_greeting(prompt);
    let max_gen = if model.hidden >= 2048 {
        if use_bpe { if is_greeting { 8 } else { 6 } } else { 4 }
    } else {
        max_seq.saturating_sub(prompt_len).min(16)
    };
    k_nano::slog_cortex!("GEN", "info", "max_gen={} greet={}", max_gen, is_greeting as u8);

    // recent is Vec<u16> for u16-based argmax/sample functions
    let mut recent_u16: Vec<u16> = Vec::new();
    if !is_greeting { if let Some(&last) = tokens.last() { recent_u16.push(last as u16); } }

    // Ngram speculator for speculative decoding (works with u16 tokens internally)
    let mut tokens_u16: Vec<u16> = tokens.iter().map(|&t| t as u16).collect();
    let mut spec = NgramSpeculator::new();
    let tokens_u16_slice: Vec<u16> = tokens.iter().map(|&t| t as u16).collect();
    spec.feed_slice(&tokens_u16_slice);

    let mut step = 0usize;
    while step < max_gen {
        if tokens.len() >= max_seq { break; }

        // F0: dump top-16 logits on first step for parity
        if step == 0 && use_bpe { dump_logits_top(&last_logits, 16); }

        // F4: apply structured decoder mask (zero invalid tokens)
        if let Some(ref d) = decoder {
            d.mask_logits(&mut last_logits.data);
        }

        // ── Select next token (returns u16) ──
        let next_u16 = if COHERENCE_ENABLED.load(core::sync::atomic::Ordering::Relaxed) && use_bpe {
            sample_token_coherence(&last_logits, 0, &recent_u16)
        } else if use_bpe {
            // Greeting no longer uses a hardcoded token pool — full model argmax.
            // (Audit 7.5: GREETING_BIAS_IDS was effectively canned output.)
            if model.vocab_size > 0 && model.vocab_size <= 33_000 {
                argmax_row_hf_vocab(&last_logits, 0, &recent_u16)
            } else if false { // RUN_WEATHER_E2E_SKINNY placeholder
                argmax_row_weather_only(&last_logits, 0, &recent_u16)
            } else {
                argmax_row_hf_vocab(&last_logits, 0, &recent_u16)
            }
        } else {
            argmax_row_char_vocab(&last_logits, 0, recent_u16.last().copied())
        };
        let next = next_u16 as u32;

        // F4: advance structured decoder FSM
        if let Some(ref mut d) = decoder {
            d.step(next_u16 as u16);
        }

        if next == eos || next == eot {
            k_nano::slog_cortex!("GEN", "info", "eos/special at step={} id={}", step + 1, next);
            break;
        }

        k_nano::slog_cortex!("GEN", "info", "step={} next={} cols={}", step + 1, next, last_logits.shape.1);

        // ── Speculative decoding (ngram draft + verify) ──
        tokens.push(next);
        recent_u16.push(next_u16 as u16);
        if recent_u16.len() > 4 { recent_u16.remove(0); }
        tokens_u16.push(next_u16 as u16);
        step += 1;
        spec.feed(next_u16 as u16);
        record_classic_step();

        // Early-exit: greetingish / weatherish
        if use_bpe {
            let partial = crate::bpe::decode(&tokens[prompt_len..]);
            if is_greeting && crate::bpe::text_is_greetingish(&partial) {
                k_nano::slog_cortex!("GEN", "info", "early_exit greetingish step={}", step);
                break;
            }
        }

        // Try speculative draft (skip when structured decoder active — ngram drafts don't respect FSM constraints)
        let draft = spec.propose();
        if draft.len() >= 2 && !COHERENCE_ENABLED.load(core::sync::atomic::Ordering::Relaxed) && decoder.is_none() {
            // ngram speculation (disabled when coherence sampling active — distributions differ)
            let m = draft.len().min(max_gen - step).min(crate::ngram_spec::M);
            if m > 0 {
                let drafts_u16 = &draft[..m];
                // Convert u16 drafts to u32 for forward_with_kv_all_logits
                let drafts_u32: Vec<u32> = drafts_u16.iter().map(|&t| t as u32).collect();
                let all_logits = model.forward_with_kv_all_logits(&drafts_u32, &mut cache);
                let (extra_accept, bonus_u16) = verify_draft(&all_logits, drafts_u16);
                let kept = (1 + extra_accept).min(m);
                record_spec_hit(kept as u64);

                for &t in drafts_u16.iter().take(kept) {
                    tokens.push(t as u32);
                    recent_u16.push(t);
                    if recent_u16.len() > 4 { recent_u16.remove(0); }
                    tokens_u16.push(t);
                    step += 1;
                    spec.feed(t);
                }

                // Bonus token after accepted prefix
                if bonus_u16 as u16 != eos_u16 && step < max_gen && tokens.len() < max_seq {
                    tokens.push(bonus_u16);
                    recent_u16.push(bonus_u16 as u16);
                    if recent_u16.len() > 4 { recent_u16.remove(0); }
                    tokens_u16.push(bonus_u16 as u16);
                    step += 1;
                    spec.feed(bonus_u16 as u16);
                    record_spec_bonus_forward();
                    record_spec_tokens(1);
                }
                if tokens.last() == Some(&eos) || tokens.last() == Some(&eot) { break; }
                continue;
            }
        }

        // Normal KV forward for the next step
        if step < max_gen && tokens.len() < max_seq {
            let t_step = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            let (new_hidden, new_logits) = model.forward_with_kv(&[next], &mut cache);
            let t_step1 = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            k_nano::slog_cortex!("GEN", "info", "step={} token={} kv_cache: {} ticks (ctx={})",
                step, next, t_step1 - t_step, tokens.len());
            last_hidden = new_hidden;
            last_logits = new_logits;
        }
    }

    // ADR-0047: publish last hidden as latent thought (non-fatal).
    crate::projection::publish_thought(&last_hidden.data);

    let gen = &tokens[prompt_len..];
    // F0: structured result log
    let bpe_label = if use_bpe {
        if model.vocab_size > 0 && model.vocab_size <= 33_000 { "SP32" } else { "LLAMA" }
    } else { "CHAR" };
    let stop_label = if gen.last().copied().map_or(false, |t| t == eos || t == eot) { "EOS" } else { "MAX_GEN" };
    let coh = COHERENCE_ENABLED.load(core::sync::atomic::Ordering::Relaxed);
    k_nano::slog_cortex!("GEN", "info",
        "result first={} last={} stop={} bpe={} coherence={} ids={:?}",
        gen.first().copied().unwrap_or(0xFFFF),
        gen.last().copied().unwrap_or(0xFFFF),
        stop_label, bpe_label, coh as u8, gen);

    let out = if use_bpe { crate::bpe::decode(gen) } else {
        let u16s: Vec<u16> = gen.iter().map(|&t| t as u16).collect();
        Tokenizer::decode(&u16s)
    };
    if out.is_empty() {
        k_nano::slog_cortex!("GEN", "info", "decoded_empty n={} first_gen={}",
            gen.len(), gen.first().copied().unwrap_or(0xFFFF));
    } else {
        let preview: alloc::string::String = out.chars().take(64).collect();
        k_nano::slog_cortex!("GEN", "info", "decoded_len={} text='{}'", out.len(), preview);
    }
    out
}

pub fn generate_text(model: &TransformerModel, prompt: &str) -> alloc::string::String {
    // Consume any decoder threaded via DECODER_CELL
    let mut decoder_opt = DECODER_CELL.take();
    let raw = match decoder_opt.as_mut() {
        Some(ptr) => {
            let decoder = unsafe { &mut **ptr };
            generate_speculative(model, prompt, Some(decoder))
        }
        None => generate_speculative(model, prompt, None),
    };
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

pub static CURRENT_MODEL: spin::Mutex<Option<Box<dyn Model>>> = spin::Mutex::new(None);
pub static RUSTCODER_MODEL: spin::Mutex<Option<Box<dyn Model>>> = spin::Mutex::new(None);
/// Gate do W2A8 (ADR-0084 §3 F4): true só quando soft_stride=1, MAX_SEQ
/// adequado e geração ≥ algo útil. Hoje false — gaps pendentes.
pub static GENERATION_GAPS_RESOLVED: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);pub static HWEXPERT_MODEL: spin::Mutex<Option<Box<dyn Model>>> = spin::Mutex::new(None);
/// Dimensão do CURRENT_MODEL (p/ skip LLM-TEST em 2B).
pub static CURRENT_MODEL_EMBED_DIM: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);

/// Diagnóstico honesto do modelo carregado
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ModelStatus {
    NoneLoaded = 0,
    ToyFallback = 1,
    BitNetReal = 2,
}

impl ModelStatus {
    pub fn name(self) -> &'static str {
        match self {
            Self::NoneLoaded => "none",
            Self::ToyFallback => "toy-fallback",
            Self::BitNetReal => "bitnet-real",
        }
    }
    pub fn is_ai_ready(self) -> bool {
        matches!(self, Self::BitNetReal)
    }
}

pub static MODEL_STATUS: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

pub fn model_status() -> ModelStatus {
    match MODEL_STATUS.load(core::sync::atomic::Ordering::Acquire) {
        2 => ModelStatus::BitNetReal,
        1 => ModelStatus::ToyFallback,
        _ => ModelStatus::NoneLoaded,
    }
}

/// Structured info about the currently loaded model
pub struct ModelInfo {
    pub status: ModelStatus,
    pub embed_dim: usize,
    pub vocab_size: u32,
    pub num_layers: usize,
    pub max_seq: usize,
    pub hidden: usize,
}

impl core::fmt::Display for ModelInfo {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "model={} dim={} vocab={} layers={} max_seq={} hidden={}",
            self.status.name(), self.embed_dim, self.vocab_size, self.num_layers, self.max_seq, self.hidden)
    }
}

pub fn model_info() -> Option<ModelInfo> {
    let guard = CURRENT_MODEL.lock();
    let st = model_status();
    guard.as_ref().map(|m| ModelInfo {
        status: st,
        embed_dim: m.embed_dim(),
        vocab_size: m.vocab_size(),
        num_layers: 0, // ponytail: trait doesn't expose num_layers yet
        max_seq: m.max_seq(),
        hidden: 0,
    })
}

pub const NO_MODEL_MSG: &str = "[CORTEX] AI indisponível — nenhum modelo carregado";

pub fn set_model(model: Box<dyn Model>) {
    CURRENT_MODEL_EMBED_DIM.store(model.embed_dim(), core::sync::atomic::Ordering::Relaxed);
    *CURRENT_MODEL.lock() = Some(model);
    MODEL_STATUS.store(ModelStatus::BitNetReal as u8, core::sync::atomic::Ordering::Release);
    crate::model_hub::mark_active(true);
    let dim = CURRENT_MODEL_EMBED_DIM.load(core::sync::atomic::Ordering::Relaxed);
    k_nano::slog_cortex!("CORTEX", "info", "model=bitnet-real dim={} status=AI_READY", dim);
}

static INFER_IN_FLIGHT: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// True enquanto `generate_via_model*` está no hot path (HUD Jarbas).
pub fn infer_in_flight() -> bool {
    INFER_IN_FLIGHT.load(core::sync::atomic::Ordering::Relaxed)
}

fn infer_guard() -> InferGuard {
    INFER_IN_FLIGHT.store(true, core::sync::atomic::Ordering::Relaxed);
    InferGuard
}
struct InferGuard;
impl Drop for InferGuard {
    fn drop(&mut self) {
        INFER_IN_FLIGHT.store(false, core::sync::atomic::Ordering::Relaxed);
    }
}

/// Generate text using the currently loaded model (simplified, no Trinity routing).
/// Called by hermes (crate dependency) — bin's version adds Trinity routing.
pub fn generate_via_model(prompt: &str) -> String {
    let _busy = infer_guard();
    let guard = CURRENT_MODEL.lock();
    match guard.as_ref() {
        Some(m) => m.generate(prompt),
        None => String::from(NO_MODEL_MSG),
    }
}

/// Generate text with structured decoding using the currently loaded model.
pub fn generate_via_model_with_decoder(prompt: &str, dec: &mut StructuredDecoder) -> String {
    let _busy = infer_guard();
    DECODER_CELL.set(dec as *mut StructuredDecoder);
    let guard = CURRENT_MODEL.lock();
    match guard.as_ref() {
        Some(m) => m.generate(prompt),
        None => String::from("[CORTEX] No model loaded"),
    }
}

/// Generate text with structured decoding by grammar constraint.
/// Selects the appropriate FSM and masks logits at each step to enforce
/// the output format (JSON, shell-safe commands, skill commands, or free text).
///
/// # Examples (conceptual)
/// ```no_run
/// let json = cortex::cortex::generate_structured(
///     "list 3 colors",
///     cortex::structured_decode::OutputGrammar::Json,
/// );
/// assert!(json.starts_with('{') && json.ends_with('}'));
/// ```
pub fn generate_structured(prompt: &str, grammar: OutputGrammar) -> String {
    let mut dec = StructuredDecoder::new(grammar.into());
    generate_via_model_with_decoder(prompt, &mut dec)
}

/// Registra .bitnet em slot nomeado sem necessariamente virar Active.
pub fn register_model_slot(slot: crate::model_hub::ModelSlot, model: Box<dyn Model>) {
    match slot {
        crate::model_hub::ModelSlot::Active => set_model(model),
        crate::model_hub::ModelSlot::RustCoder => set_rustcoder_model(model),
        crate::model_hub::ModelSlot::HwExpert => set_hwexpert_model(model),
        other => crate::model_hub::register_model(other, model),
    }
}

/// Aceita múltiplos blobs: cada um vai para slot heurístico por tamanho (ou `hint`).
pub fn load_models_multi(blobs: &[(&[u8], Option<&str>)]) -> usize {
    let mut n = 0usize;
    for (data, hint) in blobs {
        let Some(m) = load_model(data) else {
            continue;
        };
        let slot = hint
            .and_then(crate::model_hub::ModelSlot::from_name)
            .unwrap_or_else(|| crate::model_hub::slot_from_bitnet_bytes(data.len()));
        // Primeiro modelo “grande” ou Active vazio → CURRENT; demais → slots.
        let boxed = alloc::boxed::Box::new(m);
        if !model_is_loaded()
            && matches!(
                slot,
                crate::model_hub::ModelSlot::Active
                    | crate::model_hub::ModelSlot::GeneratorPro
                    | crate::model_hub::ModelSlot::Vision
            )
        {
            let also_pro = slot == crate::model_hub::ModelSlot::GeneratorPro
                || crate::model_hub::slot_from_bitnet_bytes(data.len())
                    == crate::model_hub::ModelSlot::GeneratorPro;
            set_model(boxed);
            if also_pro {
                crate::model_hub::mark_pro_alias(true);
            }
        } else {
            register_model_slot(slot, boxed);
        }
        n += 1;
    }
    n
}

/// True se CURRENT_MODEL está setado (LLM LOADED).
pub fn model_is_loaded() -> bool {
    CURRENT_MODEL.lock().is_some()
}

/// True se HW Expert MoE está setado.
pub fn hwexpert_is_loaded() -> bool {
    HWEXPERT_MODEL.lock().is_some()
}

/// True se RustCoder expert está setado.
pub fn rustcoder_is_loaded() -> bool {
    RUSTCODER_MODEL.lock().is_some()
}

pub fn set_rustcoder_model(model: Box<dyn Model>) {
    *RUSTCODER_MODEL.lock() = Some(model);
    crate::model_hub::mark_slot(crate::model_hub::ModelSlot::RustCoder, true);
    k_nano::slog_cortex!("CORTEX", "info", "RustCoder expert model loaded (hub).");
}

pub fn set_hwexpert_model(model: Box<dyn Model>) {
    *HWEXPERT_MODEL.lock() = Some(model);
    crate::model_hub::mark_slot(crate::model_hub::ModelSlot::HwExpert, true);
    k_nano::slog_cortex!("CORTEX", "info", "HW Expert model loaded (SDIO MoE).");
}

/// Sintetiza um HardwareRegisterMap para um dispositivo PCI.
/// Estrategia hierarquica com 3 niveis:
///   1. Tenta mapa direto por HWID (tabela conhecida)
///   2. Usa IA para identificar familia do chip e aplicar mapa correspondente
///   3. Heuristica por vendor (fallback)
pub fn generate_register_map(vid: u16, did: u16) -> Option<crate::HardwareRegisterMap> {
    use crate::HardwareRegisterMap as Hm;

    // Nivel 1: mapa direto por HWID (da tabela conhecida)
    let direct = match (vid, did) {
        // Intel WiFi
        (0x8086, 0x08B1)|(0x8086,0x08B2)|(0x8086,0x24F3)|(0x8086,0x24F4)
        |(0x8086,0x24F5)|(0x8086,0x24F6)|(0x8086,0x24FD)|(0x8086,0x2526)
        |(0x8086,0x2527)|(0x8086,0x2723)|(0x8086,0x2725)|(0x8086,0x2726)
        |(0x8086,0x3165)|(0x8086,0x3166)|(0x8086,0x06F0)|(0x8086,0x02F0)
            => Some(Hm { tx_ring_low:0x1000, rx_ring_low:0x1004, rx_control:0x0008,
                        doorbell_tx:0x2000, doorbell_rx:0x2004, cmd_start_rx:0x0001,
                        ring_size:64, rx_buf_len:2048 }),
        // Realtek WiFi
        (0x0BDA,_)|(0x10EC,0x8176)|(0x10EC,0x8179)|(0x10EC,0x8812)
            => Some(Hm { tx_ring_low:0x00A0, rx_ring_low:0x00A4, rx_control:0x002C,
                        doorbell_tx:0x00D0, doorbell_rx:0x00D4, cmd_start_rx:0x8002,
                        ring_size:16, rx_buf_len:2048 }),
        // Atheros/Qualcomm WiFi
        (0x168C,_) => Some(Hm { tx_ring_low:0x0800, rx_ring_low:0x0804, rx_control:0x0010,
                                doorbell_tx:0x0C00, doorbell_rx:0x0C04, cmd_start_rx:0x0001,
                                ring_size:32, rx_buf_len:2048 }),
        // Broadcom WiFi
        (0x14E4,_) => Some(Hm { tx_ring_low:0x0500, rx_ring_low:0x0504, rx_control:0x0020,
                                doorbell_tx:0x0600, doorbell_rx:0x0604, cmd_start_rx:0x0100,
                                ring_size:32, rx_buf_len:2048 }),
        _ => None,
    };
    if let Some(m) = direct { return Some(m); }

    // Nivel 2 free-text HW Expert REMOVIDO (lixo OA5US…). v4 → k_ai::hw_capability.

    // Nivel 3: heuristica por vendor ID
    let vendor_map = match vid {
        0x8086 => Some(Hm { tx_ring_low:0x1000, rx_ring_low:0x1004, rx_control:0x0008,
                            doorbell_tx:0x2000, doorbell_rx:0x2004, cmd_start_rx:0x0001,
                            ring_size:32, rx_buf_len:2048 }),
        0x10EC|0x0BDA => Some(Hm { tx_ring_low:0x00A0, rx_ring_low:0x00A4, rx_control:0x002C,
                                    doorbell_tx:0x00D0, doorbell_rx:0x00D4, cmd_start_rx:0x8002,
                                    ring_size:16, rx_buf_len:2048 }),
        0x168C => Some(Hm { tx_ring_low:0x0800, rx_ring_low:0x0804, rx_control:0x0010,
                            doorbell_tx:0x0C00, doorbell_rx:0x0C04, cmd_start_rx:0x0001,
                            ring_size:32, rx_buf_len:2048 }),
        0x14E4 => Some(Hm { tx_ring_low:0x0500, rx_ring_low:0x0504, rx_control:0x0020,
                            doorbell_tx:0x0600, doorbell_rx:0x0604, cmd_start_rx:0x0100,
                            ring_size:32, rx_buf_len:2048 }),
        _ => None,
    };
    if let Some(m) = vendor_map {
        k_nano::slog_cortex!("AI", "MAP", "Heuristica vendor {:#06x}: mapa generico aplicado", vid);
        return Some(m);
    }
    None
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
    let tokens: Vec<u32> = Tokenizer::encode(prompt).into_iter().map(|t| t as u32).collect();
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

            let next = argmax_from_slice(&noisy_logits, 0) as u32;

            if next == EOS as u32 || next >= VOCAB_SIZE as u32 { break; }
            t.push(next);
            traj_text.push(Tokenizer::decode_char(next as u16).unwrap_or('?'));

            // Atualiza best score
            if q > best_score && traj_text.len() > 3 {
                best_score = q;
                best_text = traj_text.clone();
            }
        }
    }

    if best_text.is_empty() {
        let u16s: Vec<u16> = tokens.iter().map(|&t| t as u16).collect();
        Tokenizer::decode(&u16s)
    } else {
        best_text
    }
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
        // Controles HW antes de chat/status (evita LLM para "ajuste o volume").
        if lower.contains("volume")
            || lower.contains("mute")
            || lower.contains("brilho")
            || lower.contains("brightness")
        {
            Intent::AudioVolume
        } else if lower.contains("hello")
            || lower.contains("hey")
            || lower.contains("ola")
            || lower.contains("olá")
            || lower.contains("oi")
            || lower.contains("bom dia")
            || lower.contains("boa tarde")
            || lower.contains("boa noite")
        {
            Intent::Greeting
        } else if lower.contains("status") || lower.contains("system info") {
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
        } else {
            Intent::Chat
        }
    }
}

#[derive(Debug)]
pub enum Intent {
    SystemStatus, Echo, HardwareInfo, HardwareIdentify, TrustAllow, TrustDeny,
    Network, HttpFetch, Help, Conversation, Usage, Greeting, Chat, AudioVolume,
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
            Intent::AudioVolume => "audio_set_volume",
        }
    }
}
