#![allow(dead_code)]
//! bitnet_w2a8 — Kernel W2A8 (ADR-0084 §3 F4 / ADR-0105 B3).
//!
//! Espelha ggml-bitnet-mad.cpp: ativações int8 (si per-token via absmax) ×
//! pesos ternários i8 {-1,0,1}. Host: `_mm256_maddubs_epi16`. Bare-metal
//! soft-float: path **escalar quantizado** (mesma matemática; sem claim AVX2).
//!
//! Gate: WHPX/KVM/bare-metal (não TCG) + `GENERATION_GAPS_RESOLVED`.

use crate::tensor::{PackedTernaryTensor, Tensor};
use alloc::vec;
use alloc::vec::Vec;

/// Gate de ativação do W2A8 (ADR-0105 B3.2): não-TCG + gaps resolvidos.
/// AVX2 maddubs só no host; no_std usa path escalar quando o gate abre.
/// Honesty s386: `probe_done()` — pré-detect hypervisor()==None ≡ bare-metal
/// e abriria W2A8 em TCG por engano (SESSION_243).
pub fn w2a8_enabled() -> bool {
    if !crate::cortex::GENERATION_GAPS_RESOLVED.load(core::sync::atomic::Ordering::Relaxed) {
        return false;
    }
    if !k_nano::platform_probe::probe_done() {
        return false;
    }
    use k_nano::platform_probe::HypervisorKind;
    match k_nano::platform_probe::hypervisor() {
        HypervisorKind::Tcg => false,
        HypervisorKind::None
        | HypervisorKind::Kvm
        | HypervisorKind::MicrosoftHv => true,
        // VBox/VMware: AVX filtrado — não vender W2A8 como ganho.
        _ => false,
    }
}

/// Unpack 2-bit ternário — mantido para testes/paridade; hot path usa inline.
#[cfg(test)]
fn unpack_byte(b: u8, out: &mut [i8; 4]) {
    let p0 = (b & 3) as i8;
    let p1 = ((b >> 2) & 3) as i8;
    let p2 = ((b >> 4) & 3) as i8;
    let p3 = ((b >> 6) & 3) as i8;
    out[0] = (p0 & 1) - (p0 >> 1);
    out[1] = (p1 & 1) - (p1 >> 1);
    out[2] = (p2 & 1) - (p2 >> 1);
    out[3] = (p3 & 1) - (p3 >> 1);
}

/// Repack pesos (k,n) packed 2-bit → coluna-major i8 (n,k) contíguo p/ maddubs.
fn repack_col_major(w: &PackedTernaryTensor) -> Vec<i8> {
    let (k, n) = w.shape;
    let mut out = vec![0i8; n * k];
    for j in 0..n {
        for t in 0..k {
            let idx = t * n + j;
            let byte = w.packed_data[idx >> 2];
            let pair = (byte >> ((idx & 3) << 1)) & 3;
            let v = ((pair & 1) as i8) - ((pair >> 1) as i8);
            out[j * k + t] = v;
        }
    }
    out
}

/// LUT byte→4 pesos ternários (0b01=+1, 0b10=-1, 0b00/0b11=0).
/// `const` = .rodata: troca 4× (shift+mask+sub) por 1 load no hot path.
const fn w2a8_lut() -> [f32; 1024] {
    let mut l = [0f32; 1024];
    let mut b = 0usize;
    while b < 256 {
        let mut lane = 0usize;
        while lane < 4 {
            let pair = (b >> (lane * 2)) & 3;
            l[b * 4 + lane] = ((pair & 1) as i8 - (pair >> 1) as i8) as f32;
            lane += 1;
        }
        b += 1;
    }
    l
}
static W2A8_LUT: [f32; 1024] = w2a8_lut();

