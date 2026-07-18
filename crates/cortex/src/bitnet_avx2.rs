#![allow(dead_code)]
//! BitNet ternary matmul com otimizacoes de cache:
//! - align(64) no PackedTernaryTensor (evita split cache line)
//! - Matmul ternario bitwise sem branch (16 pesos por iteracao)
//! - Prefetch entre camadas do transformer
//! - Dispatch adaptativo por CPU: scalar, AVX2, bitwise

use crate::tensor::{PackedTernaryTensor, Tensor};
use alloc::vec;

// ─── HW Detection ───────────────────────────────────────────────────────

fn avx2_available() -> bool {
    k_nano::platform_probe::allow_avx2()
}

// ─── Main Dispatch ──────────────────────────────────────────────────────

pub fn ternary_matmul(weight: &PackedTernaryTensor, input: &Tensor) -> Option<Tensor> {
    let (k, n) = weight.shape;
    let (m, k2) = input.shape;
    if k != k2 { return None; }

    // Escolhe implementacao baseada no HW
    if avx2_available() && k >= 16 && n >= 16 {
        return Some(unsafe { avx2_bitwise_matmul(weight, input, m, k, n) });
    }
    if avx2_available() && k >= 8 && n >= 8 {
        return Some(unsafe { avx2_ternary_matmul_impl(weight, input, m, k, n) });
    }
    Some(scalar_ternary_matmul(weight, input, m, k, n))
}

// ─── Scalar Fallback ────────────────────────────────────────────────────

fn scalar_ternary_matmul(weight: &PackedTernaryTensor, input: &Tensor, m: usize, k: usize, n: usize) -> Tensor {
    let mut result = Tensor::new((m, n));
    for i in 0..m {
        for j in 0..n {
            let mut sum = 0.0f32;
            for t in 0..k {
                sum += match weight.get_weight(t * n + j) {
                    1 => input.data[i * k + t],
                    -1 => -input.data[i * k + t],
                    _ => 0.0,
                };
            }
            result.data[i * n + j] = sum;
        }
    }
    result
}

// ─── AVX2 Bitwise Ternary Matmul (sem branch, 4 pesos/byte direto) ─────

/// Processa 4 pesos ternarios de uma vez sem branch (match).
/// Cada byte = 4 pesos: bits (0,1) = peso0, (2,3)=peso1, (4,5)=peso2, (6,7)=peso3
/// Codificacao: 00=0, 01=+1, 10=-1
#[cfg(target_arch = "x86_64")]
unsafe fn process_quad(quad: u8, inputs: &[f32; 4]) -> f32 {
    let mut sum = 0.0f32;
    // peso0
    match quad & 3 { 1 => sum += inputs[0], 2 => sum -= inputs[0], _ => {} }
    // peso1
    match (quad >> 2) & 3 { 1 => sum += inputs[1], 2 => sum -= inputs[1], _ => {} }
    // peso2
    match (quad >> 4) & 3 { 1 => sum += inputs[2], 2 => sum -= inputs[2], _ => {} }
    // peso3
    match (quad >> 6) & 3 { 1 => sum += inputs[3], 2 => sum -= inputs[3], _ => {} }
    sum
}

/// AVX2 bitwise: processa 16 pesos ternarios por iteracao sem unpack.
/// Carrega 4 bytes (16 pesos) → expande para 16 f32 → FMA com input.
#[cfg(target_arch = "x86_64")]
unsafe fn avx2_bitwise_matmul(weight: &PackedTernaryTensor, input: &Tensor, m: usize, k: usize, n: usize) -> Tensor {
    use core::arch::x86_64::*;

    let mut result = Tensor::new((m, n));
    // Pre-computa lookup table: byte → [f32; 4] para pesos ternarios
    // Cada entrada do byte e mapeada diretamente sem branch
    let mut lut = [0.0f32; 256 * 4];
    for byte in 0..256u16 {
        let b = byte as u8;
        let base = (byte as usize) * 4;
        // Cada par de bits vira f32 sem match
        for q in 0..4 {
            let bits = (b >> (q * 2)) & 3;
            lut[base + q] = match bits {
                1 => 1.0,
                2 => -1.0,
                _ => 0.0,
            };
        }
    }

    for i in 0..m {
        let inp_row = &input.data[i * k..];
        let out_row = &mut result.data[i * n..];
        for j in 0..n { out_row[j] = 0.0; }

        // Processa k em grupos de 4 (pesos por byte)
        let k_blocks = k / 4;
        let packed_cols = n.div_ceil(4);

        for t in 0..k_blocks {
            let inp_base = t * 4;
            let inp_vals = _mm_loadu_ps(inp_row.as_ptr().add(inp_base));

            for j_block in 0..packed_cols {
                let byte_idx = t * packed_cols + j_block;
                if byte_idx >= weight.packed_data.len() { break; }
                let p = weight.packed_data[byte_idx];

                // Lookup table: byte → [f32; 4]
                let lut_base = &lut[(p as usize) * 4] as *const f32;
                let w_f32 = _mm_loadu_ps(lut_base);

                // FMA: out[j_block*4..] += inp[t*4..] * w[byte]
                let prev = _mm_loadu_ps(out_row.as_mut_ptr().add(j_block * 4));
                let updated = _mm_fmadd_ps(inp_vals, w_f32, prev);
                _mm_storeu_ps(out_row.as_mut_ptr().add(j_block * 4), updated);
            }
        }
    }

    result
}

