//! ViT-B/16 vision encoder — SigLIP. 384×384 RGBA → 768-dim embedding.
//!
//! Modelo .bitnet v5 com RTN+scale: cada PackedTernaryTensor é seguido
//! de um f32 scale no arquivo binário.
//!
//! # Arquitetura
//! - Patch embedding: Conv2d(3→768, kernel=16, stride=16) → 576 patches
//! - Position embeddings: 576×768 (adicionados aos patches)
//! - CLS token: learnable 768-dim (posição 0)
//! - 12× Encoder layers (MHA 12 heads + FFN 768→3072→768, GELU)
//! - Post LayerNorm no token CLS

use alloc::vec;
use alloc::vec::Vec;

use cortex::tensor::{PackedTernaryTensor, Tensor};
use cortex::bitnet_sse;

// ─── Constants ──────────────────────────────────────────────────────────

/// ViT-B/16: 384×384 input, 16×16 patches.
const IMG_SIZE: usize = 384;
const PATCH_SIZE: usize = 16;
const NUM_PATCHES: usize = (IMG_SIZE / PATCH_SIZE) * (IMG_SIZE / PATCH_SIZE); // 576
const HIDDEN: usize = 768;
const NUM_LAYERS: usize = 12;
const NUM_HEADS: usize = 12;
const HEAD_DIM: usize = HIDDEN / NUM_HEADS; // 64
/// FFN intermediate: HIDDEN * 4 = 3072
const MLP_DIM: usize = HIDDEN * 4;
const SEQ_LEN: usize = NUM_PATCHES + 1; // 577 (576 patches + CLS)
const EPS: f32 = 1e-6;

// ─── Encoder Layer ─────────────────────────────────────────────────────

pub struct EncoderLayer {
    /// LayerNorm (attention) weight (768)
    pub ln1: Vec<f32>,
    /// LayerNorm (attention) bias (768)
    pub ln1_bias: Vec<f32>,
    /// Q projection weight (768, 768) packed ternary
    pub q: PackedTernaryTensor,
    pub q_scale: f32,
    /// K projection weight (768, 768)
    pub k: PackedTernaryTensor,
    pub k_scale: f32,
    /// V projection weight (768, 768)
    pub v: PackedTernaryTensor,
    pub v_scale: f32,
    /// Output projection weight (768, 768)
    pub o: PackedTernaryTensor,
    pub o_scale: f32,
    /// LayerNorm (FFN) weight (768)
    pub ln2: Vec<f32>,
    /// LayerNorm (FFN) bias (768)
    pub ln2_bias: Vec<f32>,
    /// FFN gate/up projection (768, 3072) packed ternary
    pub fc1: PackedTernaryTensor,
    pub fc1_scale: f32,
    /// FFN down projection (3072, 768) packed ternary
    pub fc2: PackedTernaryTensor,
    pub fc2_scale: f32,
}

// ─── VisionEncoder ─────────────────────────────────────────────────────

pub struct VisionEncoder {
    /// Patch embedding weight (768, 768) — flattened 3×16×16 kernel por canal
    pub patch_embed: PackedTernaryTensor,
    /// Patch embedding bias (768)
    pub patch_bias: Vec<f32>,
    /// Position embeddings (576, 768) packed ternary
    pub pos_embed: PackedTernaryTensor,
    pub pos_scale: f32,
    /// Learnable CLS token (768)
    pub cls_token: [f32; HIDDEN],
    /// 12 encoder layers
    pub layers: Vec<EncoderLayer>,
    /// Post LayerNorm weight (768)
    pub post_ln: Vec<f32>,
    /// Post LayerNorm bias (768)
    pub post_ln_bias: Vec<f32>,
    /// Scale for patch_embed ternary matmul
    pub patch_scale: f32,
}

