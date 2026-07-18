//! Shared GPU kernel logic — CPU golden + ABI POD.
//! Compilado só em tools/; packers geram CUBIN/HSACO/zebin offline.
//! Não depende de Vulkan/CUDA runtime.

#![cfg_attr(not(test), no_std)]

/// Parâmetros POD vector_add (espelha jarbas::gpu::compute_abi::VectorAddParams).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct VectorAddParams {
    pub n: u32,
    pub _pad: u32,
    pub a_pa: u64,
    pub b_pa: u64,
    pub c_pa: u64,
}

/// c[i] = a[i] + b[i]
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
        let d = (got[i] - expect[i]).abs();
        if d > eps {
            return false;
        }
    }
    true
}

/// Stub lógico BitLinear W2A8 — CPU-first; device via KernelPack depois.
pub fn bitlinear_w2a8_ref(weights_w2: &[u8], acts_a8: &[i8], out: &mut [i32], k: usize) -> bool {
    if k == 0 || acts_a8.len() < k || out.is_empty() {
        return false;
    }
    // Placeholder: soma ponderada trivial para golden harness
    let mut acc = 0i32;
    for i in 0..k {
        let w = (weights_w2.get(i / 4).copied().unwrap_or(0) >> ((i % 4) * 2)) & 0x3;
        let wv = match w {
            0 => 0i32,
            1 => 1,
            2 => -1,
            _ => 0,
        };
        acc += wv * (acts_a8[i] as i32);
    }
    out[0] = acc;
    true
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
    fn bitlinear_stub_runs() {
        let w = [0b0001_1001u8]; // sample packed
        let a = [1i8, 2, 3, 4];
        let mut o = [0i32; 1];
        assert!(bitlinear_w2a8_ref(&w, &a, &mut o, 4));
    }
}
