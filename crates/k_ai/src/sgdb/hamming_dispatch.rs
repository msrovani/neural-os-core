//! ADR-0063 D1 — Despachante Hamming: scalar | AVX2 LUT | AVX-512.
//! Seleção única no boot via `platform_probe` (TCG → scalar).

use core::sync::atomic::{AtomicU8, Ordering};

const PATH_SCALAR: u8 = 0;
const PATH_AVX2_LUT: u8 = 1;
const PATH_AVX512: u8 = 2;

static HAMMING_PATH: AtomicU8 = AtomicU8::new(PATH_SCALAR);

pub type HammingFn = fn(&[u64], &[u64]) -> u32;

/// Chamado no boot (após platform_probe).
pub fn select_best_hamming_kernel() {
    #[cfg(target_arch = "x86_64")]
    {
        if k_nano::platform_probe::allow_avx512() {
            HAMMING_PATH.store(PATH_AVX512, Ordering::Relaxed);
            return;
        }
        if k_nano::platform_probe::allow_avx2() {
            HAMMING_PATH.store(PATH_AVX2_LUT, Ordering::Relaxed);
            return;
        }
    }
    HAMMING_PATH.store(PATH_SCALAR, Ordering::Relaxed);
}

pub fn path_name() -> &'static str {
    match HAMMING_PATH.load(Ordering::Relaxed) {
        PATH_AVX512 => "avx512",
        PATH_AVX2_LUT => "avx2_lut",
        _ => "scalar",
    }
}

pub fn active_kernel() -> HammingFn {
    match HAMMING_PATH.load(Ordering::Relaxed) {
        PATH_AVX512 => hamming_avx512_or_fallback,
        PATH_AVX2_LUT => hamming_avx2_lut_or_fallback,
        _ => hamming_scalar,
    }
}

#[inline]
pub fn hamming(a: &[u64], b: &[u64]) -> u32 {
    active_kernel()(a, b)
}

/// 1024-dim = 16×u64 — caminho hot L4/L5.
#[inline]
pub fn hamming_1024(v1: &[u64; 16], v2: &[u64; 16]) -> u32 {
    hamming(v1.as_slice(), v2.as_slice())
}

pub fn hamming_scalar(a: &[u64], b: &[u64]) -> u32 {
    let n = a.len().min(b.len());
    let mut d = 0u32;
    for i in 0..n {
        d += (a[i] ^ b[i]).count_ones();
    }
    let longer = if a.len() > b.len() { a } else { b };
    for j in n..longer.len() {
        d += longer[j].count_ones();
    }
    d
}

fn hamming_avx2_lut_or_fallback(a: &[u64], b: &[u64]) -> u32 {
    #[cfg(target_arch = "x86_64")]
    {
        if k_nano::platform_probe::allow_avx2() {
            return unsafe { hamming_avx2_lut(a, b) };
        }
    }
    hamming_scalar(a, b)
}

fn hamming_avx512_or_fallback(a: &[u64], b: &[u64]) -> u32 {
    #[cfg(target_arch = "x86_64")]
    {
        if k_nano::platform_probe::allow_avx512() {
            return unsafe { hamming_avx512(a, b) };
        }
        if k_nano::platform_probe::allow_avx2() {
            return unsafe { hamming_avx2_lut(a, b) };
        }
    }
    hamming_scalar(a, b)
}

