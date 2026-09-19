//! Shared GPU kernel logic — CPU golden + ABI POD.
//! Compilado só em tools/; packers geram CUBIN/HSACO/zebin offline.
//! Golden W2A8 espelha cortex::bitnet_w2a8::w2a8_reference_quantized (si+round).

#![cfg_attr(not(test), no_std)]

extern crate alloc;

/// Parâmetros POD vector_add.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct VectorAddParams {
    pub n: u32,
    pub _pad: u32,
    pub a_pa: u64,
    pub b_pa: u64,
    pub c_pa: u64,
}

pub fn vector_add_f32(a: &[f32], b: &[f32], c: &mut [f32]) -> bool {
    if a.len() != b.len() || a.len() != c.len() || a.is_empty() {
        return false;
    }
    for i in 0..a.len() {
        c[i] = a[i] + b[i];
    }
    true
}

pub fn vector_add_check(got: &[f32], expect: &[f32], eps: f32) -> bool {
    if got.len() != expect.len() || got.is_empty() {
        return false;
    }
    for i in 0..got.len() {
        if (got[i] - expect[i]).abs() > eps {
            return false;
        }
    }
    true
}

/// Unpack 2-bit ternário {-1,0,1} (mesma convenção cortex bitnet_w2a8).
fn unpack_pair(byte: u8, lane: usize) -> i8 {
    let pair = (byte >> ((lane & 3) << 1)) & 3;
    ((pair & 1) as i8) - ((pair >> 1) as i8)
}

/// Round-to-nearest ties-away — `f32::round` ausente em no_std soft-float.
#[inline]
fn round_nearest(x: f32) -> f32 {
    if x >= 0.0 {
        (x + 0.5) as i32 as f32
    } else {
        (x - 0.5) as i32 as f32
    }
}

/// Golden BitLinear W2A8 (M=1 decode): out[n] = si · Σ round(x[k]/si) · w(k,n).
/// Layout pesos: packed row-major (k,n) 2-bit — espelha PackedTernaryTensor.
/// Substitui o placeholder Wave0; shapes Falcon3 via caller (3072/9216/…).
pub fn bitlinear_w2a8_ref(
    weights_w2: &[u8],
    x: &[f32],
    out: &mut [f32],
    k: usize,
    n: usize,
) -> bool {
    if k == 0 || n == 0 || x.len() < k || out.len() < n {
        return false;
    }
    let need = (k * n + 3) / 4;
    if weights_w2.len() < need {
        return false;
    }
    let mut max_abs = 0.0f32;
    for &v in &x[..k] {
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
            let byte = weights_w2[idx >> 2];
            let wv = unpack_pair(byte, idx) as f32;
            let q = round_nearest(x[t] * inv_si) as i32;
            acc += q as f32 * wv;
        }
        out[j] = acc * si;
    }
    true
}

/// GEMV shapes únicos Falcon3 1.58bit (1B/3B/7B≡10B) para harness de pack.
pub fn falcon3_gemv_shapes() -> [(usize, usize); 7] {
    [
        (2048, 2048),
        (8192, 2048),
        (2048, 8192),
        (3072, 3072),
        (9216, 3072),
        (3072, 9216),
        (23040, 3072), // 7B/10B up
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_add_golden() {
        let a = [1.0f32, 2.0, 3.0, 4.0];
        let b = [10.0f32, 20.0, 30.0, 40.0];
        let mut c = [0.0f32; 4];
        assert!(vector_add_f32(&a, &b, &mut c));
        assert!(vector_add_check(&c, &[11.0, 22.0, 33.0, 44.0], 1e-6));
    }

    #[test]
    fn w2a8_ref_identity_scale() {
        // k=4 n=1: pesos +1,+1,+1,+1 packed 0b01_01_01_01 = 0x55
        let w = [0x55u8];
        let x = [1.0f32, 2.0, 3.0, 4.0];
        let mut o = [0.0f32; 1];
        assert!(bitlinear_w2a8_ref(&w, &x, &mut o, 4, 1));
        // si = 4/127; q = round(x/si); sum q * 1 * si ≈ sum x
        assert!((o[0] - 10.0).abs() < 0.5, "got {}", o[0]);
    }

    #[test]
    fn falcon3_shapes_not_bitnet2b() {
        for &(n, k) in &falcon3_gemv_shapes() {
            assert_ne!(k, 2560);
            assert!(n > 0 && k > 0);
        }
    }
}
