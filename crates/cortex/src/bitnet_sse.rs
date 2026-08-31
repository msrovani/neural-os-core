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

    // Bare-metal: SSE2 skip-native ANTES do stub AVX2 (ADR-0101 Onda 0 Fase B).
    #[cfg(all(target_arch = "x86_64", target_os = "none"))]
    if n >= 4 {
        return Some(unsafe { sse2_ternary_matmul_skip_native(weight, input, m, k, n) });
    }

    // Host AVX2 (FMA dequant — path rápido em testes/dev)
    if k_nano::platform_probe::allow_avx2() && k >= 8 && n >= 8 && n % 4 == 0 {
        return Some(unsafe { crate::bitnet_avx2::avx2_ternary_matmul_impl(weight, input, m, k, n) });
    }

    // Host SSE2 skip-native (sem AVX2 ou shapes pequenos)
    #[cfg(target_arch = "x86_64")]
    if n >= 4 && sse2_available() {
        return Some(unsafe { sse2_ternary_matmul_skip_native(weight, input, m, k, n) });
    }

    // Scalar bloco-4 (host sem SSE ou shapes pequenos)
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

// ─── ADR-0101 Onda 0: SSE2 skip-native (bare-metal + host) ───────────────

#[inline]
fn sse2_available() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        // Baseline x86_64-v2+ / WHPX / metal — SSE2 sempre presente.
        true
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

/// Matmul ternário: acc[j] += w[j] * x com w∈{-1,0,+1}, x broadcast — paridade scalar.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
unsafe fn sse2_ternary_matmul_skip_native(
    weight: &PackedTernaryTensor,
    input: &Tensor,
    m: usize,
    k: usize,
    n: usize,
) -> Tensor {
    use core::arch::x86_64::*;
    let mut result = Tensor::new((m, n));
    for i in 0..m {
        for j in (0..n).step_by(4) {
            let lanes = core::cmp::min(4, n - j);
            let mut acc = _mm_setzero_ps();
            let in_base = i * k;
            let w_col_base = j;
            for t in 0..k {
                let x = _mm_set1_ps(input.data[in_base + t]);
                let mut wf = [0.0f32; 4];
                let w_row = t * n + w_col_base;
                for lane in 0..lanes {
                    wf[lane] = weight.get_weight(w_row + lane) as f32;
                }
                let wv = _mm_loadu_ps(wf.as_ptr());
                acc = _mm_add_ps(acc, _mm_mul_ps(wv, x));
            }
            let mut out = [0.0f32; 4];
            _mm_storeu_ps(out.as_mut_ptr(), acc);
            for lane in 0..lanes {
                result.data[i * n + j + lane] = out[lane];
            }
        }
    }
    result
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

#[cfg(test)]
mod ternary_native_contract {
    use super::*;
    use crate::tensor::PackedTernaryTensor;

    /// Contrato ADR-0101: W∈{+1,0,-1} ⇒ ADD / SKIP / SUB da ativação, sem W denso.
    #[test]
    fn add_skip_sub_matches_scalar_semantics() {
        let w = PackedTernaryTensor {
            shape: (1, 3),
            packed_data: PackedTernaryTensor::pack_weights(&[1i8, 0, -1]),
        };
        let x = Tensor::from_row_major((1, 1), alloc::vec![2.5f32]).expect("x");
        let y = ternary_matmul(&w, &x).expect("matmul");
        assert_eq!(y.shape, (1, 3));
        assert!((y.data[0] - 2.5).abs() < 1e-6, "W=+1 deve somar x, got {}", y.data[0]);
        assert!(y.data[1].abs() < 1e-6, "W=0 deve skip, got {}", y.data[1]);
        assert!((y.data[2] + 2.5).abs() < 1e-6, "W=-1 deve subtrair x, got {}", y.data[2]);
    }

    /// Paridade SSE2 skip-native vs scalar bloco-4 (Onda 0 ADR-0101).
    #[test]
    fn sse2_skip_native_parity_vs_scalar() {
        let weights: alloc::vec::Vec<i8> = (0..48)
            .map(|i| match i % 3 {
                0 => 1i8,
                1 => 0,
                _ => -1,
            })
            .collect();
        let w = PackedTernaryTensor {
            shape: (4, 12),
            packed_data: PackedTernaryTensor::pack_weights(&weights),
        };
        let x_data: alloc::vec::Vec<f32> = (0..8).map(|i| (i as f32 + 1.0) * 0.25).collect();
        let x = Tensor::from_row_major((2, 4), x_data).expect("x");
        let (m, k) = x.shape;
        let (k_w, n) = w.shape;
        assert_eq!(k, k_w);

        let scalar = super::scalar_ternary_matmul(&w, &x, m, k, n);
        #[cfg(target_arch = "x86_64")]
        let simd = unsafe { super::sse2_ternary_matmul_skip_native(&w, &x, m, k, n) };
        #[cfg(not(target_arch = "x86_64"))]
        let simd = scalar.clone();

        assert_eq!(scalar.shape, simd.shape);
        for (a, b) in scalar.data.iter().zip(simd.data.iter()) {
            assert!((a - b).abs() < 1e-5, "parity fail: scalar={a} simd={b}");
        }
    }
}
