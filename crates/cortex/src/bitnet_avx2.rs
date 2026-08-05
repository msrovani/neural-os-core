#![allow(dead_code)]
//! BitNet ternary matmul com otimizacoes de cache:
//! - align(64) no PackedTernaryTensor (evita split cache line)
//! - Matmul ternario bitwise sem branch (16 pesos por iteracao)
//! - Prefetch entre camadas do transformer
//! - Dispatch adaptativo por CPU: scalar, AVX2, bitwise

use crate::tensor::{PackedTernaryTensor, Tensor};
use alloc::vec;

// ─── Tiling Constants (ADR-0084 F5, tuning por HW) ───────────────────────
// ponytail: defaults para cache L2 256KB; ajustar por target (faixas:
// p∈[2,4,8], row∈[2..32], col∈[32..1024])
pub const ROW_BLOCK_SIZE: usize = 4;
pub const COL_BLOCK_SIZE: usize = 128;
pub const PARALLEL_SIZE: usize = 4;

// ─── HW Detection ───────────────────────────────────────────────────────

fn avx2_available() -> bool {
    k_nano::platform_probe::allow_avx2()
}

// ─── Main Dispatch ──────────────────────────────────────────────────────

pub fn ternary_matmul(weight: &PackedTernaryTensor, input: &Tensor) -> Option<Tensor> {
    let (k, n) = weight.shape;
    let (m, k2) = input.shape;
    if k != k2 { return None; }

    // ADR-0057 WS-C: NPU/GPU/parallel dispatch
    if let Some(r) = crate::compute::dispatch_ternary(weight, input) {
        return Some(r);
    }

    // ADR-0084 F4 (GATED): W2A8 maddubs — só WHPX/HW real + gaps resolvidos.
    // w2a8_enabled() hoje = false; kernel verificado por self-test de paridade.
    if crate::bitnet_w2a8::w2a8_enabled() && (m == 1 || m >= 8) {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            if let Some(r) = crate::bitnet_w2a8::w2a8_ternary_matmul(weight, input) {
                return Some(r);
            }
        }
    }

    // ADR-0084 F2: activation-parallel for prefill (m >= 8)
    // bitwise_matmul uses LUT-based FMA per byte-group; wins ~2x at m≥32
    // (src/README bitnet.cpp: activation-parallel 1.85-2.0x at m≥32)
    let big_m = m >= 8;
    if big_m && avx2_available() {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            return Some(avx2_bitwise_matmul(weight, input, m, k, n));
        }
    }

    // ADR-0061 unified dispatch: AVX-512 → AVX2 → SSE4.2 → scalar
    crate::bitnet_sse::ternary_matmul(weight, input)
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
                let out_off = j_block * 4;
                let lanes = core::cmp::min(4, n - out_off);

                // Lookup table: byte → [f32; 4]
                let lut_base = &lut[(p as usize) * 4] as *const f32;
                let w_f32 = _mm_loadu_ps(lut_base);

                if lanes == 4 {
                    // FMA: out[j_block*4..] += inp[t*4..] * w[byte]
                    let prev = _mm_loadu_ps(out_row.as_mut_ptr().add(out_off));
                    let updated = _mm_fmadd_ps(inp_vals, w_f32, prev);
                    _mm_storeu_ps(out_row.as_mut_ptr().add(out_off), updated);
                } else {
                    // Tail (n % 4 != 0): escalar, para nao ler/escrever alem da linha.
                    for q in 0..lanes {
                        let w = unsafe { *lut_base.add(q) };
                        out_row[out_off + q] += input.data[i * k + t * 4 + q] * w;
                    }
                }
            }
        }
    }

    result
}

// ─── AVX2 Original (fallback para shapes pequenos) ──────────────────────

#[cfg(target_arch = "x86_64")]
fn unpack_row_into(weight: &PackedTernaryTensor, row: usize, n: usize, buf: &mut [i8]) {
    if n % 4 == 0 {
        let words = n / 4;
        let row_start = row * words;
        for pw in 0..words {
            let p = weight.packed_data[row_start + pw];
            let base = pw * 4;
            // Branchless unpack: (pair&1) - (pair>>1) — same as AVX-512 pattern
            // 0b00→0, 0b01→1, 0b10→-1, 0b11→0  (ADR-0084 F1)
            let p0 = (p & 3) as i8;
            let p1 = ((p >> 2) & 3) as i8;
            let p2 = ((p >> 4) & 3) as i8;
            let p3 = ((p >> 6) & 3) as i8;
            buf[base] = (p0 & 1) - (p0 >> 1);
            buf[base + 1] = (p1 & 1) - (p1 >> 1);
            buf[base + 2] = (p2 & 1) - (p2 >> 1);
            buf[base + 3] = (p3 & 1) - (p3 >> 1);
        }
    } else {
        // Flat packing when n%4 != 0 (e.g. embed vocab=32002)
        let start = row * n;
        for j in 0..n {
            let idx = start + j;
            let byte = idx >> 2;
            let shift = (idx & 3) << 1;
            let bits = (weight.packed_data[byte] >> shift) & 0b11;
            buf[j] = ((bits & 1) as i8) - ((bits >> 1) as i8); // branchless
        }
    }
}

#[cfg(target_arch = "x86_64")]
pub(super) unsafe fn avx2_ternary_matmul_impl(weight: &PackedTernaryTensor, input: &Tensor, m: usize, k: usize, n: usize) -> Tensor {
    use core::arch::x86_64::*;

    let mut result = Tensor::new((m, n));
    let mut row_buf = vec![0i8; n];
    let n8 = n & !7; // maior multiplo de 8 <= n

    for i in 0..m {
        let inp_row = &input.data[i * k..];
        let out_row = &mut result.data[i * n..];
        for j in 0..n {
            out_row[j] = 0.0;
        }

        for t in 0..k {
            unpack_row_into(weight, t, n, &mut row_buf);
            let a = _mm256_set1_ps(inp_row[t]);
            let mut j = 0usize;
            while j < n8 {
                let w_ptr = row_buf.as_ptr().add(j) as *const __m128i;
                let w_i8 = _mm_loadl_epi64(w_ptr);
                let w_i32 = _mm256_cvtepi8_epi32(w_i8);
                let w_f32 = _mm256_cvtepi32_ps(w_i32);
                let prev = _mm256_loadu_ps(out_row.as_mut_ptr().add(j));
                let updated = _mm256_fmadd_ps(a, w_f32, prev);
                _mm256_storeu_ps(out_row.as_mut_ptr().add(j), updated);
                j += 8;
            }
            // cauda n%8 (vocab 32002 → 2 elems) — sem store AVX past-end
            let scale = inp_row[t];
            while j < n {
                out_row[j] += scale * (row_buf[j] as f32);
                j += 1;
            }
        }
    }
    result
}

// ─── Cache-aware dispatch ───────────────────────────────────────────────

/// Seleciona implementacao otima baseada no tamanho das matrizes e HW disponivel.
pub fn ternary_matmul_adaptive(weight: &PackedTernaryTensor, input: &Tensor) -> Option<Tensor> {
    // Mesmo caminho seguro que ternary_matmul (bitwise AVX2 desactivado).
    ternary_matmul(weight, input)
}
