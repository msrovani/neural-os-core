#![allow(dead_code)]
//! Bit-Plane Encoding para BitNet 1.58-bit — escovação de bits branchless.
//!
//! # Representação
//! Pesos ternários (-1, 0, +1) são codificados em dois planos de bits de 512 bits:
//!
//! | Valor Ternário | Sign Bit (S) | Mask Bit (M) | Operação |
//! |---------------|-------------|-------------|----------|
//! | +1            | 0           | 1           | Positivo, ativo |
//! | -1            | 1           | 1           | Negativo, ativo |
//! | 0             | 0           | 0           | Inativo (zero) |
//!
//! # Branchless Dot Product (AVX-512)
//! Para cada par (input, weight):
//! 1. `pos_terms = (Input AND Mask) AND NOT Sign` → VPTERNLOGD ctrl=0x20
//! 2. `neg_terms = (Input AND Mask) AND Sign` → VPTERNLOGD ctrl=0x80
//! 3. `cnt_pos = popcnt(pos_terms)` → VPOPCNTDQ
//! 4. `cnt_neg = popcnt(neg_terms)` → VPOPCNTDQ
//! 5. `result = cnt_pos - cnt_neg` → VPSUBD + VPREDD (redução)
//!
//! **Zero branches, zero float conversions, zero desvios condicionais.**
//! Cada iteração processa 512 bits = 512 pesos em paralelo.
//!
//! # Comparação com o kernel atual (unpack → FMA)
//! - **Atual**: unpack 4 bytes → 16 i8 → sign-extend i32 → cvt i32→f32 → FMA
//! - **Bit-plane**: AND/NOT/popcnt em 512 bits — 1 ciclo por 512 pesos
//! - **Ganho esperado**: 3-8× em inference throughput (depende do ratio 0.0)

use crate::tensor::{PackedTernaryTensor, Tensor};
use alloc::vec;

// ─── Bit-Plane Structures ─────────────────────────────────────────────────

/// Dois planos de bits representando pesos ternários.
/// Cada bitplane é um array de u64, com 64 bits por u64 (512 bits = 8 × u64).
#[derive(Clone)]
pub struct BitPlanes {
    /// S[i] = 1 se o peso i é negativo (-1), 0 caso contrário.
    pub sign: alloc::vec::Vec<u64>,
    /// M[i] = 1 se o peso i é não-zero (+1 ou -1), 0 caso contrário (zero).
    pub mask: alloc::vec::Vec<u64>,
    /// Número total de pesos representados.
    pub len: usize,
}

/// Tamanho de um vetor AVX-512 (512 bits = 64 bytes = 8 × u64).
const ZMM_U64_COUNT: usize = 8;

impl BitPlanes {
    /// Converte um `PackedTernaryTensor` em bit-planes.
    ///
    /// Cada byte packed contém 4 pesos em pares de bits (2 bits cada):
    /// - `00` = 0 (mask=0, sign=0)
    /// - `01` = +1 (mask=1, sign=0)
    /// - `10` = -1 (mask=1, sign=1)
    /// - `11` = não ocorre (inválido)
    ///
    /// O unpacking é 100% branchless.
    pub fn from_packed(packed: &PackedTernaryTensor) -> Self {
        let n = packed.packed_data.len() * 4; // 4 pesos por byte
        let words = (n + 63) / 64; // u64 words necessárias
        let mut sign = alloc::vec![0u64; words];
        let mut mask = alloc::vec![0u64; words];

        for (byte_idx, &byte) in packed.packed_data.iter().enumerate() {
            let base = byte_idx * 4;

            // 4 pesos por byte — desempacota branchless
            for bit_pair in 0..4 {
                let pair = (byte >> (bit_pair * 2)) & 0b11;
                let weight_idx = base + bit_pair;
                if weight_idx >= n {
                    break;
                }
                let word = weight_idx / 64;
                let bit = weight_idx % 64;

                // mask = (pair != 0) = (pair | (pair >> 1)) & 1 — branchless
                let m = ((pair | (pair >> 1)) & 1) as u64;
                // sign = (pair == 0b10) = (pair >> 1) & ~(pair & 1) — branchless
                let s = (((pair >> 1) & !(pair & 1)) & 1) as u64;

                mask[word] |= m << bit;
                sign[word] |= s << bit;
            }
        }

        Self { sign, mask, len: n }
    }

