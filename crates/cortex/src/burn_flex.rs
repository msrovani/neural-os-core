//! burn-flex backend — #333: SIMD GEMM + quantization wrapper.
//! Adapta Tensor e PackedTernaryTensor para interface burn.
//! WIP: stub funcional, integracao com burn::backend::Backend futura.

use alloc::vec::Vec;
use alloc::string::String;
use crate::tensor::{Tensor, PackedTernaryTensor};

/// Tipo de dados do backend
#[derive(Clone, Copy, PartialEq)]
pub enum FloatElem { F32, I8 }

/// Dispositivo (CPU sempre em bare-metal)
#[derive(Clone, Copy)]
pub struct Device;

/// Backend SIMD flex
pub struct FlexBackend;

impl FlexBackend {
    pub fn new() -> Self { FlexBackend }

    /// GEMM: C = A @ B usando o melhor backend disponivel
    pub fn gemm(a: &Tensor, b: &Tensor) -> Option<Tensor> {
        // Tenta GPU primeiro, fallback CPU AVX2, fallback scalar
        a.matmul(b)
    }

    /// GEMM ternario: pesos {-1,0,+1} @ ativacoes f32
    pub fn gemm_ternary(w: &PackedTernaryTensor, x: &Tensor) -> Option<Tensor> {
        w.matmul_hybrid(x)
    }

    /// Quantizacao para ternario (threshold adaptativo)
    pub fn quantize_ternary(data: &[f32], threshold: f32) -> Vec<i8> {
        data.iter().map(|&v| {
            if v > threshold { 1 } else if v < -threshold { -1 } else { 0 }
        }).collect()
    }

    /// Pack 4 pesos ternarios em 1 byte
    pub fn pack_ternary(weights: &[i8]) -> Vec<u8> {
        let mut packed = Vec::with_capacity((weights.len() + 3) / 4);
        for chunk in weights.chunks(4) {
            let mut byte = 0u8;
            for (j, &w) in chunk.iter().enumerate() {
                let bits = match w { 1 => 0b01, -1 => 0b10, _ => 0b00 };
                byte |= bits << (j * 2);
            }
            packed.push(byte);
        }
        packed
    }

    /// Status do backend
    pub fn status() -> String {
        alloc::format!("[FLEX] GEMM: Tensor::matmul + PackedTernaryTensor::matmul_hybrid")
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
        // w[0]=1 em bits 0-1 (01), w[1]=-1 em bits 2-3 (10), w[2]=0 em bits 4-5 (00),
        // w[3]=1 em bits 6-7 (01) → byte = 0b0100_1001. O literal antigo (0b01_10_00_01)
        // lia MSB-first e divergia do código.
        assert_eq!(p[0], 0b0100_1001);
    }
}
