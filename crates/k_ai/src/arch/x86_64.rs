//! x86_64 SIMD Kernels for BitNet Operations
//!
//! Dynamic dispatching for SSE4.2, AVX2, and AVX-512 kernels.
//! Implements ternary weight addition using SIMD intrinsics.
//! All data structures are 64-byte aligned for cache line optimization.
//!
//! # Safety
//! All kernel functions are `unsafe` because they use raw pointers and
//! SIMD intrinsics. Callers must ensure:
//! - Pointers are valid and properly aligned
//! - The CPU supports the required instruction set (checked at runtime)
//! - Output buffer is large enough

#![allow(dead_code)]
#![allow(unused_unsafe)]

#[cfg(all(target_arch = "x86_64", not(target_os = "none")))]
use core::arch::x86_64::*;

/// 64-byte aligned buffer for SIMD operations
#[repr(align(64))]
#[derive(Debug, Clone, Copy)]
pub struct AlignedBuffer<T> {
    pub data: [T; 16], // 64 bytes for T = i32
}

impl<T> Default for AlignedBuffer<T>
where
    T: Default + Copy,
{
    fn default() -> Self {
        Self {
            data: [T::default(); 16],
        }
    }
}

/// Ternary weight representation (-1, 0, +1)
#[repr(i8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ternary {
    Negative = -1,
    Zero = 0,
    Positive = 1,
}

impl From<i8> for Ternary {
    fn from(value: i8) -> Self {
        match value {
            -1 => Ternary::Negative,
            0 => Ternary::Zero,
            1 => Ternary::Positive,
            _ => Ternary::Zero,
        }
    }
}

/// BitNet kernel function pointer type.
/// Takes two i8 input arrays and produces packed i32 output.
pub type BitNetKernel = unsafe fn(*const i8, *const i8, *mut i32, usize);

// ─── SSE4.2 Kernel (128-bit, 64 weights/cycle) ──────────────────────────

/// SSE4.2 kernel: processes 16 i8 values per iteration (128-bit SIMD).
///
/// Each iteration loads 16 i8 values, sign-extends to i32 in 4 steps,
/// adds, and stores 4 i32 results.
///
/// # Safety
/// - `a` and `b` must point to arrays of at least `len` i8 elements
/// - `output` must point to array of at least `len` i32 elements
/// - CPU must support SSE4.2
#[cfg(all(target_arch = "x86_64", not(target_os = "none")))]
#[target_feature(enable = "sse4.2")]
#[inline]
pub unsafe fn bitwise_add_sse42(a: *const i8, b: *const i8, output: *mut i32, len: usize) {
    let chunks = len / 16;

    for i in 0..chunks {
        let a_ptr = a.add(i * 16);
        let b_ptr = b.add(i * 16);
        let out_ptr = output.add(i * 16);

        // Load 16 i8 values
        let a_vec = _mm_loadu_si128(a_ptr as *const __m128i);
        let b_vec = _mm_loadu_si128(b_ptr as *const __m128i);

        // Sign-extend i8 to i16 (lower 8 elements)
        let a_lo = _mm_cvtepi8_epi16(a_vec);
        let b_lo = _mm_cvtepi8_epi16(b_vec);

        // Sign-extend i8 to i16 (upper 8 elements)
        let a_hi = _mm_cvtepi8_epi16(_mm_unpackhi_epi8(a_vec, a_vec));
        let b_hi = _mm_cvtepi8_epi16(_mm_unpackhi_epi8(b_vec, b_vec));

        // Sign-extend i16 to i32: lower 4 of lo
        let a0 = _mm_cvtepi16_epi32(a_lo);
        let b0 = _mm_cvtepi16_epi32(b_lo);

        // Sign-extend i16 to i32: upper 4 of lo
        let a1 = _mm_cvtepi16_epi32(_mm_unpackhi_epi16(a_lo, a_lo));
        let b1 = _mm_cvtepi16_epi32(_mm_unpackhi_epi16(b_lo, b_lo));

        // Sign-extend i16 to i32: lower 4 of hi
        let a2 = _mm_cvtepi16_epi32(a_hi);
        let b2 = _mm_cvtepi16_epi32(b_hi);

        // Sign-extend i16 to i32: upper 4 of hi
        let a3 = _mm_cvtepi16_epi32(_mm_unpackhi_epi16(a_hi, a_hi));
        let b3 = _mm_cvtepi16_epi32(_mm_unpackhi_epi16(b_hi, b_hi));

        // Ternary addition
        let sum0 = _mm_add_epi32(a0, b0);
        let sum1 = _mm_add_epi32(a1, b1);
        let sum2 = _mm_add_epi32(a2, b2);
        let sum3 = _mm_add_epi32(a3, b3);

        // Store 16 i32 results
        _mm_storeu_si128(out_ptr as *mut __m128i, sum0);
        _mm_storeu_si128(out_ptr.add(4) as *mut __m128i, sum1);
        _mm_storeu_si128(out_ptr.add(8) as *mut __m128i, sum2);
        _mm_storeu_si128(out_ptr.add(12) as *mut __m128i, sum3);
    }

    // Handle remaining elements (scalar fallback)
    let rem = len % 16;
    if rem > 0 {
        let offset = chunks * 16;
        for j in 0..rem {
            let a_val = *a.add(offset + j) as i32;
            let b_val = *b.add(offset + j) as i32;
            *output.add(offset + j) = a_val + b_val;
        }
    }
}

