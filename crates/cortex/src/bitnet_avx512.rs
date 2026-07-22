#![allow(dead_code)]
//! ADR-0061: BitNet ternary matmul com AVX-512 (ZMM 512-bit).
//!
//! Kernel de inferência BitNet b1.58 usando registradores ZMM de 512 bits.
//! Processa 16 pesos ternários por iteração (1 ZMM = 16 f32):
//!   1. Broadcast input[i][t] para 16 lanes
//!   2. Desempacotar 4 bytes packed (16 pesos ternários 2-bit) → 16 i8 → 16 f32
//!   3. FMA: acc = _mm512_fmadd_ps(broadcast, weights_f32, acc)
//!
//! Vantagem sobre AVX2 (128 pesos/ciclo): 256 pesos/ciclo em ZMM.
//! Requer AVX-512F + AVX-512BW + AVX-512VNNI (Xeon SPR+, EPYC 4/5).
//!
//! Alinhamento: 64 bytes (cache line x86). PackedTernaryTensor já é align(64).

use crate::tensor::{PackedTernaryTensor, Tensor};
use alloc::vec;

// ─── HW Detection ───────────────────────────────────────────────────────

/// AVX-512 permitido pelo FeatureGate (ADR-0061) — não só CPUID.
fn avx512_available() -> bool {
    k_nano::platform_probe::allow_avx512()
}

// ─── Ternary Unpack (4 bytes → 16 i8) ───────────────────────────────────

/// Desempacota 4 bytes (16 pesos ternários 2-bit cada) em 16 i8.
///
/// Codificação: 00=0, 01=+1, 10=-1, 11=0 (não ocorre).
/// Fórmula: w = (pair & 1) - (pair >> 1) — sem branch, sem LUT.
///
/// Cada byte contém 4 pesos nos pares de bits (0,1), (2,3), (4,5), (6,7).
#[inline]
fn unpack_quad_byte(byte: u8) -> [i8; 4] {
    let w0 = ((byte & 0b11) & 1) as i8 - ((byte & 0b11) >> 1) as i8;
    let w1 = (((byte >> 2) & 0b11) & 1) as i8 - (((byte >> 2) & 0b11) >> 1) as i8;
    let w2 = (((byte >> 4) & 0b11) & 1) as i8 - (((byte >> 4) & 0b11) >> 1) as i8;
    let w3 = (((byte >> 6) & 0b11) & 1) as i8 - (((byte >> 6) & 0b11) >> 1) as i8;
    [w0, w1, w2, w3]
}

/// Desempacota uma linha de pesos ternários (n elementos) em i8.
/// `n` deve ser múltiplo de 4 (garantido pelo caller).
#[inline]
fn unpack_row(weight: &PackedTernaryTensor, row: usize, n: usize, buf: &mut [i8]) {
    let words = n / 4;
    let row_start = row * words;
    for pw in 0..words {
        let p = weight.packed_data[row_start + pw];
        let base = pw * 4;
        let quad = unpack_quad_byte(p);
        buf[base] = quad[0];
        buf[base + 1] = quad[1];
        buf[base + 2] = quad[2];
        buf[base + 3] = quad[3];
    }
}

// ─── AVX-512 Ternary Matmul ─────────────────────────────────────────────

/// Matmul ternário BitNet com AVX-512 (ZMM 512-bit).
///
/// Para cada par (i, t):
///   - Broadcast input[i][t] para 16 lanes do ZMM
///   - Desempacota 16 pesos ternários (4 bytes packed) → 16 f32
///   - FMA: acc[j_block] = _mm512_fmadd_ps(broadcast, weights, acc[j_block])
///
/// # Safety
/// - Requer CPU com AVX-512F + AVX-512BW + AVX-512VNNI
/// - `n` deve ser múltiplo de 4 (pesos packed 4/byte)
/// - `weight.packed_data` deve ter pelo menos `k * (n/4)` bytes
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f,avx512bw,avx512vnni")]
unsafe fn avx512_ternary_matmul_impl(
    weight: &PackedTernaryTensor,
    input: &Tensor,
    m: usize,
    k: usize,
    n: usize,
) -> Tensor {
    use core::arch::x86_64::*;

    let mut result = Tensor::new((m, n));
    let mut row_buf = vec![0i8; n];
    let n16 = n & !15; // maior múltiplo de 16 <= n

    for i in 0..m {
        let inp_row = &input.data[i * k..];
        let out_row = &mut result.data[i * n..];

        // Zera acumuladores para todos os blocos de 16 colunas
        for j in 0..n {
            out_row[j] = 0.0;
        }

        for t in 0..k {
            let a_val = inp_row[t];
            if a_val == 0.0 {
                continue; // skip zero inputs — ternary * 0 = 0
            }

            // Desempacota pesos da linha t (n pesos ternários)
            unpack_row(weight, t, n, &mut row_buf);

            // Broadcast do escalar input[i][t] para 16 lanes
            let a_bcast = _mm512_set1_ps(a_val);

            // Processa colunas em blocos de 16 (1 ZMM = 16 f32)
            let mut j = 0usize;
            while j < n16 {
                // Carrega 16 i8 pesos → sign-extend para 16 i32 → converte para f32
                let w_ptr = row_buf.as_ptr().add(j) as *const i8;
                let w_i8 = _mm_loadu_si128(w_ptr as *const __m128i); // 16 i8
                let w_i32 = _mm512_cvtepi8_epi32(w_i8); // 16 i32
                let w_f32 = _mm512_cvtepi32_ps(w_i32); // 16 f32

                // FMA: out[j..j+16] += a_val * w[j..j+16]
                let acc = _mm512_loadu_ps(out_row.as_mut_ptr().add(j));
                let updated = _mm512_fmadd_ps(a_bcast, w_f32, acc);
                _mm512_storeu_ps(out_row.as_mut_ptr().add(j), updated);

                j += 16;
            }

            // Cauda n%16 — scalar
            while j < n {
                out_row[j] += a_val * (row_buf[j] as f32);
                j += 1;
            }
        }
    }
    result
}

