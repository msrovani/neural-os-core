//! ADR-0061 / ADR-0101: BitNet ternary matmul SSE2.
//!
//! Contrato Onda 0: W∈{-1,0,+1} ⇒ ADD / SUB / SKIP da ativação — **sem** `W*x` mul.
//! Accumulação em XMM (`_mm_add_ps`). Soft-float: sem intrins XMM de load/store i8
//! (SESSION_336); pesos lidos via `get_weight` + delta f32.
//!
//! Canário: `sse2_sret_boot_canary()` (abaixo) é o guarda do bug sret soft-float
//! (SESSION_336/362) — host test `sse2_sret_boot_canary_passes_on_host` +
//! paridade SSE2↔scalar (`sse2_add_sub_skip_parity_vs_scalar`, formas
//! Falcon3-like em `falcon3_shaped_parity_64x32`) cobrem o caminho. O canário
//! host não prova o target soft-float — aceite de metal depende do boot canary.

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
        crate::matmul_diag::note_call(k, n, m, 0, false);
        return None;
    }

    if let Some(r) = crate::bitnet_avx512::ternary_matmul_avx512(weight, input) {
        if r.is_valid() {
            crate::matmul_diag::note_call(k, n, m, 1, true);
            return Some(r);
        }
        crate::matmul_diag::note_call(k, n, m, 2, false);
        crate::matmul_diag::note_avx512_invalid();
        return None;
    }

    // Bare-metal: SSE2 ADD/SUB/SKIP antes do stub AVX2 (ADR-0101 Onda 0).
    // s364: Tensor alocado FORA de #[target_feature] — sret soft-float
    // corrompia Vec/shape e #GP em memcpy no mesh 1G (pós mesh-skip).
    #[cfg(all(target_arch = "x86_64", target_os = "none"))]
    if n >= 4 {
        let mut result = Tensor::new((m, n));
        if !result.is_valid() || !input.is_valid() {
            crate::matmul_diag::note_sse2_zero_guard();
            return None;
        }
        unsafe {
            sse2_ternary_matmul_fill(weight, input, m, k, n, &mut result.data);
        }
        let r = Tensor {
            shape: (m, n),
            data: result.data,
        };
        crate::matmul_diag::note_sse2_ok();
        crate::matmul_diag::note_call(k, n, m, 3, r.is_valid());
        return Some(r);
    }

    // Host AVX2 (FMA dequant — bandwidth ≠ contrato lab; ok em testes)
    if k_nano::platform_probe::allow_avx2() && k >= 8 && n >= 8 && n % 4 == 0 {
        let r = unsafe { crate::bitnet_avx2::avx2_ternary_matmul_impl(weight, input, m, k, n) };
        crate::matmul_diag::note_call(k, n, m, 4, r.is_valid());
        return Some(r);
    }

    #[cfg(target_arch = "x86_64")]
    if n >= 4 {
        let r = unsafe { sse2_ternary_matmul_add_sub_skip(weight, input, m, k, n) };
        // Mesmo rebuild do path bare-metal (SESSION_336/362 sret soft-float).
        let r = Tensor {
            shape: (m, n),
            data: r.data,
        };
        crate::matmul_diag::note_call(k, n, m, 5, r.is_valid());
        return Some(r);
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
        crate::matmul_diag::note_call(k, n, m, 6, result.is_valid());
        return Some(result);
    }

    let r = scalar_ternary_matmul(weight, input, m, k, n);
    crate::matmul_diag::note_call(k, n, m, 7, r.is_valid());
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
/// Preenche `out` (len == m*n) — **não** retorna Tensor (sret soft-float #GP).
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
pub(crate) unsafe fn sse2_ternary_matmul_fill(
    weight: &PackedTernaryTensor,
    input: &Tensor,
    m: usize,
    k: usize,
    n: usize,
    out: &mut [f32],
) {
    use core::arch::x86_64::*;
    debug_assert_eq!(out.len(), m.saturating_mul(n));
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
            let mut tmp = [0.0f32; 4];
            _mm_storeu_ps(tmp.as_mut_ptr(), acc);
            for lane in 0..lanes {
                out[i * n + j + lane] = tmp[lane];
            }
        }
    }
}