/// Kernel SSE2: t-externo/j-interno (acesso SEQUENCIAL a packed_data) + LUT.
/// `out` (len == m*n) alocado FORA do `#[target_feature]` — sret soft-float #GP
/// (SESSION_336/362). Requer `n % 4 == 0`.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
unsafe fn w2a8_fill_lut_sse2(
    w: &PackedTernaryTensor,
    xq: &[i32],
    si: &[f32],
    m: usize,
    k: usize,
    n: usize,
    out: &mut [f32],
) {
    use core::arch::x86_64::*;
    let nq = n / 4;
    for i in 0..m {
        let xrow = &xq[i * k..(i + 1) * k];
        let orow = &mut out[i * n..(i + 1) * n];
        for v in orow.iter_mut() {
            *v = 0.0;
        }
        for t in 0..k {
            let xv = _mm_set1_ps(xrow[t] as f32);
            let base_b = t * nq;
            for jb in 0..nq {
                let byte = w.packed_data[base_b + jb] as usize;
                let wv = _mm_loadu_ps(W2A8_LUT.as_ptr().add(byte * 4));
                let o = jb * 4;
                let prev = _mm_loadu_ps(orow.as_ptr().add(o));
                _mm_storeu_ps(orow.as_mut_ptr().add(o), _mm_add_ps(prev, _mm_mul_ps(xv, wv)));
            }
        }
        let sv = _mm_set1_ps(si[i]);
        let mut j = 0usize;
        while j < n {
            let p = _mm_loadu_ps(orow.as_ptr().add(j));
            _mm_storeu_ps(orow.as_mut_ptr().add(j), _mm_mul_ps(p, sv));
            j += 4;
        }
    }
}

/// Quantiza ativações (i32 + escala/linha) e roda o kernel SSE2. `None` em OOM.
#[cfg(target_arch = "x86_64")]
fn w2a8_scalar_lut(w: &PackedTernaryTensor, x: &Tensor, m: usize, k: usize, n: usize) -> Option<Tensor> {
    let mut out = Tensor::new((m, n));
    if !out.is_valid() {
        return None;
    }
    let mut xq = alloc::vec::Vec::new();
    if xq.try_reserve_exact(m * k).is_err() {
        return None;
    }
    xq.resize(m * k, 0i32);
    let mut si = alloc::vec::Vec::new();
    if si.try_reserve_exact(m).is_err() {
        return None;
    }
    si.resize(m, 1.0f32);
    for i in 0..m {
        let mut max_abs = 0.0f32;
        for &v in &x.data[i * k..(i + 1) * k] {
            let a = v.abs();
            if a > max_abs {
                max_abs = a;
            }
        }
        let s = if max_abs > 1e-9 { max_abs / 127.0 } else { 1.0 };
        si[i] = s;
        let inv = 1.0 / s;
        let base = i * k;
        for t in 0..k {
            xq[base + t] = unsafe { libm::roundf(x.data[base + t] * inv) } as i32;
        }
    }
    unsafe {
        w2a8_fill_lut_sse2(w, &xq, &si, m, k, n, &mut out.data);
    }
    Some(out)
}

/// Path escalar quantizado — funciona em soft-float bare-metal (ADR-0105 B3).
/// t-externo/j-interno (memória sequencial) + LUT byte→4 pesos + SSE2 (4 lanes).
/// `n % 4 != 0` cai na referência (o byte LUT não alinha com a linha `t`).
pub fn w2a8_ternary_matmul_scalar(w: &PackedTernaryTensor, x: &Tensor) -> Option<Tensor> {
    let (k, n) = w.shape;
    let (m, k2) = x.shape;
    if k != k2 || k == 0 || n == 0 || m == 0 {
        return None;
    }
    #[cfg(target_arch = "x86_64")]
    if n % 4 == 0 {
        if let Some(r) = w2a8_scalar_lut(w, x, m, k, n) {
            return Some(r);
        }
    }
    let _ = (m, k);
    w2a8_reference_quantized(w, x)
}

