//! Layout W2A8 device (C1) — TQ2-style signed int8, **sem** viés +128 do maddubs CPU.
//!
//! Dp4a / MAD / Dot no GPU usam ativação i8 ∈ [-127,127] e pesos ternários i8.
//! CPU AVX2 maddubs usa u8 = q+128 — **não** reutilizar esse layout no device.
//!
//! Upload VRAM = residual AWAITING_HW (DMA/CE); este módulo prepara bytes +
//! verifica paridade host vs `gpu_kernels`/`bitnet_w2a8` golden quantizado.

use alloc::vec::Vec;
use cortex::tensor::PackedTernaryTensor;
use crate::gpu::compute_abi::OpProfile;

/// Buffer pronto para upload (sysmem hoje; CE→VRAM no metal).
#[derive(Debug, Clone)]
pub struct W2a8DeviceBuffers {
    pub profile: OpProfile,
    /// Ativações i8 signed (M*K), row-major.
    pub x_i8: Vec<i8>,
    /// Pesos i8 coluna-major (N*K) ∈ {-1,0,1}.
    pub w_i8: Vec<i8>,
    pub m: usize,
    pub k: usize,
    pub n: usize,
    /// Escala por linha de x (si).
    pub si: Vec<f32>,
}

/// Quantiza linha f32 → i8 signed (round, clamp) — espelho GPU dp4a/mad.
fn quantize_row_signed(row: &[f32], out: &mut [i8]) -> f32 {
    let mut max_abs = 0.0f32;
    for &v in row {
        let a = v.abs();
        if a > max_abs {
            max_abs = a;
        }
    }
    let si = if max_abs > 1e-9 { max_abs / 127.0 } else { 1.0 };
    let inv = 1.0 / si;
    for (t, &v) in row.iter().enumerate() {
        let q = round_nearest(v * inv) as i32;
        out[t] = q.clamp(-127, 127) as i8;
    }
    si
}

#[inline]
fn round_nearest(x: f32) -> f32 {
    if x >= 0.0 {
        (x + 0.5) as i32 as f32
    } else {
        (x - 0.5) as i32 as f32
    }
}

fn unpack_pair(byte: u8, lane: usize) -> i8 {
    let pair = (byte >> ((lane & 3) << 1)) & 3;
    ((pair & 1) as i8) - ((pair >> 1) as i8)
}

/// Repack packed W2 (k,n) → coluna-major i8 (n,k).
pub fn pack_weights_col_major(w: &PackedTernaryTensor) -> Option<Vec<i8>> {
    let (k, n) = w.shape;
    if k == 0 || n == 0 {
        return None;
    }
    let need = (k * n + 3) / 4;
    if w.packed_data.len() < need {
        return None;
    }
    let mut out = Vec::new();
    if out.try_reserve_exact(n * k).is_err() {
        return None;
    }
    out.resize(n * k, 0i8);
    for j in 0..n {
        for t in 0..k {
            let idx = t * n + j;
            let byte = w.packed_data[idx >> 2];
            out[j * k + t] = unpack_pair(byte, idx);
        }
    }
    Some(out)
}

/// Prepara buffers device a partir de tensores Cortex.
pub fn prepare_device_buffers(
    w: &PackedTernaryTensor,
    x: &cortex::tensor::Tensor,
    profile: OpProfile,
) -> Option<W2a8DeviceBuffers> {
    let (k, n) = w.shape;
    let (m, k2) = x.shape;
    if k != k2 || m == 0 || n == 0 || !x.is_valid() {
        return None;
    }
    let w_i8 = pack_weights_col_major(w)?;
    let mut x_i8 = Vec::new();
    let mut si = Vec::new();
    if x_i8.try_reserve_exact(m * k).is_err() || si.try_reserve_exact(m).is_err() {
        return None;
    }
    x_i8.resize(m * k, 0i8);
    si.resize(m, 0.0f32);
    for i in 0..m {
        let row = &x.data[i * k..(i + 1) * k];
        si[i] = quantize_row_signed(row, &mut x_i8[i * k..(i + 1) * k]);
    }
    let _ = profile; // layout idêntico Dp4a/Mad/Dot signed; encoding no kernel ISA
    Some(W2a8DeviceBuffers {
        profile,
        x_i8,
        w_i8,
        m,
        k,
        n,
        si,
    })
}

/// GEMV host com o **mesmo** quant signed (golden de referência p/ device).
/// out[i,j] = si_i · Σ_t x_i8[i,t] · w_i8[j,t]
pub fn host_gemv_signed(buf: &W2a8DeviceBuffers) -> Option<Vec<f32>> {
    let mut out = Vec::new();
    if out.try_reserve_exact(buf.m * buf.n).is_err() {
        return None;
    }
    out.resize(buf.m * buf.n, 0.0f32);
    for i in 0..buf.m {
        let xi = &buf.x_i8[i * buf.k..(i + 1) * buf.k];
        let si = buf.si[i];
        for j in 0..buf.n {
            let wj = &buf.w_i8[j * buf.k..(j + 1) * buf.k];
            let mut acc = 0i32;
            for t in 0..buf.k {
                acc += xi[t] as i32 * wj[t] as i32;
            }
            out[i * buf.n + j] = acc as f32 * si;
        }
    }
    Some(out)
}

/// Bytes totais para upload (x + w).
pub fn upload_byte_len(buf: &W2a8DeviceBuffers) -> usize {
    buf.x_i8.len() + buf.w_i8.len()
}

/// Tenta “upload” — hoje só valida tamanho; CE/VRAM = AWAITING_HW.
/// Retorna false se stub/profile Scalar (não sobe device).
pub fn try_stage_upload(buf: &W2a8DeviceBuffers) -> bool {
    if matches!(buf.profile, OpProfile::ScalarInt8) {
        return false;
    }
    upload_byte_len(buf) > 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use cortex::tensor::{PackedTernaryTensor, Tensor};

    fn ones_weights(k: usize, n: usize) -> PackedTernaryTensor {
        // todos +1 → pairs 0b01
        let need = (k * n + 3) / 4;
        let packed = alloc::vec![0x55u8; need];
        PackedTernaryTensor {
            packed_data: packed,
            shape: (k, n),
        }
    }

    #[test]
    fn layout_no_plus128_bias() {
        let w = ones_weights(4, 1);
        let mut x = Tensor::new((1, 4));
        assert!(x.is_valid());
        x.data = alloc::vec![1.0, 2.0, 3.0, 4.0];
        let buf = prepare_device_buffers(&w, &x, OpProfile::Dp4aW2A8).expect("prep");
        // signed quant: valores positivos → i8 > 0, nunca u8 offset
        assert!(buf.x_i8.iter().all(|&v| v >= 0));
        assert!(buf.w_i8.iter().all(|&v| v == 1));
        let out = host_gemv_signed(&buf).expect("gemv");
        assert!((out[0] - 10.0).abs() < 0.6, "got {}", out[0]);
        assert!(try_stage_upload(&buf));
    }

    #[test]
    fn scalar_profile_skips_upload() {
        let w = ones_weights(2, 2);
        let mut x = Tensor::new((1, 2));
        x.data = alloc::vec![1.0, 1.0];
        let buf = prepare_device_buffers(&w, &x, OpProfile::ScalarInt8).expect("prep");
        assert!(!try_stage_upload(&buf));
    }
}
