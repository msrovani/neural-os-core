#![allow(dead_code)]
use crate::tensor::{PackedTernaryTensor, Tensor};
use alloc::vec;

/// Unpack one row of a PackedTernaryTensor into an i8 buffer.
/// Packed format: byte = [elem0(bits 0-1), elem1(bits 2-3), elem2(bits 4-5), elem3(bits 6-7)]
/// Decode: 0b00→0, 0b01→1, 0b10→-1
fn unpack_row_into(packed: &PackedTernaryTensor, row: usize, n: usize, buf: &mut [i8]) {
    let packed_row_words = n.div_ceil(4);
    let row_start = row * packed_row_words;
    for pw in 0..packed_row_words {
        let p = packed.packed_data[row_start + pw];
        let base = pw * 4;
        // elem0 = bits 0-1
        if base < n { buf[base] = match p & 3 { 0 => 0, 1 => 1, _ => -1 }; }
        // elem1 = bits 2-3
        if base + 1 < n { buf[base + 1] = match (p >> 2) & 3 { 0 => 0, 1 => 1, _ => -1 }; }
        // elem2 = bits 4-5
        if base + 2 < n { buf[base + 2] = match (p >> 4) & 3 { 0 => 0, 1 => 1, _ => -1 }; }
        // elem3 = bits 6-7
        if base + 3 < n { buf[base + 3] = match (p >> 6) & 3 { 0 => 0, 1 => 1, _ => -1 }; }
    }
}

#[cfg(target_arch = "x86_64")]
fn avx2_available() -> bool {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        // CPUID leaf 1, ECX bit 31 = hypervisor present
        let leaf1 = core::arch::x86_64::__cpuid(1);
        let has_hypervisor = (leaf1.ecx & (1 << 31)) != 0;

        // CPUID leaf 7, EBX bit 5 = AVX2
        let leaf7 = core::arch::x86_64::__cpuid(7);
        let has_avx2 = (leaf7.ebx & (1 << 5)) != 0;
        if !has_avx2 { return false; }

        if has_hypervisor {
            // So bloqueia AVX2 se hypervisor = TCG (QEMU sem accel)
            // TCG emula cada VEX como VM exit (~10k ciclos)
            // WHPX/KVM com -cpu host executa AVX2 nativo
            let hv = core::arch::x86_64::__cpuid(0x40000000);
            let vendor: [u8; 12] = [
                (hv.ebx >> 0) as u8, (hv.ebx >> 8) as u8, (hv.ebx >> 16) as u8, (hv.ebx >> 24) as u8,
                (hv.ecx >> 0) as u8, (hv.ecx >> 8) as u8, (hv.ecx >> 16) as u8, (hv.ecx >> 24) as u8,
                (hv.edx >> 0) as u8, (hv.edx >> 8) as u8, (hv.edx >> 16) as u8, (hv.edx >> 24) as u8,
            ];
            if &vendor[..9] == b"TCGTCGTCG" {
                return false;
            }
        }

        true
    }
    #[cfg(not(target_arch = "x86_64"))]
    { false }
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

    if k % 8 != 0 || n % 8 != 0 {
        return scalar_ternary_matmul(weight, input, m, k, n).unwrap();
    }

    let mut result = Tensor::new((m, n));
    // unpack one row at a time into a reusable buffer (n bytes, not k*n)
    let mut row_buf = vec![0i8; n];

    for i in 0..m {
        let inp_row = &input.data[i * k..];
        let out_row = &mut result.data[i * n..];

        // zero out output row
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