impl VisionEncoder {
    /// Carrega modelo .bitnet v5 do slice `data`.
    ///
    /// Formato esperado:
    /// - magic: u32 = 0xBE11BE11
    /// - version: u16 = 5
    /// - Para cada PackedTernaryTensor: u16 rows, u16 cols, [u8; ceil(rows*cols/4)], f32 scale
    /// - Para cada Vec<f32>: u16 len, [f32; len]
    ///
    /// Ordem: patch_embed, patch_bias, pos_embed, cls_token,
    ///        12× (ln1, ln1_bias, q, k, v, o, ln2, ln2_bias, fc1, fc2),
    ///        post_ln, post_ln_bias
    pub fn load(data: &[u8]) -> Option<Self> {
        let mut off = 0usize;

        // ── Header ──
        let magic = read_u32(data, &mut off)?;
        if magic != 0xBE11BE11 {
            return None;
        }
        let _version = read_u16(data, &mut off)?; // 5

        // ── patch_embed: PackedTernaryTensor (768, 768) + scale ──
        let pe_rows = read_u16(data, &mut off)? as usize;
        let pe_cols = read_u16(data, &mut off)? as usize;
        let patch_embed = read_packed_ternary(data, &mut off, pe_rows, pe_cols)?;
        let patch_scale = read_f32(data, &mut off)?;

        // ── patch_bias: Vec<f32> (768) ──
        let bias_len = read_u16(data, &mut off)? as usize;
        let patch_bias = read_f32_vec(data, &mut off, bias_len)?;

        // ── pos_embed: PackedTernaryTensor (576, 768) + scale ──
        let pos_rows = read_u16(data, &mut off)? as usize;
        let pos_cols = read_u16(data, &mut off)? as usize;
        let pos_embed = read_packed_ternary(data, &mut off, pos_rows, pos_cols)?;
        let pos_scale = read_f32(data, &mut off)?;

        // ── cls_token: [f32; 768] ──
        let cls_len = read_u16(data, &mut off)? as usize;
        let cls_vec = read_f32_vec(data, &mut off, cls_len)?;
        let cls_token: [f32; HIDDEN] = cls_vec.try_into().ok()?;

        // ── 12 encoder layers ──
        let mut layers = Vec::with_capacity(NUM_LAYERS);
        for _ in 0..NUM_LAYERS {
            layers.push(load_layer(data, &mut off)?);
        }

        // ── post_ln: weight + bias ──
        let ln_len = read_u16(data, &mut off)? as usize;
        let post_ln = read_f32_vec(data, &mut off, ln_len)?;
        let post_ln_bias = read_f32_vec(data, &mut off, ln_len)?;

        Some(VisionEncoder {
            patch_embed,
            patch_bias,
            pos_embed,
            pos_scale,
            cls_token,
            layers,
            post_ln,
            post_ln_bias,
            patch_scale,
        })
    }

    /// Forward pass: RGBA8888 → 768-dim embedding.
    ///
    /// 1. Resize para 384×384 (bilinear, se necessário)
    /// 2. Extrair 16×16 patches → (576, 768)
    /// 3. Patch embedding: ternary_matmul + scale + bias
    /// 4. Adicionar position embeddings (unpack → add)
    /// 5. Prepend CLS token → (577, 768)
    /// 6. 12× Encoder layers
    /// 7. Post LayerNorm no CLS token
    /// 8. Retornar embedding [f32; 768]
    pub fn encode(&self, rgba: &[u8], width: u32, height: u32) -> [f32; HIDDEN] {
        // 1. RGB normalizado, resize se necessário
        let rgb = if (width as usize) == IMG_SIZE && (height as usize) == IMG_SIZE {
            rgba_to_rgb_normalized(rgba, width as usize, height as usize)
        } else {
            resize_bilinear_rgba_to_rgb(rgba, width as usize, height as usize)
        };

        // 2. Extrair patches: (NUM_PATCHES, HIDDEN) = (576, 768)
        let patch_data = extract_patches(&rgb, IMG_SIZE, IMG_SIZE, PATCH_SIZE);
        let mut hidden_states = Tensor {
            shape: (NUM_PATCHES, HIDDEN),
            data: patch_data,
        };

        // 3. Patch embedding: matmul + scale + bias
        if let Some(mut embed) = bitnet_sse::ternary_matmul(&self.patch_embed, &hidden_states) {
            // Apply scale
            for val in embed.data.iter_mut() {
                *val *= self.patch_scale;
            }
            // Add bias: broadcast over all patches
            let bias = &self.patch_bias;
            for i in 0..NUM_PATCHES {
                for j in 0..HIDDEN {
                    embed.data[i * HIDDEN + j] += bias[j];
                }
            }
            hidden_states = embed;
        }

        // 4. Add position embeddings
        let pos_data = unpack_ternary_to_f32(&self.pos_embed, self.pos_scale);
        if pos_data.len() == NUM_PATCHES * HIDDEN {
            for i in 0..NUM_PATCHES {
                for j in 0..HIDDEN {
                    hidden_states.data[i * HIDDEN + j] += pos_data[i * HIDDEN + j];
                }
            }
        }

        // 5. Prepend CLS token → (SEQ_LEN, HIDDEN) = (577, 768)
        let mut seq_data = Vec::with_capacity(SEQ_LEN * HIDDEN);
        seq_data.extend_from_slice(&self.cls_token);
        seq_data.extend_from_slice(&hidden_states.data);
        let mut seq = Tensor {
            shape: (SEQ_LEN, HIDDEN),
            data: seq_data,
        };

        // 6. Encoder layers
        for layer in self.layers.iter() {
            seq = self.forward_layer(layer, seq);
        }

        // 7. Post LayerNorm no CLS token (position 0)
        let cls_only = &seq.data[..HIDDEN];
        let cls_norm = layer_norm(cls_only, &self.post_ln, &self.post_ln_bias, EPS);

        // 8. Retornar embedding [f32; 768]
        let mut embedding = [0.0f32; HIDDEN];
        embedding.copy_from_slice(&cls_norm);
        embedding
    }