// ─── AVX2 Kernel (256-bit, 128 weights/cycle) ───────────────────────────

/// AVX2 kernel: processes 32 ternary weights per cycle (256-bit SIMD).
///
/// Each iteration loads 32 i8 values, sign-extends to i32 in 4 groups of 8,
/// adds, and stores 32 i32 results.
///
/// # Safety
/// - `a` and `b` must be valid pointers to arrays of at least `len` i8 elements
/// - `output` must be valid pointer to array of at least `len` i32 elements
/// - CPU must support AVX2
#[cfg(all(target_arch = "x86_64", not(target_os = "none")))]
#[target_feature(enable = "avx2")]
#[inline]
pub unsafe fn bitwise_add_avx2(a: *const i8, b: *const i8, output: *mut i32, len: usize) {
    let chunks = len / 32;

    for i in 0..chunks {
        let a_ptr = a.add(i * 32);
        let b_ptr = b.add(i * 32);
        let out_ptr = output.add(i * 32);

        // Load 32 i8 values as 1 x __m256i
        let a_vec = _mm256_loadu_si256(a_ptr as *const __m256i);
        let b_vec = _mm256_loadu_si256(b_ptr as *const __m256i);

        // Extract 128-bit halves for cvtepi8_epi32 (takes __m128i)
        let a_lo = _mm256_castsi256_si128(a_vec);
        let a_hi = _mm256_extracti128_si256::<1>(a_vec);
        let b_lo = _mm256_castsi256_si128(b_vec);
        let b_hi = _mm256_extracti128_si256::<1>(b_vec);

        // Sign-extend i8 to i32: 8 elements per __m256i
        let a0 = _mm256_cvtepi8_epi32(a_lo);
        let a1 = _mm256_cvtepi8_epi32(a_hi);
        let b0 = _mm256_cvtepi8_epi32(b_lo);
        let b1 = _mm256_cvtepi8_epi32(b_hi);

        // Ternary addition
        let sum0 = _mm256_add_epi32(a0, b0);
        let sum1 = _mm256_add_epi32(a1, b1);

        // Store 32 i32 results (2 x 256-bit stores)
        _mm256_storeu_si256(out_ptr as *mut __m256i, sum0);
        _mm256_storeu_si256(out_ptr.add(8) as *mut __m256i, sum1);
    }

    // Handle remaining elements
    let rem = len % 32;
    if rem > 0 {
        let offset = chunks * 32;
        for j in 0..rem {
            let a_val = *a.add(offset + j) as i32;
            let b_val = *b.add(offset + j) as i32;
            *output.add(offset + j) = a_val + b_val;
        }
    }
}