/// AVX2: POPCNT via LUT nibble + VPSHUFB (sem POPCNT vetorial nativo).
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn hamming_avx2_lut(a: &[u64], b: &[u64]) -> u32 {
    use core::arch::x86_64::*;
    // LUT: popcount de nibble 0..15
    let lut = _mm256_setr_epi8(
        0, 1, 1, 2, 1, 2, 2, 3, 1, 2, 2, 3, 2, 3, 3, 4, 0, 1, 1, 2, 1, 2, 2, 3, 1, 2, 2, 3, 2, 3,
        3, 4,
    );
    let low_mask = _mm256_set1_epi8(0x0f);
    let n = a.len().min(b.len());
    let mut d = 0u32;
    let mut i = 0;
    // 4×u64 = 32 bytes = 1 YMM
    while i + 4 <= n {
        let va = _mm256_loadu_si256(a.as_ptr().add(i) as *const __m256i);
        let vb = _mm256_loadu_si256(b.as_ptr().add(i) as *const __m256i);
        let x = _mm256_xor_si256(va, vb);
        let lo = _mm256_and_si256(x, low_mask);
        let hi = _mm256_and_si256(_mm256_srli_epi16(x, 4), low_mask);
        let plo = _mm256_shuffle_epi8(lut, lo);
        let phi = _mm256_shuffle_epi8(lut, hi);
        let sum = _mm256_add_epi8(plo, phi);
        // horizontal sum bytes → u32 via sad
        let sad = _mm256_sad_epu8(sum, _mm256_setzero_si256());
        let mut tmp = [0u64; 4];
        _mm256_storeu_si256(tmp.as_mut_ptr() as *mut __m256i, sad);
        d += (tmp[0] + tmp[1] + tmp[2] + tmp[3]) as u32;
        i += 4;
    }
    while i < n {
        d += (a[i] ^ b[i]).count_ones();
        i += 1;
    }
    let longer = if a.len() > b.len() { a } else { b };
    for j in n..longer.len() {
        d += longer[j].count_ones();
    }
    d
}

/// AVX-512: VPOPCNTDQ se CPUID leaf7 ECX.14; senão XOR+ZMM + count_ones.
#[cfg(target_arch = "x86_64")]
unsafe fn hamming_avx512(a: &[u64], b: &[u64]) -> u32 {
    let leaf7 = core::arch::x86_64::__cpuid_count(7, 0);
    let has_vpop = (leaf7.ecx & (1 << 14)) != 0;
    if has_vpop {
        hamming_avx512_vpopcnt(a, b)
    } else {
        hamming_avx512_xor(a, b)
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f", enable = "avx512vpopcntdq")]
unsafe fn hamming_avx512_vpopcnt(a: &[u64], b: &[u64]) -> u32 {
    use core::arch::x86_64::*;
    let n = a.len().min(b.len());
    let mut d = 0u32;
    let mut i = 0;
    while i + 8 <= n {
        let va = _mm512_loadu_si512(a.as_ptr().add(i) as *const _);
        let vb = _mm512_loadu_si512(b.as_ptr().add(i) as *const _);
        let x = _mm512_xor_si512(va, vb);
        let pc = _mm512_popcnt_epi64(x);
        let mut tmp = [0u64; 8];
        _mm512_storeu_si512(tmp.as_mut_ptr() as *mut _, pc);
        for t in &tmp {
            d += *t as u32;
        }
        i += 8;
    }
    while i < n {
        d += (a[i] ^ b[i]).count_ones();
        i += 1;
    }
    let longer = if a.len() > b.len() { a } else { b };
    for j in n..longer.len() {
        d += longer[j].count_ones();
    }
    d
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
unsafe fn hamming_avx512_xor(a: &[u64], b: &[u64]) -> u32 {
    use core::arch::x86_64::*;
    let n = a.len().min(b.len());
    let mut d = 0u32;
    let mut i = 0;
    while i + 8 <= n {
        let va = _mm512_loadu_si512(a.as_ptr().add(i) as *const _);
        let vb = _mm512_loadu_si512(b.as_ptr().add(i) as *const _);
        let x = _mm512_xor_si512(va, vb);
        let mut tmp = [0u64; 8];
        _mm512_storeu_si512(tmp.as_mut_ptr() as *mut _, x);
        for t in &tmp {
            d += t.count_ones();
        }
        i += 8;
    }
    while i < n {
        d += (a[i] ^ b[i]).count_ones();
        i += 1;
    }
    let longer = if a.len() > b.len() { a } else { b };
    for j in n..longer.len() {
        d += longer[j].count_ones();
    }
    d
}

/// Smoke: 1024-dim (16 words) top-1 idêntico scalar vs kernel ativo.
pub fn smoke_1024() -> bool {
    select_best_hamming_kernel();
    let mut a = [0u64; 16];
    let mut b = [0u64; 16];
    let mut c = [0u64; 16];
    a[0] = 0xFFFF_FFFF_FFFF_FFFF;
    b[0] = 0xFFFF_FFFF_FFFF_FFFF; // dist 0
    c[0] = 0; // dist 64
    let d_ab = hamming_1024(&a, &b);
    let d_ac = hamming_1024(&a, &c);
    let d_s = hamming_scalar(&a, &c);
    d_ab == 0 && d_ac == 64 && d_ac == d_s
}