    /// Forward pass through one encoder layer.
    fn forward_layer(&self, layer: &EncoderLayer, input: Tensor) -> Tensor {
        let seq_len = input.shape.0;
        let hidden = input.shape.1;

        // ── Attention sub-layer ──
        // LayerNorm on each token
        let mut normed = Tensor::new(input.shape);
        for i in 0..seq_len {
            let start = i * hidden;
            let token = &input.data[start..start + hidden];
            let ln = layer_norm(token, &layer.ln1, &layer.ln1_bias, EPS);
            normed.data[start..start + hidden].copy_from_slice(&ln);
        }

        // QKV projections via ternary matmul + scale
        let q = self.ternary_linear(&normed, &layer.q, layer.q_scale);
        let k = self.ternary_linear(&normed, &layer.k, layer.k_scale);
        let v = self.ternary_linear(&normed, &layer.v, layer.v_scale);

        // Multihead self-attention
        let attn_out = multihead_attention(&q, &k, &v, NUM_HEADS, HEAD_DIM);

        // O projection
        let o = self.ternary_linear(&attn_out, &layer.o, layer.o_scale);

        // Residual
        let mut x = Tensor::new(input.shape);
        for i in 0..input.data.len() {
            x.data[i] = input.data[i] + o.data[i];
        }

        // ── FFN sub-layer ──
        let mut normed2 = Tensor::new(x.shape);
        for i in 0..seq_len {
            let start = i * hidden;
            let token = &x.data[start..start + hidden];
            let ln = layer_norm(token, &layer.ln2, &layer.ln2_bias, EPS);
            normed2.data[start..start + hidden].copy_from_slice(&ln);
        }

        // fc1 → GELU → fc2
        let fc1_out = self.ternary_linear(&normed2, &layer.fc1, layer.fc1_scale);
        let mut gelu_out = Tensor::new(fc1_out.shape);
        for (i, &v) in fc1_out.data.iter().enumerate() {
            gelu_out.data[i] = gelu_approx(v);
        }
        let fc2_out = self.ternary_linear(&gelu_out, &layer.fc2, layer.fc2_scale);

        // Residual
        for i in 0..x.data.len() {
            x.data[i] += fc2_out.data[i];
        }

        x
    }

    /// Ternary linear layer: y = ternary_matmul(x, weight) * scale
    fn ternary_linear(&self, x: &Tensor, weight: &PackedTernaryTensor, scale: f32) -> Tensor {
        if let Some(mut out) = bitnet_sse::ternary_matmul(weight, x) {
            for val in out.data.iter_mut() {
                *val *= scale;
            }
            out
        } else {
            // Fallback: shapes mismatch — should not happen with valid model
            Tensor::new((x.shape.0, weight.shape.1))
        }
    }
}

