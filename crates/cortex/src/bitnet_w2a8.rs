#![allow(dead_code)]
//! bitnet_w2a8 — Kernel W2A8 oficial (ADR-0084 §3 Fase 4, GATED).
//!
//! Espelha ggml-bitnet-mad.cpp: ativações int8 (si per-token via absmax) ×
//! pesos ternários i8 {-1,0,1} via `_mm256_maddubs_epi16` (u8×i8→i16, 32
//! MACs/instrução), acumulação i32, epílogo com escala f32/linha + desconto
//! do viés (ativação u8 = xq + 128 ⇒ subtrair 128·Σw por par).
//!
//! Layout: pesos precisam de coluna-major (n,k) i8 contíguo p/ maddubs —
//! repack feito aqui (uma vez por chamada). Ativação u8: xq = round(x/si)+128.
//!
//! ⚠️ GATE (ADR-0084 §3 F4): ganho nulo sob TCG; ativar só em WHPX/HW real
//! com gaps de geração resolvidos. `w2a8_enabled()` hoje retorna false —
//! o dispatch não muda comportamento; o kernel é verificado por self-test.

use crate::tensor::{PackedTernaryTensor, Tensor};
use alloc::vec;
use alloc::vec::Vec;

/// Gate de ativação do W2A8 (ADR-0084 §3 F4): WHPX/HW real (não TCG) +
/// AVX2 + gaps de geração resolvidos (soft_stride/MAX_SEQ — pendente).
pub fn w2a8_enabled() -> bool {
    if !k_nano::platform_probe::allow_avx2() {
        return false;
    }
    let hv = k_nano::platform_probe::hypervisor();
    let real_hw = hv == k_nano::platform_probe::HypervisorKind::None
        || hv == k_nano::platform_probe::HypervisorKind::Kvm;
    // ponytail: gaps (soft_stride=3, MAX_SEQ=64, 4-8 tokens) ainda pendentes
    real_hw && crate::cortex::GENERATION_GAPS_RESOLVED.load(core::sync::atomic::Ordering::Relaxed)
}

/// Unpack 2-bit ternário {-1,0,1} de um byte (branchless, ADR-0084 F1).
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
    // peso (t,j): byte (t*n+j)/4, bits ((t*n+j)%4)*2
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

/// W2A8: out[m,n] = quantized(x) @ w_ternary. Returns None se shapes não batem.
/// Escala: cada linha i de x é normalizada por si_i; resultado × w_scale.
/// unsafe: usa intrins — chamar só com `w2a8_enabled()` (runtime check).
///
/// ⚠️ Só compila onde o target suporta: `_mm256_maddubs_epi16` (pmaddubsw
/// 256-bit) exige split LLVM p/ 128-bit (SSSE3), e o target no_std
/// (`x86_64-unknown-none`) desabilita -ssse3 no nível do target — o
/// `#[target_feature]` por função não re-legaliza. Host/test têm AVX2+
/// SSSE3 nativos (SESSION_247: gate por target, não cfg(test)).
#[cfg(all(target_arch = "x86_64", not(target_os = "none")))]
#[target_feature(enable = "avx2,ssse3")]
pub unsafe fn w2a8_ternary_matmul(w: &PackedTernaryTensor, x: &Tensor) -> Option<Tensor> {
    use core::arch::x86_64::*;
    let (k, n) = w.shape;
    let (m, k2) = x.shape;
    if k != k2 || k == 0 || n == 0 {
        return None;
    }

    // Repack coluna-major (n,k) i8
    let w8 = repack_col_major(w);

    let mut result = Tensor::new((m, n));

    for i in 0..m {
        let row = &x.data[i * k..(i + 1) * k];
        // si per-token (absmax / 127)
        let mut max_abs = 0.0f32;
        for &v in row {
            let a = v.abs();
            if a > max_abs {
                max_abs = a;
            }
        }
        let si = if max_abs > 1e-9 { max_abs / 127.0 } else { 1.0 };
        let inv_si = 1.0 / si;
        // ativações u8 = clamp(round(x/si) + 128, 0, 255) — no_std: libm::roundf
        let mut xq = vec![0u8; k];
        for (t, &v) in row.iter().enumerate() {
            let q = unsafe { libm::roundf(v * inv_si) } as i32 + 128;
            xq[t] = q.clamp(0, 255) as u8;
        }

        for j in 0..n {
            let wj = &w8[j * k..(j + 1) * k];
            let mut acc: i64 = 0;
            let mut t = 0usize;
            // 32 t's por iteração (maddubs 256: 32 u8×32 i8 → 16 i16)
            while t + 32 <= k {
                let a32 = _mm256_loadu_si256(xq.as_ptr().add(t) as *const __m256i);
                let b32 = _mm256_loadu_si256(wj.as_ptr().add(t) as *const __m256i);
                // r[0..16] = (a[2l]*b[2l] + a[2l+1]*b[2l+1]) como i16 — u8×i8
                let r = _mm256_maddubs_epi16(a32, b32);
                // desconto do viés: 128·(b[2l]+b[2l+1]) por par
                let ones = _mm256_set1_epi8(1); // u8 = 1
                let wsum = _mm256_maddubs_epi16(ones, b32); // (1*b[2l]+1*b[2l+1])
                let bias = _mm256_mullo_epi16(wsum, _mm256_set1_epi16(128));
                let corr = _mm256_sub_epi16(r, bias);
                // somar os 16 i16 → 8 i32 (via madd com 1)
                let acc32 = _mm256_madd_epi16(corr, _mm256_set1_epi16(1));
                // reduzir 8 i32 → soma — só intrins 256-bit (alvo no_std sem SSE2)
                let mut buf = [0i32; 8];
                _mm256_storeu_si256(buf.as_mut_ptr() as *mut __m256i, acc32);
                acc += (buf[0] + buf[1] + buf[2] + buf[3]
                    + buf[4] + buf[5] + buf[6] + buf[7]) as i64;
                t += 32;
            }
            // tail k%32 — escalar. ⚠️ xq = q+128: subtrair o viés 128*w (mesmo
            // desconto que o caminho SIMD faz via corr = r - bias).
            while t < k {
                let q = xq[t] as i32 - 128;
                acc += (q * wj[t] as i32) as i64;
                t += 1;
            }
            // epílogo: out = acc · si (xq já inclui o +128 desconto aplicado)
            result.data[i * n + j] = acc as f32 * si;
        }
    }
    Some(result)
}