/// Wrapper legado: aloca fora e chama fill (seguro p/ soft-float sret).
#[cfg(target_arch = "x86_64")]
pub(crate) unsafe fn sse2_ternary_matmul_add_sub_skip(
    weight: &PackedTernaryTensor,
    input: &Tensor,
    m: usize,
    k: usize,
    n: usize,
) -> Tensor {
    let mut result = Tensor::new((m, n));
    if !result.is_valid() || !input.is_valid() {
        crate::matmul_diag::note_sse2_zero_guard();
        k_nano::slog_cortex!(
            "MatmulDiag",
            "warn",
            "sse2 zero-guard: result.shape=({},{}) result.len={} valid={} input_valid={}",
            result.shape.0,
            result.shape.1,
            result.data.len(),
            result.is_valid() as u32,
            input.is_valid() as u32
        );
        return Tensor::zero((0, 0));
    }
    sse2_ternary_matmul_fill(weight, input, m, k, n, &mut result.data);
    crate::matmul_diag::note_kernel_shape(result.shape.0, result.shape.1, result.data.len());
    // Rebuild shape fora de qualquer target_feature residue.
    Tensor {
        shape: (m, n),
        data: result.data,
    }
}

/// Canário de boot (pré-K33[28] sgdb): matmul tiny via `ternary_matmul`.
///
/// Prova o rebuild de shape pós-SSE2 soft-float (SESSION_336/362) **sem**
/// ROUTER.BITNET, Tickv nem `classify_intent`. Aceite QEMU = slog
/// `sret canary PASS` + `shape=(1,4)`.
pub fn sse2_sret_boot_canary() -> bool {
    const M: usize = 1;
    const K: usize = 4;
    const N: usize = 4;
    let weights: [i8; 16] = [
        1, 0, -1, 1, //
        0, 1, 0, -1, //
        1, -1, 0, 1, //
        0, 0, 1, -1,
    ];
    let w = PackedTernaryTensor {
        shape: (K, N),
        packed_data: PackedTernaryTensor::pack_weights(&weights),
    };
    let Some(x) = Tensor::from_row_major((M, K), alloc::vec![1.0f32, 2.0, 3.0, 4.0]) else {
        k_nano::slog_cortex!("MatmulDiag", "fail", "sret canary: x alloc fail");
        return false;
    };
    let Some(y) = ternary_matmul(&w, &x) else {
        k_nano::slog_cortex!(
            "MatmulDiag",
            "fail",
            "sret canary: ternary_matmul None | {}",
            crate::matmul_diag::status_line()
        );
        return false;
    };
    let ok = y.is_valid() && y.shape == (M, N) && y.data.len() == M * N;
    if ok {
        k_nano::slog_cortex!(
            "MatmulDiag",
            "ok",
            "sret canary PASS shape=({},{}) len={} | {}",
            y.shape.0,
            y.shape.1,
            y.data.len(),
            crate::matmul_diag::status_line()
        );
    } else {
        k_nano::slog_cortex!(
            "MatmulDiag",
            "fail",
            "sret canary FAIL shape=({},{}) len={} valid={} | {}",
            y.shape.0,
            y.shape.1,
            y.data.len(),
            y.is_valid() as u32,
            crate::matmul_diag::status_line()
        );
    }
    ok
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

    #[test]
    fn sse2_sret_boot_canary_passes_on_host() {
        assert!(
            super::sse2_sret_boot_canary(),
            "tiny canary must pass on host (ABI nativo ≠ prova soft-float)"
        );
    }

    /// Lane D-accel: tail n%4 (lanes=min(4,n-j)) — ADD/SUB/SKIP exato vs scalar.
    #[test]
    fn sse_tail_n10_parity_vs_scalar() {
        let k = 48usize;
        let n = 10usize;
        let m = 2usize;
        let weights: alloc::vec::Vec<i8> = (0..k * n)
            .map(|i| match i % 3 {
                0 => 1i8,
                1 => 0,
                _ => -1,
            })
            .collect();
        let w = PackedTernaryTensor {
            shape: (k, n),
            packed_data: PackedTernaryTensor::pack_weights(&weights),
        };
        let x = Tensor::from_row_major(
            (m, k),
            (0..m * k).map(|i| (i as f32 + 1.0) * 0.125).collect(),
        )
        .expect("x");
        let scalar = super::scalar_ternary_matmul(&w, &x, m, k, n);
        #[cfg(target_arch = "x86_64")]
        let simd = unsafe { super::sse2_ternary_matmul_add_sub_skip(&w, &x, m, k, n) };
        #[cfg(not(target_arch = "x86_64"))]
        let simd = scalar.clone();
        assert_eq!(scalar.shape, (m, n));
        assert_eq!(simd.shape, (m, n));
        for (a, b) in scalar.data.iter().zip(simd.data.iter()) {
            assert!((a - b).abs() < 1e-5, "tail parity {a} vs {b}");
        }
    }
}
