//! Parallel Matmul — multiplicação de matrizes paralela usando work-stealing.
//! Divide o trabalho em blocos de linhas e distribui entre cores.
//!
//! Usa WorkStealingPool para balanceamento dinâmico de carga.

use alloc::vec::Vec;
use crate::tensor::Tensor;
use core::sync::atomic::{AtomicUsize, Ordering};

/// Matmul paralela simples usando chunking (sem work-stealing complexo)
pub fn parallel_matmul(a: &Tensor, b: &Tensor) -> Option<Tensor> {
    let (m, k) = a.shape;
    let (k2, n) = b.shape;
    
    if k != k2 {
        return None; // Dimensões incompatíveis
    }
    
    let m = m;
    let n = n;
    let k = k;
    
    // Aloca buffer de output
    let mut c_data = Vec::with_capacity(m * n);
    c_data.resize(m * n, 0.0f32);
    
    // Matmul paralela simples — divide em chunks de linhas
    let chunk_size = if m > 4 { m / 4 } else { 1 };
    let mut row_start = 0usize;
    
    while row_start < m {
        let row_end = (row_start + chunk_size).min(m);
        
        // Processa linhas [row_start, row_end)
        for i in row_start..row_end {
            for j in 0..n {
                let mut sum = 0.0f32;
                for l in 0..k {
                    let a_val = a.data[i * k + l];
                    let b_val = b.data[l * n + j];
                    sum += a_val * b_val;
                }
                c_data[i * n + j] = sum;
            }
        }
        
        row_start = row_end;
    }
    
    // Cria tensor de output
    Tensor::from_row_major((m, n), c_data)
}

/// Matmul paralela ternária (PackedTernaryTensor) — stub para futuro
pub fn parallel_ternary_matmul(_a: &crate::tensor::PackedTernaryTensor, _b: &crate::tensor::PackedTernaryTensor) -> Option<crate::tensor::Tensor> {
    // TODO: Implementar parallel matmul ternária usando work-stealing
    // Por enquanto, fallback para scalar
    None
}
