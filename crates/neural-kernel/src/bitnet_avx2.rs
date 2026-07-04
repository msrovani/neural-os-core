#![allow(dead_code)]
use crate::tensor::{PackedTernaryTensor, Tensor};
use alloc::vec;
use alloc::vec::Vec;

fn unpack_ternary_row(packed: &PackedTernaryTensor, row: usize, n: usize) -> Vec<i8> {
    let mut out = vec![0i8; n];
    let row_start = row * n;
    for j in 0..n {
        out[j] = packed.get_weight(row_start + j);
    }
    out
}

fn unpack_all(packed: &PackedTernaryTensor) -> Vec<i8> {
    let (k, n) = packed.shape;
    let mut out = vec![0i8; k * n];
    for i in 0..k * n {
        out[i] = packed.get_weight(i);
    }
    out
}

#[cfg(target_arch = "x86_64")]
fn avx2_available() -> bool {
    unsafe {
        let result = core::arch::x86_64::__cpuid(7);
        (result.ebx & (1 << 5)) != 0
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn avx2_available() -> bool { false }

pub fn ternary_matmul(weight: &PackedTernaryTensor, input: &Tensor) -> Option<Tensor> {
    let (k, n) = weight.shape;
    let (m, k2) = input.shape;
    if k != k2 { return None; }

    if avx2_available() && k >= 8 && n >= 8 {
        return Some(unsafe { avx2_ternary_matmul_impl(weight, input, m, k, n) });
    }

    scalar_ternary_matmul(weight, input, m, k, n)
}

fn scalar_ternary_matmul(weight: &PackedTernaryTensor, input: &Tensor, m: usize, k: usize, n: usize) -> Option<Tensor> {
    let mut result = Tensor::new((m, n));
    for i in 0..m {
        for j in 0..n {
            let mut sum = 0.0f32;
            for t in 0..k {
                let w = weight.get_weight(t * n + j);
                match w {
                    1 => sum += input.data[i * k + t],
                    -1 => sum -= input.data[i * k + t],
                    _ => {}
                }
            }
            result.data[i * n + j] = sum;
        }
    }
    Some(result)
}

#[cfg(target_arch = "x86_64")]
unsafe fn avx2_ternary_matmul_impl(weight: &PackedTernaryTensor, input: &Tensor, m: usize, k: usize, n: usize) -> Tensor {
    use core::arch::x86_64::*;

    let unpacked = unpack_all(weight);
    let mut result = Tensor::new((m, n));

    for i in 0..m {
        let inp_row = &input.data[i * k..];
        let out_row = &mut result.data[i * n..];

        for j in (0..n).step_by(8) {
            let remaining = core::cmp::min(8, n - j);
            let mut acc = _mm256_setzero_ps();

            for t in 0..k {
                let input_val = _mm256_set1_ps(inp_row[t]);

                let w_ptr = unpacked.as_ptr().add(t * n + j) as *const __m128i;
                let w_i8 = _mm_loadl_epi64(w_ptr);
                let w_i32 = _mm256_cvtepi8_epi32(w_i8);
                let w_f32 = _mm256_cvtepi32_ps(w_i32);

                acc = _mm256_fmadd_ps(input_val, w_f32, acc);
            }

            _mm256_storeu_ps(out_row.as_mut_ptr().add(j), acc);

            for r in 0..remaining {
                let mut sum = 0.0f32;
                for t in 0..k {
                    let w = unpacked[t * n + j + r] as i8;
                    match w {
                        1 => sum += inp_row[t],
                        -1 => sum -= inp_row[t],
                        _ => {}
                    }
                }
                out_row[j + r] += sum;
            }
        }
    }
    result
}
