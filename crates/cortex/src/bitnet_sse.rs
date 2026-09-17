//! ADR-0061 / ADR-0101: BitNet ternary matmul SSE2.
//!
//! Contrato Onda 0: W∈{-1,0,+1} ⇒ ADD / SUB / SKIP da ativação — **sem** `W*x` mul.
//! Accumulação em XMM (`_mm_add_ps`). Soft-float: sem intrins XMM de load/store i8
//! (SESSION_336); pesos lidos via `get_weight` + delta f32.

use crate::tensor::{PackedTernaryTensor, Tensor};

// ─── Feature Detection ─────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdLevel {
    Scalar,
    Sse42,
    Avx2,
    Avx512,
}

pub fn detect_simd_level() -> SimdLevel {
    #[cfg(target_arch = "x86_64")]
    {
        if k_nano::platform_probe::allow_avx512() {
            return SimdLevel::Avx512;
        }
        if k_nano::platform_probe::allow_avx2() {
            return SimdLevel::Avx2;
        }
        return SimdLevel::Sse42;
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        SimdLevel::Scalar
    }
}

/// Ternary matmul: AVX-512 → AVX2 host → SSE2 ADD/SUB/SKIP → scalar.
pub fn ternary_matmul(weight: &PackedTernaryTensor, input: &Tensor) -> Option<Tensor> {
    let (k, n) = weight.shape;
    let (m, k2) = input.shape;
    if k != k2 || !input.is_valid() || m == 0 || n == 0 || k == 0 {
        return None;
    }

    if let Some(r) = crate::bitnet_avx512::ternary_matmul_avx512(weight, input) {
        return if r.is_valid() { Some(r) } else { None };
    }

    // Bare-metal: SSE2 ADD/SUB/SKIP antes do stub AVX2 (ADR-0101 Onda 0).
    #[cfg(all(target_arch = "x86_64", target_os = "none"))]
    if n >= 4 {
        return Some(unsafe { sse2_ternary_matmul_add_sub_skip(weight, input, m, k, n) });
    }

    // Host AVX2 (FMA dequant — bandwidth ≠ contrato lab; ok em testes)
    if k_nano::platform_probe::allow_avx2() && k >= 8 && n >= 8 && n % 4 == 0 {
        return Some(unsafe { crate::bitnet_avx2::avx2_ternary_matmul_impl(weight, input, m, k, n) });
    }

    #[cfg(target_arch = "x86_64")]
    if n >= 4 {
        return Some(unsafe { sse2_ternary_matmul_add_sub_skip(weight, input, m, k, n) });
    }

    if n >= 4 {
        let mut result = Tensor::new((m, n));
        if !result.is_valid() {
            return None;
        }
        for i in 0..m {
            for j in (0..n).step_by(4) {
                let mut sums = [0.0f32; 4];
                let lanes = core::cmp::min(4, n - j);
                for t in 0..k {
                    let w_idx = t * n + j;
                    let inp = input.data[i * k + t];
                    for lane in 0..lanes {
                        match weight.get_weight(w_idx + lane) {
                            1 => sums[lane] += inp,
                            -1 => sums[lane] -= inp,
                            _ => {}
                        }
                    }
                }
                for lane in 0..lanes {
                    result.data[i * n + j + lane] = sums[lane];
                }
            }
        }
        return Some(result);
    }

    let r = scalar_ternary_matmul(weight, input, m, k, n);
    if r.is_valid() {
        Some(r)
    } else {
        None
    }
}

/// Alias legado (nome antigo mentia mul).
#[cfg(target_arch = "x86_64")]
pub(crate) unsafe fn sse2_ternary_matmul_skip_native(
    weight: &PackedTernaryTensor,
    input: &Tensor,
    m: usize,
    k: usize,
    n: usize,
) -> Tensor {
    sse2_ternary_matmul_add_sub_skip(weight, input, m, k, n)
}