// ─── Binary reader helpers ─────────────────────────────────────────────

fn read_u16(data: &[u8], off: &mut usize) -> Option<u16> {
    if *off + 2 > data.len() {
        return None;
    }
    let v = u16::from_le_bytes(data[*off..*off + 2].try_into().ok()?);
    *off += 2;
    Some(v)
}

fn read_u32(data: &[u8], off: &mut usize) -> Option<u32> {
    if *off + 4 > data.len() {
        return None;
    }
    let v = u32::from_le_bytes(data[*off..*off + 4].try_into().ok()?);
    *off += 4;
    Some(v)
}

fn read_f32(data: &[u8], off: &mut usize) -> Option<f32> {
    if *off + 4 > data.len() {
        return None;
    }
    let v = f32::from_le_bytes(data[*off..*off + 4].try_into().ok()?);
    *off += 4;
    Some(v)
}

fn read_f32_vec(data: &[u8], off: &mut usize, n: usize) -> Option<Vec<f32>> {
    let mut v = Vec::with_capacity(n);
    for _ in 0..n {
        v.push(read_f32(data, off)?);
    }
    Some(v)
}

fn read_packed_ternary(
    data: &[u8],
    off: &mut usize,
    rows: usize,
    cols: usize,
) -> Option<PackedTernaryTensor> {
    let count = (rows * cols + 3) / 4;
    if *off + count > data.len() {
        return None;
    }
    let packed = data[*off..*off + count].to_vec();
    *off += count;
    Some(PackedTernaryTensor {
        shape: (rows, cols),
        packed_data: packed,
    })
}

fn load_layer(data: &[u8], off: &mut usize) -> Option<EncoderLayer> {
    // ln1: u16 len + [f32; 768]
    let _ = read_u16(data, off)?;
    let ln1 = read_f32_vec(data, off, HIDDEN)?;
    let ln1_bias = read_f32_vec(data, off, HIDDEN)?;

    // q: (768, 768) packed ternary + scale
    let qr = read_u16(data, off)? as usize;
    let qc = read_u16(data, off)? as usize;
    let q = read_packed_ternary(data, off, qr, qc)?;
    let q_scale = read_f32(data, off)?;

    // k: (768, 768)
    let kr = read_u16(data, off)? as usize;
    let kc = read_u16(data, off)? as usize;
    let k = read_packed_ternary(data, off, kr, kc)?;
    let k_scale = read_f32(data, off)?;

    // v: (768, 768)
    let vr = read_u16(data, off)? as usize;
    let vc = read_u16(data, off)? as usize;
    let v = read_packed_ternary(data, off, vr, vc)?;
    let v_scale = read_f32(data, off)?;

    // o: (768, 768)
    let or = read_u16(data, off)? as usize;
    let oc = read_u16(data, off)? as usize;
    let o = read_packed_ternary(data, off, or, oc)?;
    let o_scale = read_f32(data, off)?;

    // ln2: u16 len + [f32; 768] + [f32; 768]
    let _ = read_u16(data, off)?;
    let ln2 = read_f32_vec(data, off, HIDDEN)?;
    let ln2_bias = read_f32_vec(data, off, HIDDEN)?;

    // fc1: (768, 3072) packed ternary + scale
    let f1r = read_u16(data, off)? as usize;
    let f1c = read_u16(data, off)? as usize;
    let fc1 = read_packed_ternary(data, off, f1r, f1c)?;
    let fc1_scale = read_f32(data, off)?;

    // fc2: (3072, 768) packed ternary + scale
    let f2r = read_u16(data, off)? as usize;
    let f2c = read_u16(data, off)? as usize;
    let fc2 = read_packed_ternary(data, off, f2r, f2c)?;
    let fc2_scale = read_f32(data, off)?;

    Some(EncoderLayer {
        ln1,
        ln1_bias,
        q,
        q_scale,
        k,
        k_scale,
        v,
        v_scale,
        o,
        o_scale,
        ln2,
        ln2_bias,
        fc1,
        fc1_scale,
        fc2,
        fc2_scale,
    })
}

// ─── Image processing ──────────────────────────────────────────────────

