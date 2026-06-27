//! XOR Delta — compressão e reconstrução bit-exata de tensores ternários.
//!
//! ArchiveTensor armazena pesos empacotados + resíduo XOR para permitir
//! round-trip perfeito entre compressão e descompressão.
//! Útil para checkpoint de pesos de rede, snapshot de bitmaps, etc.

use alloc::vec::Vec;
use crate::tensor::PackedTernaryTensor;

/// Estrutura de arquivo compactado: pesos empacotados + delta XOR
pub struct ArchiveTensor {
    /// Pesos ternários empacotados (2 bits/peso)
    pub packed: Vec<u8>,
    /// Resíduo XOR: packed_base ^ packed_current = delta
    pub delta: Vec<u8>,
    /// Dimensão original (linhas, colunas)
    pub rows: usize,
    pub cols: usize,
}

impl ArchiveTensor {
    /// Cria um archive a partir de um tensor empacotado e uma base de referência.
    /// `base` é o estado anterior (pode ser vazio para archive inicial).
    pub fn new(current: &PackedTernaryTensor, base: &[u8]) -> Self {
        let delta = if base.len() == current.packed_data.len() {
            // XOR entre current e base
            current.packed_data.iter()
                .zip(base.iter())
                .map(|(c, b)| c ^ b)
                .collect()
        } else if base.is_empty() {
            // Primeiro archive: delta = próprio current
            current.packed_data.clone()
        } else {
            // Tamanhos diferentes — não é possível delta, usa full
            current.packed_data.clone()
        };

        ArchiveTensor {
            packed: current.packed_data.clone(),
            delta,
            rows: current.shape.0,
            cols: current.shape.1,
        }
    }

    /// Reconstrói o tensor original a partir do archive.
    /// Se `base` for fornecido, aplica XOR com delta para restaurar.
    pub fn reconstruct(&self, base: &[u8]) -> PackedTernaryTensor {
        let reconstructed = if base.len() == self.delta.len() {
            // base ^ delta = original
            base.iter()
                .zip(self.delta.iter())
                .map(|(b, d)| b ^ d)
                .collect()
        } else {
            // Sem base válida, retorna o stored (pode ser incompleto)
            self.packed.clone()
        };

        PackedTernaryTensor {
            shape: (self.rows, self.cols),
            packed_data: reconstructed,
        }
    }

    /// Tamanho total do archive em bytes
    pub fn total_bytes(&self) -> usize {
        self.packed.len() + self.delta.len()
    }

    /// Taxa de compressão: dados originais / archive
    pub fn compression_ratio(&self) -> f32 {
        let orig = self.rows * self.cols / 4; // packed: 4 pesos/byte
        let arch = self.total_bytes();
        if arch == 0 { return 1.0; }
        orig as f32 / arch as f32
    }

    /// Apenas o delta (útil para transmissão entre checkpoints)
    pub fn delta_only(&self) -> &[u8] {
        &self.delta
    }
}

/// Aplica XOR entre dois buffers byte a byte, produzindo um terceiro.
pub fn xor_buffers(a: &[u8], b: &[u8]) -> Vec<u8> {
    let len = core::cmp::min(a.len(), b.len());
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        out.push(a[i] ^ b[i]);
    }
    out
}

/// Aplica XOR in-place: a[i] ^= b[i]
pub fn xor_in_place(a: &mut [u8], b: &[u8]) {
    let len = core::cmp::min(a.len(), b.len());
    for i in 0..len {
        a[i] ^= b[i];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tensor::quantize_to_packed;
    use crate::tensor::Tensor;

    #[test]
    fn roundtrip() {
        let t = Tensor::from_row_major((3, 4), vec![1.0, -0.5, 2.0, -1.5, 0.5, 0.0, -2.0, 1.5, -0.8, 3.0, -3.0, 0.1]).unwrap();
        let packed = quantize_to_packed(&t, 0.5);
        let archive = ArchiveTensor::new(&packed, &[]);
        let restored = archive.reconstruct(&[]);
        assert_eq!(packed.packed_data.len(), restored.packed_data.len());
    }

    #[test]
    fn delta_detects_change() {
        let t1 = Tensor::from_row_major((2, 2), vec![1.0, 0.0, -1.0, 0.5]).unwrap();
        let p1 = quantize_to_packed(&t1, 0.5);

        let t2 = Tensor::from_row_major((2, 2), vec![0.0, 1.0, -1.0, -0.5]).unwrap();
        let p2 = quantize_to_packed(&t2, 0.5);

        let archive = ArchiveTensor::new(&p2, &p1.packed_data);
        // O delta deve ser diferente de zero (os tensores mudaram)
        assert!(!archive.delta.iter().all(|&b| b == 0));

        // Reconstruir de p1 + delta = p2
        let restored = archive.reconstruct(&p1.packed_data);
        assert_eq!(restored.packed_data, p2.packed_data);
    }
}