/// W2A8: out[m,n] = quantized(x) @ w_ternary.
/// Host: AVX2 maddubs. Bare-metal: scalar quantizado.
pub unsafe fn w2a8_ternary_matmul(w: &PackedTernaryTensor, x: &Tensor) -> Option<Tensor> {
    static PATH_LOGGED: core::sync::atomic::AtomicBool =
        core::sync::atomic::AtomicBool::new(false);
    #[cfg(all(target_arch = "x86_64", not(target_os = "none")))]
    {
        if k_nano::platform_probe::allow_avx2() {
            if !PATH_LOGGED.swap(true, core::sync::atomic::Ordering::Relaxed) {
                k_nano::slog_cortex!("W2A8", "ok", "path=maddubs (host AVX2)");
            }
            return w2a8_ternary_matmul_avx2(w, x);
        }
    }
    if !PATH_LOGGED.swap(true, core::sync::atomic::Ordering::Relaxed) {
        k_nano::slog_cortex!("W2A8", "ok", "path=scalar_quant (soft-float / no AVX2)");
    }
    w2a8_ternary_matmul_scalar(w, x)
}

/// Host AVX2+SSSE3 (não compila no target soft-float — SESSION_249b).
#[cfg(all(target_arch = "x86_64", not(target_os = "none")))]
#[target_feature(enable = "avx2,ssse3")]
unsafe fn w2a8_ternary_matmul_avx2(w: &PackedTernaryTensor, x: &Tensor) -> Option<Tensor> {
    use core::arch::x86_64::*;
    let (k, n) = w.shape;
    let (m, k2) = x.shape;
    if k != k2 || k == 0 || n == 0 {
        return None;
    }

    let w8 = repack_col_major(w);
    let mut result = Tensor::new((m, n));
    if !result.is_valid() {
        return None;
    }

    for i in 0..m {
        let row = &x.data[i * k..(i + 1) * k];
        let mut max_abs = 0.0f32;
        for &v in row {
            let a = v.abs();
            if a > max_abs {
                max_abs = a;
            }
        }
        let si = if max_abs > 1e-9 { max_abs / 127.0 } else { 1.0 };
        let inv_si = 1.0 / si;
        let mut xq = alloc::vec::Vec::new();
        if xq.try_reserve_exact(k).is_err() {
            return None;
        }
        xq.resize(k, 0u8);
        for (t, &v) in row.iter().enumerate() {
            let q = unsafe { libm::roundf(v * inv_si) } as i32 + 128;
            xq[t] = q.clamp(0, 255) as u8;
        }

        for j in 0..n {
            let wj = &w8[j * k..(j + 1) * k];
            let mut acc: i64 = 0;
            let mut t = 0usize;
            while t + 32 <= k {
                let a32 = _mm256_loadu_si256(xq.as_ptr().add(t) as *const __m256i);
                let b32 = _mm256_loadu_si256(wj.as_ptr().add(t) as *const __m256i);
                let r = _mm256_maddubs_epi16(a32, b32);
                let ones = _mm256_set1_epi8(1);
                let wsum = _mm256_maddubs_epi16(ones, b32);
                let bias = _mm256_mullo_epi16(wsum, _mm256_set1_epi16(128));
                let corr = _mm256_sub_epi16(r, bias);
                let acc32 = _mm256_madd_epi16(corr, _mm256_set1_epi16(1));
                let mut buf = [0i32; 8];
                _mm256_storeu_si256(buf.as_mut_ptr() as *mut __m256i, acc32);
                acc += (buf[0] + buf[1] + buf[2] + buf[3]
                    + buf[4] + buf[5] + buf[6] + buf[7]) as i64;
                t += 32;
            }
            while t < k {
                let q = xq[t] as i32 - 128;
                acc += (q * wj[t] as i32) as i64;
                t += 1;
            }
            result.data[i * n + j] = acc as f32 * si;
        }
    }
    Some(result)
}

/// Referência escalar exata (sem quantização) p/ documentar o erro esperado.
pub fn w2a8_reference_scalar(w: &PackedTernaryTensor, x: &Tensor) -> Option<Tensor> {
    let (k, n) = w.shape;
    let (m, k2) = x.shape;
    if k != k2 {
        return None;
    }
    let mut out = Tensor::new((m, n));
    if !out.is_valid() {
        return None;
    }
    for i in 0..m {
        for j in 0..n {
            let mut acc = 0.0f32;
            for t in 0..k {
                let idx = t * n + j;
                let byte = w.packed_data[idx >> 2];
                let pair = (byte >> ((idx & 3) << 1)) & 3;
                let v = ((pair & 1) as i8) - ((pair >> 1) as i8);
                acc += x.data[i * k + t] * v as f32;
            }
            out.data[i * n + j] = acc;
        }
    }
    Some(out)
}