/// Stub no_std (x86_64-unknown-none): o target desabilita -ssse3 no nível do
/// target, então o kernel maddubs real não compila lá (LLVM split error).
/// NUNCA é chamado no boot — `w2a8_enabled()` retorna false (gaps de geração
/// pendentes + WHPX/HW real não é o ambiente dev). Só mantém o call site do
/// dispatch compilável. O kernel real (host/test) é verificado por self-test.
#[cfg(all(target_arch = "x86_64", target_os = "none"))]
pub unsafe fn w2a8_ternary_matmul(_w: &PackedTernaryTensor, _x: &Tensor) -> Option<Tensor> {
    None
}

/// Referência escalar exata (sem quantização) p/ documentar o erro esperado.
/// out[i][j] = Σ_t x[i][t] · w(t,j) — usando unpack direto do packing 2-bit.
pub fn w2a8_reference_scalar(w: &PackedTernaryTensor, x: &Tensor) -> Option<Tensor> {
    let (k, n) = w.shape;
    let (m, k2) = x.shape;
    if k != k2 {
        return None;
    }
    let mut out = Tensor::new((m, n));
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

/// Referência com a MESMA quantização do kernel (si per-token + round + bias):
/// out[i][j] = si_i · Σ_t round(x[i][t]/si_i) · w(t,j). Deve bater ~exato com
/// o kernel (só diferença de arredondamento i32 vs f32).
pub fn w2a8_reference_quantized(w: &PackedTernaryTensor, x: &Tensor) -> Option<Tensor> {
    let (k, n) = w.shape;
    let (m, k2) = x.shape;
    if k != k2 {
        return None;
    }
    let mut out = Tensor::new((m, n));
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
        for j in 0..n {
            let mut acc = 0.0f32;
            for t in 0..k {
                let idx = t * n + j;
                let byte = w.packed_data[idx >> 2];
                let pair = (byte >> ((idx & 3) << 1)) & 3;
                let v = ((pair & 1) as i8) - ((pair >> 1) as i8);
                let q = unsafe { libm::roundf(x.data[i * k + t] * inv_si) } as i32;
                acc += q as f32 * v as f32;
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

    /// Paridade W2A8 vs referência escalar (tolerância de quantização).
    #[test]
    fn w2a8_parity_scalar() {
        let k = 48usize; // 3×16 + tail
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
        // ref com a MESMA quantização (si + round) — deve bater ~exato com o kernel
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
        // kernel i32 vs ref f32 — só arredondamento de acumulação; folga 1%
        assert!(max_rel < 0.01, "w2a8 divergiu: max_rel={:.4}", max_rel);

        // documental: erro do f32 puro deve ser ≤ ~5% (quantização int8)
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

    /// Gate off por padrão (ADR-0084 §3 F4 — não regride TCG).
    #[test]
    fn w2a8_gate_off_by_default() {
        assert!(!w2a8_enabled(), "W2A8 não deve estar ativo por padrão");
    }
}