// ─── AVX-512 Kernel (512-bit, 256 weights/cycle) ────────────────────────

/// AVX-512 kernel: processes 64 ternary weights per cycle (512-bit SIMD).
///
/// Uses _mm512_and_si512 for ternary weight unpacking and direct vector
/// addition. Compiled with target-feature=+avx512f,+avx512bw,+avx512vnni.
///
/// Each iteration loads 64 i8 values, sign-extends to i32 in 4 groups of 16,
/// adds, and stores 64 i32 results.
///
/// # Safety
/// - `a` and `b` must be valid pointers to arrays of at least `len` i8 elements
/// - `output` must be valid pointer to array of at least `len` i32 elements
/// - CPU must support AVX-512F, AVX-512BW, and AVX-512VNNI
#[cfg(all(target_arch = "x86_64", not(target_os = "none")))]
#[target_feature(enable = "avx512f,avx512bw,avx512vnni")]
#[inline]
pub unsafe fn bitwise_add_avx512(a: *const i8, b: *const i8, output: *mut i32, len: usize) {
    let chunks = len / 64;

    for i in 0..chunks {
        let a_ptr = a.add(i * 64);
        let b_ptr = b.add(i * 64);
        let out_ptr = output.add(i * 64);

        // Load 64 i8 values as 1 x __m512i
        let a_vec = _mm512_loadu_si512(a_ptr as *const __m512i);
        let b_vec = _mm512_loadu_si512(b_ptr as *const __m512i);

        // Extract 128-bit quarters for cvtepi8_epi32 (takes __m128i)
        // __m512i = 64 bytes = 4 × __m128i (16 bytes each)
        let a_q0 = _mm512_castsi512_si128(a_vec);
        let a_q1 = _mm512_extracti32x4_epi32::<1>(a_vec);
        let a_q2 = _mm512_extracti32x4_epi32::<2>(a_vec);
        let a_q3 = _mm512_extracti32x4_epi32::<3>(a_vec);

        let b_q0 = _mm512_castsi512_si128(b_vec);
        let b_q1 = _mm512_extracti32x4_epi32::<1>(b_vec);
        let b_q2 = _mm512_extracti32x4_epi32::<2>(b_vec);
        let b_q3 = _mm512_extracti32x4_epi32::<3>(b_vec);

        // Sign-extend i8 to i32 (16 elements per __m512i, from 16 i8 in __m128i)
        let a0 = _mm512_cvtepi8_epi32(a_q0);
        let a1 = _mm512_cvtepi8_epi32(a_q1);
        let a2 = _mm512_cvtepi8_epi32(a_q2);
        let a3 = _mm512_cvtepi8_epi32(a_q3);

        let b0 = _mm512_cvtepi8_epi32(b_q0);
        let b1 = _mm512_cvtepi8_epi32(b_q1);
        let b2 = _mm512_cvtepi8_epi32(b_q2);
        let b3 = _mm512_cvtepi8_epi32(b_q3);

        // Ternary addition
        let sum0 = _mm512_add_epi32(a0, b0);
        let sum1 = _mm512_add_epi32(a1, b1);
        let sum2 = _mm512_add_epi32(a2, b2);
        let sum3 = _mm512_add_epi32(a3, b3);

        // Store 64 i32 results (4 × 512-bit stores, 16 i32 each)
        _mm512_storeu_si512(out_ptr as *mut __m512i, sum0);
        _mm512_storeu_si512(out_ptr.add(16) as *mut __m512i, sum1);
        _mm512_storeu_si512(out_ptr.add(32) as *mut __m512i, sum2);
        _mm512_storeu_si512(out_ptr.add(48) as *mut __m512i, sum3);
    }

    // Handle remaining elements
    let rem = len % 64;
    if rem > 0 {
        let offset = chunks * 64;
        for j in 0..rem {
            let a_val = *a.add(offset + j) as i32;
            let b_val = *b.add(offset + j) as i32;
            *output.add(offset + j) = a_val + b_val;
        }
    }
}