/// RGBA8888 → RGB f32 [0, 1], assume IMG_SIZE×IMG_SIZE.
fn rgba_to_rgb_normalized(rgba: &[u8], w: usize, h: usize) -> Vec<f32> {
    let len = w * h * 3;
    let mut rgb = vec![0.0f32; len];
    for y in 0..h {
        for x in 0..w {
            let src = (y * w + x) * 4;
            let dst = (y * w + x) * 3;
            rgb[dst] = rgba[src] as f32 / 255.0;
            rgb[dst + 1] = rgba[src + 1] as f32 / 255.0;
            rgb[dst + 2] = rgba[src + 2] as f32 / 255.0;
        }
    }
    rgb
}

/// Bilinear resize RGBA8888 → RGB f32 [0, 1].
fn resize_bilinear_rgba_to_rgb(rgba: &[u8], src_w: usize, src_h: usize) -> Vec<f32> {
    let dst_w = IMG_SIZE;
    let dst_h = IMG_SIZE;
    let mut out = vec![0.0f32; dst_w * dst_h * 3];

    for dy in 0..dst_h {
        for dx in 0..dst_w {
            let sx = (dx as f32 + 0.5) * src_w as f32 / dst_w as f32 - 0.5;
            let sy = (dy as f32 + 0.5) * src_h as f32 / dst_h as f32 - 0.5;
            let sx = sx.max(0.0).min((src_w - 1) as f32);
            let sy = sy.max(0.0).min((src_h - 1) as f32);

            let ix = sx as usize;
            let iy = sy as usize;
            let fx = sx - ix as f32;
            let fy = sy - iy as f32;
            let ix2 = (ix + 1).min(src_w - 1);
            let iy2 = (iy + 1).min(src_h - 1);

            for c in 0..3 {
                let p00 = rgba[(iy * src_w + ix) * 4 + c] as f32 / 255.0;
                let p10 = rgba[(iy * src_w + ix2) * 4 + c] as f32 / 255.0;
                let p01 = rgba[(iy2 * src_w + ix) * 4 + c] as f32 / 255.0;
                let p11 = rgba[(iy2 * src_w + ix2) * 4 + c] as f32 / 255.0;

                let top = p00 + (p10 - p00) * fx;
                let bot = p01 + (p11 - p01) * fx;
                out[(dy * dst_w + dx) * 3 + c] = top + (bot - top) * fy;
            }
        }
    }
    out
}

/// Extrair patches IMG_SIZE/PATCH_SIZE × IMG_SIZE/PATCH_SIZE.
fn extract_patches(rgb: &[f32], img_w: usize, img_h: usize, patch: usize) -> Vec<f32> {
    let num_h = img_h / patch;
    let num_w = img_w / patch;
    let num_patches = num_h * num_w;
    let patch_dim = patch * patch * 3;

    let mut patches = vec![0.0f32; num_patches * patch_dim];
    for ph in 0..num_h {
        for pw in 0..num_w {
            let pi = ph * num_w + pw;
            for py in 0..patch {
                for px in 0..patch {
                    for c in 0..3 {
                        let sy = ph * patch + py;
                        let sx = pw * patch + px;
                        let src_idx = (sy * img_w + sx) * 3 + c;
                        let dst_idx = pi * patch_dim + (py * patch + px) * 3 + c;
                        patches[dst_idx] = rgb[src_idx];
                    }
                }
            }
        }
    }
    patches
}

// ─── Neural network primitives ─────────────────────────────────────────

/// LayerNorm: ln(x) = (x - mean) / sqrt(var + eps) * weight + bias
fn layer_norm(x: &[f32], weight: &[f32], bias: &[f32], eps: f32) -> Vec<f32> {
    let n = x.len() as f32;
    let mean = x.iter().sum::<f32>() / n;
    let var = x.iter().map(|&v| (v - mean) * (v - mean)).sum::<f32>() / n;
    let inv_std = 1.0 / libm::sqrtf(var + eps);

    let mut out = Vec::with_capacity(x.len());
    for (i, &v) in x.iter().enumerate() {
        out.push((v - mean) * inv_std * weight[i] + bias[i]);
    }
    out
}