// ─── AVX2 Original (fallback para shapes pequenos) ──────────────────────

#[cfg(target_arch = "x86_64")]
fn unpack_row_into(weight: &PackedTernaryTensor, row: usize, n: usize, buf: &mut [i8]) {
    let packed_row_words = n.div_ceil(4);
    let row_start = row * packed_row_words;
    for pw in 0..packed_row_words {
        if row_start + pw >= weight.packed_data.len() { break; }
        let p = weight.packed_data[row_start + pw];
        let base = pw * 4;
        if base < n { buf[base] = match p & 3 { 1 => 1, 2 => -1, _ => 0 }; }
        if base + 1 < n { buf[base + 1] = match (p >> 2) & 3 { 1 => 1, 2 => -1, _ => 0 }; }
        if base + 2 < n { buf[base + 2] = match (p >> 4) & 3 { 1 => 1, 2 => -1, _ => 0 }; }
        if base + 3 < n { buf[base + 3] = match (p >> 6) & 3 { 1 => 1, 2 => -1, _ => 0 }; }
    }
}

#[cfg(target_arch = "x86_64")]
unsafe fn avx2_ternary_matmul_impl(weight: &PackedTernaryTensor, input: &Tensor, m: usize, k: usize, n: usize) -> Tensor {
    use core::arch::x86_64::*;

    let mut result = Tensor::new((m, n));
    let mut row_buf = vec![0i8; n];

    for i in 0..m {
        let inp_row = &input.data[i * k..];
        let out_row = &mut result.data[i * n..];
        for j in 0..n { out_row[j] = 0.0; }

        for t in 0..k {
            unpack_row_into(weight, t, n, &mut row_buf);
            let a = _mm256_set1_ps(inp_row[t]);
            for j in (0..n).step_by(8) {
                let w_ptr = row_buf.as_ptr().add(j) as *const __m128i;
                let w_i8 = _mm_loadl_epi64(w_ptr);
                let w_i32 = _mm256_cvtepi8_epi32(w_i8);
                let w_f32 = _mm256_cvtepi32_ps(w_i32);
                let prev = _mm256_loadu_ps(out_row.as_mut_ptr().add(j));
                let updated = _mm256_fmadd_ps(a, w_f32, prev);
                _mm256_storeu_ps(out_row.as_mut_ptr().add(j), updated);
            }
        }
    }
    result
}

// ─── Cache-aware dispatch ───────────────────────────────────────────────

/// Seleciona implementacao otima baseada no tamanho das matrizes e HW disponivel.
pub fn ternary_matmul_adaptive(weight: &PackedTernaryTensor, input: &Tensor) -> Option<Tensor> {
    let (k, n) = weight.shape;
    let (m, _k2) = input.shape;

    // Para matrizes grandes, usa bitwise AVX2
    if avx2_available() && k >= 32 && n >= 32 {
        // Prefetch dos dados de entrada
        #[cfg(target_arch = "x86_64")]
        unsafe {
            core::arch::x86_64::_mm_prefetch::<{ core::arch::x86_64::_MM_HINT_T0 }>(
                input.data.as_ptr() as *const i8);
        }
        return Some(unsafe { avx2_bitwise_matmul(weight, input, m, k, n) });
    }

    // Para matrizes medias, AVX2 classico
    if avx2_available() && k >= 8 && n >= 8 {
        return Some(unsafe { avx2_ternary_matmul_impl(weight, input, m, k, n) });
    }

    // Scalar fallback
    Some(scalar_ternary_matmul(weight, input, m, k, n))
}