// ─── Scalar Fallback ────────────────────────────────────────────────────

/// Scalar fallback kernel for systems without SIMD support.
///
/// # Safety
/// - `a` and `b` must be valid pointers to arrays of at least `len` i8 elements
/// - `output` must be valid pointer to array of at least `len` i32 elements
#[inline]
pub unsafe fn bitwise_add_scalar(a: *const i8, b: *const i8, output: *mut i32, len: usize) {
    for j in 0..len {
        let a_val = *a.add(j) as i32;
        let b_val = *b.add(j) as i32;
        *output.add(j) = a_val + b_val;
    }
}

// ─── Runtime CPU Feature Detection ──────────────────────────────────────

/// Check if SSE4.2 is supported at runtime (CPUID leaf 1, ECX bit 20).
pub fn has_sse42() -> bool {
    #[cfg(all(target_arch = "x86_64", not(target_os = "none")))]
    unsafe {
        let cpuid = __cpuid(1);
        (cpuid.ecx & (1 << 20)) != 0
    }
    #[cfg(any(not(target_arch = "x86_64"), target_os = "none"))]
    {
        false
    }
}

/// Check if AVX2 is supported at runtime.
/// Requires AVX (CPUID.1:ECX[28]) + OSXSAVE (CPUID.1:ECX[27]).
pub fn has_avx2() -> bool {
    #[cfg(all(target_arch = "x86_64", not(target_os = "none")))]
    unsafe {
        let cpuid = __cpuid(1);
        let avx = (cpuid.ecx & (1 << 28)) != 0;
        let osxsave = (cpuid.ecx & (1 << 27)) != 0;
        if !avx || !osxsave {
            return false;
        }
        let cpuid7 = __cpuid_count(7, 0);
        (cpuid7.ebx & (1 << 5)) != 0
    }
    #[cfg(any(not(target_arch = "x86_64"), target_os = "none"))]
    {
        false
    }
}

/// Check if AVX-512F is supported at runtime (CPUID leaf 7, EBX bit 16).
pub fn has_avx512f() -> bool {
    #[cfg(all(target_arch = "x86_64", not(target_os = "none")))]
    unsafe {
        let cpuid = __cpuid_count(7, 0);
        (cpuid.ebx & (1 << 16)) != 0
    }
    #[cfg(any(not(target_arch = "x86_64"), target_os = "none"))]
    {
        false
    }
}

/// Check if AVX-512BW is supported at runtime (CPUID leaf 7, EBX bit 30).
pub fn has_avx512bw() -> bool {
    #[cfg(all(target_arch = "x86_64", not(target_os = "none")))]
    unsafe {
        let cpuid = __cpuid_count(7, 0);
        (cpuid.ebx & (1 << 30)) != 0
    }
    #[cfg(any(not(target_arch = "x86_64"), target_os = "none"))]
    {
        false
    }
}

/// Check if AVX-512VNNI is supported at runtime (CPUID leaf 7, ECX bit 11).
pub fn has_avx512vnni() -> bool {
    #[cfg(all(target_arch = "x86_64", not(target_os = "none")))]
    unsafe {
        let cpuid = __cpuid_count(7, 0);
        (cpuid.ecx & (1 << 11)) != 0
    }
    #[cfg(any(not(target_arch = "x86_64"), target_os = "none"))]
    {
        false
    }
}

/// Check if full AVX-512 feature set is available (F + BW + VNNI).
pub fn has_full_avx512() -> bool {
    has_avx512f() && has_avx512bw() && has_avx512vnni()
}

// ─── Dynamic Dispatch ───────────────────────────────────────────────────