/// Matmul ternário: por peso ADD/SUB/SKIP; acc em `_mm_add_ps` (4 lanes).
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
pub(crate) unsafe fn sse2_ternary_matmul_add_sub_skip(
    weight: &PackedTernaryTensor,
    input: &Tensor,
    m: usize,
    k: usize,
    n: usize,
) -> Tensor {
    use core::arch::x86_64::*;
    let mut result = Tensor::new((m, n));
    if !result.is_valid() || !input.is_valid() {
        return Tensor::zero((0, 0));
    }
    for i in 0..m {
        for j in (0..n).step_by(4) {
            let lanes = core::cmp::min(4, n - j);
            let mut acc = _mm_setzero_ps();
            let in_base = i * k;
            for t in 0..k {
                let x = input.data[in_base + t];
                let mut delta = [0.0f32; 4];
                let w_row = t * n + j;
                for lane in 0..lanes {
                    match weight.get_weight(w_row + lane) {
                        1 => delta[lane] = x,
                        -1 => delta[lane] = -x,
                        _ => {}
                    }
                }
                let dv = _mm_loadu_ps(delta.as_ptr());
                acc = _mm_add_ps(acc, dv);
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

fn scalar_ternary_matmul(weight: &PackedTernaryTensor, input: &Tensor, m: usize, k: usize, n: usize) -> Tensor {
    let mut result = Tensor::new((m, n));
    if !result.is_valid() || !input.is_valid() {
        return Tensor::zero((0, 0));
    }
    for i in 0..m {
        for j in 0..n {
            let mut sum = 0.0f32;
            for t in 0..k {
                match weight.get_weight(t * n + j) {
                    1 => sum += input.data[i * k + t],
                    -1 => sum -= input.data[i * k + t],
                    _ => {}
                }
            }
            result.data[i * n + j] = sum;
        }
    }
    result
}

#[allow(dead_code)]
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

    #[test]
    fn add_skip_sub_matches_scalar_semantics() {
        let w = PackedTernaryTensor {
            shape: (1, 3),
            packed_data: PackedTernaryTensor::pack_weights(&[1i8, 0, -1]),
        };
        let x = Tensor::from_row_major((1, 1), alloc::vec![2.5f32]).expect("x");
        let y = ternary_matmul(&w, &x).expect("matmul");
        assert_eq!(y.shape, (1, 3));
        assert!((y.data[0] - 2.5).abs() < 1e-6, "W=+1 ADD, got {}", y.data[0]);
        assert!(y.data[1].abs() < 1e-6, "W=0 SKIP, got {}", y.data[1]);
        assert!((y.data[2] + 2.5).abs() < 1e-6, "W=-1 SUB, got {}", y.data[2]);
    }

    #[test]
    fn sse2_add_sub_skip_parity_vs_scalar() {
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
        let simd = unsafe { super::sse2_ternary_matmul_add_sub_skip(&w, &x, m, k, n) };
        #[cfg(not(target_arch = "x86_64"))]
        let simd = scalar.clone();

        assert_eq!(scalar.shape, simd.shape);
        for (a, b) in scalar.data.iter().zip(simd.data.iter()) {
            assert!((a - b).abs() < 1e-5, "parity fail: scalar={a} simd={b}");
        }
    }

    /// Forma Falcon3-like (k grande, n%4): paridade SSE vs scalar.
    #[test]
    fn falcon3_shaped_parity_64x32() {
        let k = 64usize;
        let n = 32usize;
        let weights: alloc::vec::Vec<i8> = (0..k * n)
            .map(|i| match i % 5 {
                0 | 1 => 1i8,
                2 => 0,
                _ => -1,
            })
            .collect();
        let w = PackedTernaryTensor {
            shape: (k, n),
            packed_data: PackedTernaryTensor::pack_weights(&weights),
        };
        let x = Tensor::from_row_major(
            (1, k),
            (0..k).map(|i| (i as f32) * 0.01).collect(),
        )
        .expect("x");
        let scalar = super::scalar_ternary_matmul(&w, &x, 1, k, n);
        #[cfg(target_arch = "x86_64")]
        let simd = unsafe { super::sse2_ternary_matmul_add_sub_skip(&w, &x, 1, k, n) };
        #[cfg(not(target_arch = "x86_64"))]
        let simd = scalar.clone();
        for (a, b) in scalar.data.iter().zip(simd.data.iter()) {
            assert!((a - b).abs() < 1e-4, "falcon-shape parity {a} vs {b}");
        }
    }
}