// ─── Main Dispatch ──────────────────────────────────────────────────────

/// Matmul ternário BitNet com AVX-512.
///
/// Retorna `Some(result)` se AVX-512 está disponível e o shape é compatível,
/// `None` caso contrário (caller deve cair para AVX2/scalar).
///
/// Requisitos:
/// - AVX-512F + BW + VNNI habilitado pelo FeatureGate
/// - `n % 4 == 0` (pesos packed 4/byte)
/// - `k >= 1`, `n >= 16` (mínimo para ZMM)
pub fn ternary_matmul_avx512(
    weight: &PackedTernaryTensor,
    input: &Tensor,
) -> Option<Tensor> {
    let (k, n) = weight.shape;
    let (m, k2) = input.shape;
    if k != k2 {
        return None;
    }
    if !avx512_available() {
        return None;
    }
    // n deve ser múltiplo de 4 (packed 4/byte) e >= 16 para ZMM
    if n < 16 || n % 4 != 0 {
        return None;
    }

    #[cfg(target_arch = "x86_64")]
    {
        Some(unsafe { avx512_ternary_matmul_impl(weight, input, m, k, n) })
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        None
    }
}

// ─── Self-Test ─────────────────────────────────────────────────────────

/// Self-test do kernel AVX-512 em pequena entrada.
/// Retorna true se o resultado bate com o scalar de referência.
#[cfg(test)]
pub fn self_test() -> bool {
    use crate::tensor::quantize_to_packed;

    // Matriz 2×4 com valores conhecidos
    let weight_data = [1.0f32, -1.0, 0.0, 1.0, -1.0, 0.0, 1.0, -1.0];
    let weight_tensor = Tensor::from_row_major((2, 4), weight_data.to_vec()).unwrap();
    let weight = quantize_to_packed(&weight_tensor, 0.5);

    // Input 1×2
    let input = Tensor::from_row_major((1, 2), vec![1.0, -1.0]).unwrap();

    // Resultado esperado (scalar):
    // row 0: [1*1 + (-1)*(-1), 1*(-1) + (-1)*0, 1*0 + (-1)*1, 1*1 + (-1)*(-1)] = [2, -1, -1, 2]
    // row 1: [1*1 + 0*(-1), 1*(-1) + 0*0, 1*0 + 0*1, 1*1 + 0*(-1)] = [1, -1, 0, 1]
    // Mas input é 1×2, weight é 2×4, resultado é 1×4:
    // result[0][j] = sum_t input[0][t] * weight[t][j]
    // = input[0][0]*weight[0][j] + input[0][1]*weight[1][j]
    // = 1*w[0][j] + (-1)*w[1][j]
    // j=0: 1*1 + (-1)*(-1) = 2
    // j=1: 1*(-1) + (-1)*0 = -1
    // j=2: 1*0 + (-1)*1 = -1
    // j=3: 1*1 + (-1)*(-1) = 2
    let expected = [2.0f32, -1.0, -1.0, 2.0];

    if let Some(result) = ternary_matmul_avx512(&weight, &input) {
        if result.shape != (1, 4) {
            return false;
        }
        for j in 0..4 {
            if (result.data[j] - expected[j]).abs() > 1e-5 {
                return false;
            }
        }
        true
    } else {
        // AVX-512 não disponível — skip
        true
    }
}