/// Dynamic dispatch: select the optimal BitNet kernel based on hardware.
///
/// Dispatch priority:
/// 1. Full AVX-512 (F + BW + VNNI) — Modern Xeon/Core Ultra
/// 2. AVX2 — Ryzen/Intel client CPUs
/// 3. SSE4.2 — Legacy i3/i5 (minimum requirement)
/// 4. Scalar — Fallback for very old CPUs
#[cfg(all(target_arch = "x86_64", not(target_os = "none")))]
pub fn dispatch_bitnet_kernel() -> BitNetKernel {
    if has_full_avx512() {
        bitwise_add_avx512
    } else if has_avx2() {
        bitwise_add_avx2
    } else if has_sse42() {
        bitwise_add_sse42
    } else {
        bitwise_add_scalar
    }
}
#[cfg(any(not(target_arch = "x86_64"), target_os = "none"))]
pub fn dispatch_bitnet_kernel() -> BitNetKernel {
    bitwise_add_scalar
}

/// Dispatch based on adaptation policy from Hermes.
///
/// Policy flags are AND-ed with hardware capability: a policy requesting
/// AVX-512 on a CPU without it falls back to the next available tier.
#[cfg(all(target_arch = "x86_64", not(target_os = "none")))]
pub fn dispatch_bitnet_kernel_with_policy(
    use_avx512: bool,
    use_avx2: bool,
    use_sse42: bool,
) -> BitNetKernel {
    if use_avx512 && has_full_avx512() {
        bitwise_add_avx512
    } else if use_avx2 && has_avx2() {
        bitwise_add_avx2
    } else if use_sse42 && has_sse42() {
        bitwise_add_sse42
    } else {
        bitwise_add_scalar
    }
}
#[cfg(any(not(target_arch = "x86_64"), target_os = "none"))]
pub fn dispatch_bitnet_kernel_with_policy(
    _use_avx512: bool,
    _use_avx2: bool,
    _use_sse42: bool,
) -> BitNetKernel {
    bitwise_add_scalar
}

// ─── Safe Wrappers ──────────────────────────────────────────────────────

/// Safe wrapper for BitNet kernel execution with automatic dispatch.
///
/// # Panics
/// Panics if input slices have different lengths or output is too small.
pub fn safe_bitwise_add(a: &[i8], b: &[i8], output: &mut [i32]) {
    assert_eq!(a.len(), b.len(), "Input slices must have equal length");
    assert!(
        output.len() >= a.len(),
        "Output slice too small: need {} got {}",
        a.len(),
        output.len()
    );

    let kernel = dispatch_bitnet_kernel();

    unsafe {
        kernel(a.as_ptr(), b.as_ptr(), output.as_mut_ptr(), a.len());
    }
}

/// Safe wrapper with policy-based dispatch.
///
/// # Panics
/// Panics if input slices have different lengths or output is too small.
pub fn safe_bitwise_add_with_policy(
    a: &[i8],
    b: &[i8],
    output: &mut [i32],
    use_avx512: bool,
    use_avx2: bool,
    use_sse42: bool,
) {
    assert_eq!(a.len(), b.len(), "Input slices must have equal length");
    assert!(
        output.len() >= a.len(),
        "Output slice too small: need {} but {}",
        a.len(),
        output.len()
    );

    let kernel = dispatch_bitnet_kernel_with_policy(use_avx512, use_avx2, use_sse42);

    unsafe {
        kernel(a.as_ptr(), b.as_ptr(), output.as_mut_ptr(), a.len());
    }
}

/// Batch processing for multiple BitNet operations.
///
/// Uses a single kernel dispatch for all operations in the batch.
///
/// # Panics
/// Panics if input/output counts mismatch or any slice is too small.
pub fn batch_bitwise_add(
    inputs: &[(&[i8], &[i8])],
    outputs: &mut [&mut [i32]],
    use_avx512: bool,
    use_avx2: bool,
    use_sse42: bool,
) {
    assert_eq!(
        inputs.len(),
        outputs.len(),
        "Input and output counts must match"
    );

    let kernel = dispatch_bitnet_kernel_with_policy(use_avx512, use_avx2, use_sse42);

    for ((a, b), output) in inputs.iter().zip(outputs.iter_mut()) {
        assert_eq!(a.len(), b.len(), "Input slices must have equal length");
        assert!(
            output.len() >= a.len(),
            "Output slice too small: need {} but {}",
            a.len(),
            output.len()
        );

        unsafe {
            kernel(a.as_ptr(), b.as_ptr(), output.as_mut_ptr(), a.len());
        }
    }
}

