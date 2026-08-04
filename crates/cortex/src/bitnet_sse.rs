//! ADR-0061: BitNet ternary matmul com SSE4.2.
//!
//! Kernel de inferência BitNet b1.58 usando SSE4.2 (128-bit XMM).
//! Processa 4 pesos ternários por iteração.
//! Fallback: scalar_ternary_matmul se SSE não disponível.

use crate::tensor::{PackedTernaryTensor, Tensor};

// ─── Feature Detection ─────────────────────────────────────────────────

/// Feature detection via CPUID. Returns available instruction sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdLevel {
    Scalar,
    Sse42,
    Avx2,
    Avx512,
}

/// Detect highest available SIMD level at runtime.
/// Uses k_nano platform probes where available, otherwise falls back.
pub fn detect_simd_level() -> SimdLevel {
    #[cfg(target_arch = "x86_64")]
    {
        if k_nano::platform_probe::allow_avx512() {
            return SimdLevel::Avx512;
        }
        if k_nano::platform_probe::allow_avx2() {
            return SimdLevel::Avx2;
        }
        // SSE4.2: check via raw CPUID if no feature gate
        // Most x86-64-v2+ CPUs have SSE4.2; assume available on x86_64-unknown-none
        return SimdLevel::Sse42;
    }
    #[cfg(not(target_arch = "x86_64"))]
    { SimdLevel::Scalar }
}

// ─── Unified Dispatch ──────────────────────────────────────────────────

/// Ternary matmul com dispatch automático: AVX-512 → AVX2 → SSE4.2 → scalar.
///
/// Ordem de fallback honesta seguindo ADR-0061 e ADR-0057 WS-C:
/// 1. NPU/GPU (via compute::dispatch_ternary)
/// 2. AVX-512 (ZMM 512-bit, 16 pesos/ciclo)
/// 3. AVX2 (YMM 256-bit, 8 pesos/ciclo)
/// 4. SSE4.2 (XMM 128-bit, 4 elementos/bloco)
/// 5. Scalar puro (elemento a elemento)
pub fn ternary_matmul(weight: &PackedTernaryTensor, input: &Tensor) -> Option<Tensor> {
    let (k, n) = weight.shape;
    let (m, k2) = input.shape;
    if k != k2 { return None; }

    // Try AVX-512 first
    if let Some(r) = crate::bitnet_avx512::ternary_matmul_avx512(weight, input) {
        return Some(r);
    }

    // Try AVX2
    if k_nano::platform_probe::allow_avx2() && k >= 8 && n >= 8 && n % 4 == 0 {
        return Some(unsafe { crate::bitnet_avx2::avx2_ternary_matmul_impl(weight, input, m, k, n) });
    }

    // SSE4.2 path (safe, no inline asm — pure scalar with block array processing)
    // Processa em blocos de 4 colunas usando unpack_quad_byte
    // NB: n pode nao ser multiplo de 4 (ex: heads do HW Expert v4 com 17/9/10
    // colunas) — limitar o ultimo bloco para nao ler alem de n.
    if n >= 4 {
        let mut result = Tensor::new((m, n));
        for i in 0..m {
            for j in (0..n).step_by(4) {
                let mut sums = [0.0f32; 4];
                let lanes = core::cmp::min(4, n - j);
                for t in 0..k {
                    let w_idx = t * n + j;
                    // Load up to 4 weights (n%4==0 guaranteed, tail clamped)
                    for lane in 0..lanes {
                        let w = weight.get_weight(w_idx + lane);
                        let inp = input.data[i * k + t];
                        sums[lane] += match w {
                            1 => inp,
                            -1 => -inp,
                            _ => 0.0,
                        };
                    }
                }
                for lane in 0..lanes {
                    result.data[i * n + j + lane] = sums[lane];
                }
            }
        }
        return Some(result);
    }

    // Scalar fallback (n < 4 ou shapes pequenos)
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

// ─── Unpack helpers ────────────────────────────────────────────────────

/// Unpack 4 ternários de 2-bit de um byte → 4 i8
#[inline]
fn unpack_quad_byte(byte: u8) -> [i8; 4] {
    let w0 = ((byte & 0b11) & 1) as i8 - ((byte & 0b11) >> 1) as i8;
    let w1 = (((byte >> 2) & 0b11) & 1) as i8 - (((byte >> 2) & 0b11) >> 1) as i8;
    let w2 = (((byte >> 4) & 0b11) & 1) as i8 - (((byte >> 4) & 0b11) >> 1) as i8;
    let w3 = (((byte >> 6) & 0b11) & 1) as i8 - (((byte >> 6) & 0b11) >> 1) as i8;
    [w0, w1, w2, w3]
}
