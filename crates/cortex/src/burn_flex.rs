//! burn-flex facade — #333: wrappers finos sobre Tensor / PackedTernaryTensor.
//!
//! Não é `burn::backend::Backend` (std/tokio). AIOS honesty: gemm = caminhos
//! canônicos já no cortex; sem segunda verdade de matmul.

use alloc::vec::Vec;
use alloc::string::String;
use crate::tensor::{Tensor, PackedTernaryTensor};

/// Dispositivo (CPU sempre em bare-metal).
#[derive(Clone, Copy)]
pub struct Device;

/// Facade SIMD/ternary — delega a `Tensor` / `PackedTernaryTensor`.
pub struct FlexBackend;

impl FlexBackend {
    /// GEMM f32 via `Tensor::matmul` (AVX2/SMP/scalar internos).
    pub fn gemm(a: &Tensor, b: &Tensor) -> Option<Tensor> {
        a.matmul(b)
    }

    /// GEMM ternário via `matmul_hybrid` → `dispatch_ternary`.
    pub fn gemm_ternary(w: &PackedTernaryTensor, x: &Tensor) -> Option<Tensor> {
        w.matmul_hybrid(x)
    }

    /// Quantização ternária por threshold (host/tests; kernel usa `quantize_to_packed`).
    pub fn quantize_ternary(data: &[f32], threshold: f32) -> Vec<i8> {
        data.iter()
            .map(|&v| {
                if v > threshold {
                    1
                } else if v < -threshold {
                    -1
                } else {
                    0
                }
            })
            .collect()
    }

    /// Pack 4 pesos ternários/byte (mesma ordem LSB-first do kernel BitNet).
    pub fn pack_ternary(weights: &[i8]) -> Vec<u8> {
        let mut packed = Vec::with_capacity(weights.len().div_ceil(4));
        for chunk in weights.chunks(4) {
            let mut byte = 0u8;
            for (j, &w) in chunk.iter().enumerate() {
                let bits = match w {
                    1 => 0b01,
                    -1 => 0b10,
                    _ => 0b00,
                };
                byte |= bits << (j * 2);
            }
            packed.push(byte);
        }
        packed
    }

    pub fn status() -> String {
        String::from("[FLEX] facade→Tensor::matmul + PackedTernaryTensor::matmul_hybrid")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_quantize() {
        let data = vec![0.5, -0.1, 0.0, -0.6];
        let q = FlexBackend::quantize_ternary(&data, 0.3);
        assert_eq!(q, vec![1, 0, 0, -1]);
    }
    #[test]
    fn test_pack() {
        let w = vec![1i8, -1, 0, 1];
        let p = FlexBackend::pack_ternary(&w);
        assert_eq!(p[0], 0b0100_1001);
    }
}