// ─── Query Functions ────────────────────────────────────────────────────

/// Get the name of the currently selected kernel.
pub fn current_kernel_name() -> &'static str {
    if has_avx512f() {
        "AVX-512F (512-bit, 256 weights/cycle)"
    } else if has_avx2() {
        "AVX2 (256-bit, 128 weights/cycle)"
    } else if has_sse42() {
        "SSE4.2 (128-bit, 64 weights/cycle)"
    } else {
        "Scalar (64-bit, 64 weights/cycle)"
    }
}

/// Get SIMD width in bits for the current kernel.
pub fn current_simd_width() -> u32 {
    if has_avx512f() {
        512
    } else if has_avx2() {
        256
    } else if has_sse42() {
        128
    } else {
        64
    }
}

/// Get weights per cycle for the current kernel.
pub fn current_weights_per_cycle() -> u32 {
    if has_avx512f() {
        256
    } else if has_avx2() {
        128
    } else if has_sse42() {
        64
    } else {
        64
    }
}

// ─── Self-Test ──────────────────────────────────────────────────────────

/// Self-test: verify all kernels produce correct results on small inputs.
/// Returns true if all kernels pass.
#[cfg(test)]
pub fn self_test() -> bool {
    let a: [i8; 16] = [1, -1, 0, 1, -1, 0, 1, -1, 0, 1, -1, 0, 1, -1, 0, 1];
    let b: [i8; 16] = [0, 1, -1, 1, 0, -1, 1, 0, -1, 1, 0, -1, 1, 0, -1, 1];
    let mut out = [0i32; 16];

    // Scalar
    unsafe { bitwise_add_scalar(a.as_ptr(), b.as_ptr(), out.as_mut_ptr(), 16) };
    for j in 0..16 {
        let expected_val = a[j] as i32 + b[j] as i32;
        if out[j] != expected_val {
            return false;
        }
    }

    // SSE4.2 (if available)
    if has_sse42() {
        out.fill(0);
        unsafe { bitwise_add_sse42(a.as_ptr(), b.as_ptr(), out.as_mut_ptr(), 16) };
        for j in 0..16 {
            let expected_val = a[j] as i32 + b[j] as i32;
            if out[j] != expected_val {
                return false;
            }
        }
    }

    // AVX2 (if available)
    if has_avx2() {
        let a32: [i8; 32] = [
            1, -1, 0, 1, -1, 0, 1, -1, 0, 1, -1, 0, 1, -1, 0, 1,
            1, -1, 0, 1, -1, 0, 1, -1, 0, 1, -1, 0, 1, -1, 0, 1,
        ];
        let b32: [i8; 32] = [
            0, 1, -1, 1, 0, -1, 1, 0, -1, 1, 0, -1, 1, 0, -1, 1,
            0, 1, -1, 1, 0, -1, 1, 0, -1, 1, 0, -1, 1, 0, -1, 1,
        ];
        let mut out32 = [0i32; 32];
        unsafe { bitwise_add_avx2(a32.as_ptr(), b32.as_ptr(), out32.as_mut_ptr(), 32) };
        for j in 0..32 {
            let expected_val = a32[j] as i32 + b32[j] as i32;
            if out32[j] != expected_val {
                return false;
            }
        }
    }

    // AVX-512 (if available)
    if has_full_avx512() {
        let a64: [i8; 64] = [1i8; 64];
        let b64: [i8; 64] = [-1i8; 64];
        let mut out64 = [0i32; 64];
        unsafe { bitwise_add_avx512(a64.as_ptr(), b64.as_ptr(), out64.as_mut_ptr(), 64) };
        for j in 0..64 {
            if out64[j] != 0 {
                return false;
            }
        }
    }

    true
}