    /// Dot product escalar entre input bits e weight bit-planes.
    ///
    /// Retorna `sum(input[i] * weight[i])` onde weight[i] ∈ {-1, 0, +1}.
    /// Implementação scalar (fallback sem AVX-512).
    pub fn dot_product_scalar(&self, input_bits: &[u64]) -> i64 {
        let words = self.sign.len().min(input_bits.len());
        let mut pos_count: u64 = 0;
        let mut neg_count: u64 = 0;

        for i in 0..words {
            let inp = input_bits[i];
            let m = self.mask[i];
            let s = self.sign[i];

            // termos positivos: input & mask & !sign
            let pos = inp & m & !s;
            // termos negativos: input & mask & sign
            let neg = inp & m & s;

            pos_count += pos.count_ones() as u64;
            neg_count += neg.count_ones() as u64;
        }

        pos_count as i64 - neg_count as i64
    }
}

// ─── AVX-512 Implementation ──────────────────────────────────────────────

/// Dot product branchless com AVX-512 (VPTERNLOGD + VPOPCNTDQ).
///
/// Processa 512 bits (8 × u64) por iteração:
/// 1. VPTERNLOGD ctrl=0x20 → (A AND B) AND NOT C → positive terms
/// 2. VPTERNLOGD ctrl=0x80 → A AND B AND C → negative terms
/// 3. VPOPCNTDQ → popcnt per lane (8 × u64 → 8 × u64 counts)
/// 4. VPSUBD → diff = pos - neg
/// 5. VPREDD → reduzir 8 lane sums to scalar
///
/// # Safety
/// Requer AVX-512F + AVX-512VPOPCNTDQ.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f,avx512vpopcntdq")]
unsafe fn dot_product_avx512_impl(input: &[u64], sign: &[u64], mask: &[u64]) -> i64 {
    use core::arch::x86_64::*;

    let words = sign.len().min(input.len()).min(mask.len());
    let zmm_words = words & !(ZMM_U64_COUNT - 1); // múltiplo de 8

    let mut total_pos: i64 = 0;
    let mut total_neg: i64 = 0;

    let mut i = 0;
    while i < zmm_words {
        // Load full 512-bit ZMM from u64 arrays (8 u64s = 64 bytes)
        let v_inp = _mm512_loadu_si512(input.as_ptr().add(i) as *const __m512i);
        let v_sign = _mm512_loadu_si512(sign.as_ptr().add(i) as *const __m512i);
        let v_mask = _mm512_loadu_si512(mask.as_ptr().add(i) as *const __m512i);

        // 1. Positive terms: (Input AND Mask) AND NOT Sign
        // VPTERNLOGD with imm8=0x20: result = (A AND B) AND (NOT C)
        // Requires AVX-512F (VPTERNLOGD is part of AVX-512F)
        let pos_terms = _mm512_ternarylogic_epi32::<0x20>(v_inp, v_mask, v_sign);

        // 2. Negative terms: (Input AND Mask) AND Sign
        // VPTERNLOGD with imm8=0x80: result = A AND B AND C
        let neg_terms = _mm512_ternarylogic_epi32::<0x80>(v_inp, v_mask, v_sign);

        // 3. Population count per 32-bit lane
        let cnt_pos = _mm512_popcnt_epi32(pos_terms);
        let cnt_neg = _mm512_popcnt_epi32(neg_terms);

        // 5. Horizontal reduction: sum all 16 x i32 lanes
        total_pos += _mm512_reduce_add_epi32(cnt_pos) as i64;
        total_neg += _mm512_reduce_add_epi32(cnt_neg) as i64;

        i += ZMM_U64_COUNT;
    }

    // Scalar tail para remainder
    let mut pos_tail = 0u64;
    let mut neg_tail = 0u64;
    while i < words {
        let inp = input[i];
        let m = mask[i];
        let s = sign[i];
        pos_tail += (inp & m & !s).count_ones() as u64;
        neg_tail += (inp & m & s).count_ones() as u64;
        i += 1;
    }

    (total_pos + pos_tail as i64) - (total_neg + neg_tail as i64)
}