/// Referência com a MESMA quantização do kernel (si per-token + round + bias).
pub fn w2a8_reference_quantized(w: &PackedTernaryTensor, x: &Tensor) -> Option<Tensor> {
    let (k, n) = w.shape;
    let (m, k2) = x.shape;
    if k != k2 || k == 0 || n == 0 || m == 0 {
        return None;
    }
    let mut out = Tensor::new((m, n));
    if !out.is_valid() {
        return None;
    }
    // ponytail: q(i,t) é invariante em j — quantiza a linha UMA vez.
    // Antes era O(m·n·k) chamadas a libm::roundf (soft-float) dentro do laço j;
    // agora é O(m·k). Mesma matemática do path AVX2 (bitnet_w2a8.rs:119-127).
    let mut xq = alloc::vec::Vec::new();
    if xq.try_reserve_exact(k).is_err() {
        return None;
    }
    xq.resize(k, 0i32);
    for i in 0..m {
        let mut max_abs = 0.0f32;
        for &v in &x.data[i * k..(i + 1) * k] {
            let a = v.abs();
            if a > max_abs {
                max_abs = a;
            }
        }
        let si = if max_abs > 1e-9 { max_abs / 127.0 } else { 1.0 };
        let inv_si = 1.0 / si;
        for t in 0..k {
            xq[t] = unsafe { libm::roundf(x.data[i * k + t] * inv_si) } as i32;
        }
        for j in 0..n {
            let mut acc = 0.0f32;
            for t in 0..k {
                let idx = t * n + j;
                let byte = w.packed_data[idx >> 2];
                let pair = (byte >> ((idx & 3) << 1)) & 3;
                let v = ((pair & 1) as i8) - ((pair >> 1) as i8);
                acc += xq[t] as f32 * v as f32;
            }
            out.data[i * n + j] = acc * si;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn tern(rows: usize, cols: usize, seed: u32) -> PackedTernaryTensor {
        let mut x = seed;
        let mut vals = Vec::with_capacity(rows * cols);
        for _ in 0..rows * cols {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            vals.push(match x % 3 {
                0 => 1i8,
                1 => -1i8,
                _ => 0i8,
            });
        }
        PackedTernaryTensor {
            shape: (rows, cols),
            packed_data: PackedTernaryTensor::pack_weights(&vals),
        }
    }

    #[test]
    fn w2a8_parity_scalar() {
        let k = 48usize;
        let n = 8usize;
        let m = 4usize;
        let w = tern(k, n, 99);
        let mut xdata = Vec::with_capacity(m * k);
        for i in 0..m {
            for t in 0..k {
                xdata.push(((i * 13 + t) % 7) as f32 * 0.5 - 1.5);
            }
        }
        let x = Tensor::from_row_major((m, k), xdata).unwrap();

        let got = unsafe { w2a8_ternary_matmul(&w, &x) }.expect("w2a8 None");
        let want = w2a8_reference_quantized(&w, &x).expect("ref None");

        let mut max_rel = 0.0f32;
        let mut first_bad = None;
        for idx in 0..m * n {
            let denom = want.data[idx].abs().max(1e-3);
            let rel = ((got.data[idx] - want.data[idx]).abs() / denom).min(1.0);
            if rel > max_rel {
                max_rel = rel;
            }
            if rel > 0.01 && first_bad.is_none() {
                first_bad = Some((idx, got.data[idx], want.data[idx]));
            }
        }
        if let Some((idx, g, wv)) = first_bad {
            panic!("w2a8 diverge idx={} got={} want={}", idx, g, wv);
        }
        assert!(max_rel < 0.01, "w2a8 divergiu: max_rel={:.4}", max_rel);

        let pure = w2a8_reference_scalar(&w, &x).expect("pure None");
        let mut qerr = 0.0f32;
        for idx in 0..m * n {
            let denom = pure.data[idx].abs().max(1e-3);
            let rel = ((got.data[idx] - pure.data[idx]).abs() / denom).min(1.0);
            if rel > qerr {
                qerr = rel;
            }
        }
        assert!(qerr < 0.05, "w2a8 quant err inesperado: {:.4}", qerr);
    }

    #[test]
    fn w2a8_gate_off_by_default() {
        assert!(
            !crate::cortex::GENERATION_GAPS_RESOLVED.load(core::sync::atomic::Ordering::Relaxed),
            "gaps default false"
        );
        assert!(!w2a8_enabled(), "W2A8 não deve estar ativo por padrão");
    }

    #[test]
    fn w2a8_scalar_path_matches_ref() {
        let w = tern(32, 4, 7);
        let x = Tensor::from_row_major((2, 32), vec![0.5f32; 64]).unwrap();
        let a = w2a8_ternary_matmul_scalar(&w, &x).unwrap();
        let b = w2a8_reference_quantized(&w, &x).unwrap();
        for i in 0..a.data.len() {
            assert!((a.data[i] - b.data[i]).abs() < 1e-5);
        }
    }

    /// Lane D-accel: tail n%4 (heads reais 17/9/10) — desconto do viés
    /// xq=q+128 exato vs ref quantizada; erro vs f32 puro ≤5% documentado.
    #[test]
    fn w2a8_tail_n9_matches_quantized_ref() {
        let k = 64usize;
        let n = 9usize;
        let m = 2usize;
        let w = tern(k, n, 4242);
        let mut xdata = Vec::with_capacity(m * k);
        for i in 0..m {
            for t in 0..k {
                xdata.push(((i * 31 + t * 7) % 11) as f32 * 0.25 - 1.25);
            }
        }
        let x = Tensor::from_row_major((m, k), xdata).unwrap();
        let got = w2a8_ternary_matmul_scalar(&w, &x).expect("scalar");
        let want = w2a8_reference_quantized(&w, &x).expect("ref");
        assert_eq!(got.shape, (m, n));
        for i in 0..m * n {
            assert!(
                (got.data[i] - want.data[i]).abs() < 1e-5,
                "tail bias idx={} got={} want={}",
                i,
                got.data[i],
                want.data[i]
            );
        }
        let pure = w2a8_reference_scalar(&w, &x).expect("pure");
        let mut qerr = 0.0f32;
        for i in 0..m * n {
            let denom = pure.data[i].abs().max(1e-3);
            let rel = ((got.data[i] - pure.data[i]).abs() / denom).min(1.0);
            if rel > qerr {
                qerr = rel;
            }
        }
        assert!(qerr < 0.05, "w2a8 tail quant err: {:.4}", qerr);
    }

    /// Lane D-accel: kernel maddubs host com tail k%32/n%4 — paridade vs ref
    /// quantizada (folga 1%: ordem de acumulação i64 vs f32 difere no ulp).
    /// Sem AVX2 no host o dispatch cai no scalar (mesma ref) — passa igual.
    #[cfg(all(target_arch = "x86_64", not(target_os = "none")))]
    #[test]
    fn w2a8_maddubs_tail_parity_host() {
        let k = 64usize;
        let n = 9usize;
        let m = 2usize;
        let w = tern(k, n, 777);
        let mut xdata = Vec::with_capacity(m * k);
        for i in 0..m {
            for t in 0..k {
                xdata.push(((i * 17 + t * 5) % 13) as f32 * 0.2 - 1.2);
            }
        }
        let x = Tensor::from_row_major((m, k), xdata).unwrap();
        let got = unsafe { w2a8_ternary_matmul(&w, &x) }.expect("w2a8");
        let want = w2a8_reference_quantized(&w, &x).expect("ref");
        assert_eq!(got.shape, (m, n));
        for i in 0..m * n {
            let denom = want.data[i].abs().max(1e-3);
            let rel = ((got.data[i] - want.data[i]).abs() / denom).min(1.0);
            assert!(rel < 0.01, "maddubs tail idx={} rel={:.4}", i, rel);
        }
    }
}