/// GELU approximation: x * 0.5 * (1 + tanh(√(2/π) * (x + 0.044715 * x³)))
fn gelu_approx(x: f32) -> f32 {
    const C: f32 = 0.7978845608028654; // sqrt(2/π)
    let x3 = x * x * x;
    0.5 * x * (1.0 + libm::tanhf(C * (x + 0.044715 * x3)))
}

/// Softmax over last dimension.
fn softmax(x: &[f32]) -> Vec<f32> {
    let max = x.iter().fold(core::f32::NEG_INFINITY, |a, &b| a.max(b));
    let exps: Vec<f32> = x.iter().map(|&v| libm::expf(v - max)).collect();
    let sum: f32 = exps.iter().sum();
    exps.iter().map(|&e| e / sum).collect()
}

/// Multihead self-attention (encoder, no causal mask).
/// Processes one head at a time to keep memory usage reasonable.
fn multihead_attention(
    q: &Tensor,
    k: &Tensor,
    v: &Tensor,
    num_heads: usize,
    head_dim: usize,
) -> Tensor {
    let seq_len = q.shape.0;
    let hidden = q.shape.1;
    let mut output = Tensor::new((seq_len, hidden));
    let inv_scale = 1.0 / libm::sqrtf(head_dim as f32);

    for h in 0..num_heads {
        let offset = h * head_dim;

        // scores(i,j) = Σ_d Q[i][offset+d] * K[j][offset+d] * inv_scale
        // Store attention probabilities temporarily in output's prefix
        for i in 0..seq_len {
            for j in 0..seq_len {
                let mut s = 0.0f32;
                let qi = i * hidden + offset;
                let kj = j * hidden + offset;
                for d in 0..head_dim {
                    s += q.data[qi + d] * k.data[kj + d];
                }
                output.data[i * seq_len + j] = s * inv_scale;
            }
        }

        // Softmax on each row
        for i in 0..seq_len {
            let row_start = i * seq_len;
            let sm = softmax(&output.data[row_start..row_start + seq_len]);
            output.data[row_start..row_start + seq_len].copy_from_slice(&sm);
        }

        // Weighted sum of V → head output
        let mut head_out = vec![0.0f32; seq_len * head_dim];
        for i in 0..seq_len {
            for d in 0..head_dim {
                let mut s = 0.0f32;
                for j in 0..seq_len {
                    s += output.data[i * seq_len + j] * v.data[j * hidden + offset + d];
                }
                head_out[i * head_dim + d] = s;
            }
        }

        // Copy head output to result
        for i in 0..seq_len {
            for d in 0..head_dim {
                output.data[i * hidden + offset + d] = head_out[i * head_dim + d];
            }
        }
    }

    output
}

/// Unpack PackedTernaryTensor → Vec<f32> (apply scale).
fn unpack_ternary_to_f32(t: &PackedTernaryTensor, scale: f32) -> Vec<f32> {
    let (rows, cols) = t.shape;
    let total = rows * cols;
    let mut out = vec![0.0f32; total];
    for i in 0..total {
        out[i] = t.get_weight(i) as f32 * scale;
    }
    out
}

// ─── Self-test ─────────────────────────────────────────────────────────

/// Verifica se as funções internas operam sem pânico.
/// Chamado uma vez no boot para validação.
pub fn self_test() -> bool {
    // layer_norm: entrada uniforme → saída ~0
    let x = vec![1.0f32; HIDDEN];
    let w = vec![1.0f32; HIDDEN];
    let b = vec![0.0f32; HIDDEN];
    let ln = layer_norm(&x, &w, &b, EPS);
    let max_err = ln.iter().map(|v| libm::fabsf(*v)).fold(0.0f32, f32::max);
    if max_err > 1e-4 {
        return false;
    }

    // gelu: x=0 → 0
    if libm::fabsf(gelu_approx(0.0)) > 1e-6 {
        return false;
    }
    // gelu: x large positive → ~x
    if libm::fabsf(gelu_approx(10.0) - 10.0) > 0.1 {
        return false;
    }

    // softmax: uniform → uniform probabilities
    let sm = softmax(&[1.0, 1.0, 1.0]);
    if libm::fabsf(sm[0] - 1.0 / 3.0) > 1e-6 {
        return false;
    }

    true
}