/// Dot product com dispatch automático (AVX-512 ou scalar).
pub fn dot_product(input: &[u64], bp: &BitPlanes) -> i64 {
    #[cfg(target_arch = "x86_64")]
    {
        if crate::bitnet_avx512::avx512_available() {
            return unsafe { dot_product_avx512_impl(input, &bp.sign, &bp.mask) };
        }
    }
    bp.dot_product_scalar(input)
}

// ─── Self-Test ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitplane_from_packed() {
        use crate::tensor::{quantize_to_packed, Tensor};

        // Pesos: [+1, -1, 0, +1] → em 1 byte packed:
        // bit pair 0: +1 → 01 (m=1, s=0)
        // bit pair 1: -1 → 10 (m=1, s=1)
        // bit pair 2: 0  → 00 (m=0, s=0)
        // bit pair 3: +1 → 01 (m=1, s=0)
        // byte = (01) | (10 << 2) | (00 << 4) | (01 << 6) = 0b01_00_10_01 = 0x49
        let weight_data = [1.0f32, -1.0, 0.0, 1.0];
        let w = Tensor::from_row_major((1, 4), weight_data.to_vec()).unwrap();
        let packed = quantize_to_packed(&w, 0.5);

        let bp = BitPlanes::from_packed(&packed);
        assert_eq!(bp.len, 4);

        // mask bits: 1,1,0,1 → 0b1011 = 0xB
        assert_eq!(bp.mask[0] & 0xF, 0b1011);
        // sign bits: 0,1,0,0 → 0b0010 = 0x2
        assert_eq!(bp.sign[0] & 0xF, 0b0010);
    }

    #[test]
    fn dot_product_scalar_basic() {
        // Weight: [+1, -1, 0, +1]
        let weight_data = [1.0f32, -1.0, 0.0, 1.0];
        let w = Tensor::from_row_major((1, 4), weight_data.to_vec()).unwrap();
        let packed = crate::tensor::quantize_to_packed(&w, 0.5);
        let bp = BitPlanes::from_packed(&packed);

        // Input bits: [1, 1, 1, 1] → all 4 bits set
        let input_bits = [0b1011u64]; // bits 0,1,3 set (same as mask)
        let result = bp.dot_product_scalar(&input_bits);

        // Expected: +1 (bit0) + (-1) (bit1) + 0 (bit2) + +1 (bit3) = 1
        assert_eq!(result, 1);
    }

    #[test]
    fn dot_product_all_zeros() {
        let weight_data = [0.0f32, 0.0, 0.0, 0.0];
        let w = Tensor::from_row_major((1, 4), weight_data.to_vec()).unwrap();
        let packed = crate::tensor::quantize_to_packed(&w, 0.5);
        let bp = BitPlanes::from_packed(&packed);

        let input_bits = [0b1111u64];
        let result = bp.dot_product_scalar(&input_bits);
        assert_eq!(result, 0);
    }

    #[test]
    fn dot_product_all_positive() {
        let weight_data = [1.0f32, 1.0, 1.0, 1.0];
        let w = Tensor::from_row_major((1, 4), weight_data.to_vec()).unwrap();
        let packed = crate::tensor::quantize_to_packed(&w, 0.5);
        let bp = BitPlanes::from_packed(&packed);

        let input_bits = [0b1111u64];
        let result = bp.dot_product_scalar(&input_bits);
        assert_eq!(result, 4);
    }

    #[test]
    fn dot_product_all_negative() {
        let weight_data = [-1.0f32, -1.0, -1.0, -1.0];
        let w = Tensor::from_row_major((1, 4), weight_data.to_vec()).unwrap();
        let packed = crate::tensor::quantize_to_packed(&w, 0.5);
        let bp = BitPlanes::from_packed(&packed);

        let input_bits = [0b1111u64];
        let result = bp.dot_product_scalar(&input_bits);
        assert_eq!(result, -4);
    }

    #[test]
    fn constants_correct() {
        assert_eq!(ZMM_U64_COUNT, 8);
    }
}
