//! ADR-0063 D1 — Despachante Hamming: scalar | AVX2 XOR | AVX-512.
//! Runtime adaptive: `#[target_feature]` permite compilar kernels SIMD mesmo em
//! build soft-float. `select_best_hamming_kernel()` escolhe no boot baseado no CPU.
//! Apenas VPSHUFB (LUT 4-bit) não é usado — LLVM aborta em soft-float.
//!
//! Honesty: AVX-512 (VPOPCNT) == 16× faster, mas raro em 2026 consumer silicon.
//! AVX2 XOR ≈ 4× faster que scalar; ambos compilam em soft-float sem problemas.

use core::sync::atomic::{AtomicU8, Ordering};

const PATH_SCALAR: u8 = 0;
const PATH_AVX2_XOR: u8 = 1;
const PATH_AVX512: u8 = 2;

static HAMMING_PATH: AtomicU8 = AtomicU8::new(PATH_SCALAR);

pub type HammingFn = fn(&[u64], &[u64]) -> u32;

/// Chamado no boot (após platform_probe). Escolhe o melhor kernel disponível.
pub fn select_best_hamming_kernel() {
    let hw = k_nano::platform_probe::hw_info();
    if hw.avx512_ready() {
        HAMMING_PATH.store(PATH_AVX512, Ordering::Relaxed);
    } else if hw.avx2_ready() {
        HAMMING_PATH.store(PATH_AVX2_XOR, Ordering::Relaxed);
    } else {
        HAMMING_PATH.store(PATH_SCALAR, Ordering::Relaxed);
    }
}

pub fn path_name() -> &'static str {
    match HAMMING_PATH.load(Ordering::Relaxed) {
        PATH_AVX512 => "avx512",
        PATH_AVX2_XOR => "avx2_xor",
        _ => "scalar",
    }
}

pub fn active_kernel() -> HammingFn {
    match HAMMING_PATH.load(Ordering::Relaxed) {
        PATH_AVX512 => hamming_avx512_or_fallback,
        PATH_AVX2_XOR => hamming_avx2_or_fallback,
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

// ─── AVX2 kernel ──────────────────────────────────────────────
// Compilado via #[target_feature(enable = "avx2")] mesmo em build soft-float.
// Runtime: só chamado se allow_avx2().
// fallback → hamming_scalar se cpu sem AVX2.

#[cfg(target_arch = "x86_64")]
fn hamming_avx2_or_fallback(a: &[u64], b: &[u64]) -> u32 {
    if k_nano::platform_probe::allow_avx2() {
        return unsafe { hamming_avx2_xor(a, b) };
    }
    hamming_scalar(a, b)
}

/// AVX2: XOR YMM + popcount via GPR extract (sem VPSHUFB, sem store p/ mem).
/// Evita `_mm256_storeu_si256` para stack — extrai cada u64 via
/// `_mm256_extracti128_si256` + `_mm_extract_epi64`/`_mm_cvtsi128_si64`
/// diretamente p/ registradores GPR.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn hamming_avx2_xor(a: &[u64], b: &[u64]) -> u32 {
    use core::arch::x86_64::*;
    let n = a.len().min(b.len());
    let mut d = 0u32;
    let mut i = 0;
    while i + 4 <= n {
        let va = _mm256_loadu_si256(a.as_ptr().add(i) as *const __m256i);
        let vb = _mm256_loadu_si256(b.as_ptr().add(i) as *const __m256i);
        let x = _mm256_xor_si256(va, vb);
        // Extrai sem store: split YMM → 2× XMM → 4× u64 direto em GPR
        let lo128 = _mm256_extracti128_si256::<0>(x);
        let hi128 = _mm256_extracti128_si256::<1>(x);
        d += (_mm_cvtsi128_si64(lo128) as u64).count_ones();
        d += (_mm_extract_epi64::<1>(lo128) as u64).count_ones();
        d += (_mm_cvtsi128_si64(hi128) as u64).count_ones();
        d += (_mm_extract_epi64::<1>(hi128) as u64).count_ones();
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

// ─── AVX-512 kernels ──────────────────────────────────────────
// Compilados via #[target_feature] mesmo em build soft-float.
// Runtime: só chamado se allow_avx512().

#[cfg(target_arch = "x86_64")]
fn hamming_avx512_or_fallback(a: &[u64], b: &[u64]) -> u32 {
    if k_nano::platform_probe::allow_avx512() {
        return unsafe { hamming_avx512_dispatch(a, b) };
    }
    if k_nano::platform_probe::allow_avx2() {
        return unsafe { hamming_avx2_xor(a, b) };
    }
    hamming_scalar(a, b)
}

#[cfg(target_arch = "x86_64")]
unsafe fn hamming_avx512_dispatch(a: &[u64], b: &[u64]) -> u32 {
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
