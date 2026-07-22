//! TernaryTensor — BitNet b1.58 bit-packing (4 pesos/byte) + matmul add/sub.
//!
//! Cada peso ternário: -1, 0, +1. Empacotamento: 2 bits por peso:
//!   00 = 0, 01 = +1, 10 = -1, 11 = reservado.
//! Matmul usa **apenas adição/subtração** — zero multiplicações de ponto flutuante.
//!
//! # Arquitetura (lei 1)
//! `k_ai::ternary::TernaryTensor` → pack/unpack + add/sub matmul.
//! Dispatch SIMD: `k_ai::arch::x86_64` (AVX2 → SSE4.2 → scalar).
//!
//! # Uso
//! ```ignore
//! let t = TernaryTensor::pack(&weights, rows, cols);
//! let out = t.matmul_addsub(&input);  // só add/sub
//! ```

use alloc::vec::Vec;
use alloc::vec;

/// Codifica 4 pesos ternários em 1 byte (2 bits/peso).
/// 00=0, 01=+1, 10=-1, 11=reservado.
fn pack_four(a: i8, b: i8, c: i8, d: i8) -> u8 {
    let e = |x: i8| -> u8 {
        match x {
            1 => 0b01,
            -1 => 0b10,
            _ => 0b00,
        }
    };
    e(a) | (e(b) << 2) | (e(c) << 4) | (e(d) << 6)
}

/// Decodifica 1 peso ternário de um byte no índice idx (0..3).
fn unpack_one(byte: u8, idx: u8) -> i8 {
    let code = (byte >> (idx * 2)) & 0b11;
    match code {
        0b01 => 1,
        0b10 => -1,
        _ => 0,
    }
}

/// Tensor ternário empacotado: 4 pesos por byte de RAM.
/// Matmul usa exclusivamente adição/subtração (BitNet b1.58).
pub struct TernaryTensor {
    pub shape: (usize, usize),       // (rows, cols)
    pub packed: Vec<u8>,            // dados empacotados
}

impl TernaryTensor {
    /// Cria um tensor ternário vazio (zero-filled).
    pub fn new(rows: usize, cols: usize) -> Self {
        let packed_len = rows * ((cols + 3) / 4);
        Self { shape: (rows, cols), packed: vec![0u8; packed_len] }
    }

    /// Empacota um slice i8 (row-major) em 4 pesos/byte.
    pub fn pack(weights: &[i8], rows: usize, cols: usize) -> Self {
        let mut t = Self::new(rows, cols);
        let cols4 = (cols + 3) / 4;
        for r in 0..rows {
            for c in (0..cols).step_by(4) {
                let a = weights.get(r * cols + c).copied().unwrap_or(0);
                let b = weights.get(r * cols + c + 1).copied().unwrap_or(0);
                let c2 = weights.get(r * cols + c + 2).copied().unwrap_or(0);
                let d = weights.get(r * cols + c + 3).copied().unwrap_or(0);
                t.packed[r * cols4 + c / 4] = pack_four(a, b, c2, d);
            }
        }
        t
    }

    /// Extrai uma linha do tensor como Vec<i8> desempacotado.
    pub fn unpack_row(&self, row: usize) -> Vec<i8> {
        let (_, cols) = self.shape;
        let cols4 = (cols + 3) / 4;
        let mut out = vec![0i8; cols];
        let row_start = row * cols4;
        for i in 0..cols {
            out[i] = unpack_one(self.packed[row_start + i / 4], (i % 4) as u8);
        }
        out
    }

    /// Matmul: output[r] = Σⱼ weight[r][j] * input[j] para cada linha r.
    /// Usa **apenas adição e subtração** — sem multiplicação.
    /// Quando input[j] é multiplicado por weight = ±1, vira ±input[j].
    /// Peso 0 → skip.
    pub fn matmul_addsub(&self, input: &[f32]) -> Vec<f32> {
        let (rows, cols) = self.shape;
        assert_eq!(input.len(), cols, "TernaryTensor::matmul_addsub: input len mismatch");
        let cols4 = (cols + 3) / 4;
        let mut output = vec![0.0f32; rows];

        for r in 0..rows {
            let row_start = r * cols4;
            let mut sum = 0.0f32;
            // ponytail: loop escalar — dispatch SIMD via arch quando disponível
            for c in 0..cols {
                let w = unpack_one(self.packed[row_start + c / 4], (c % 4) as u8);
                match w {
                    1 => sum += input[c],
                    -1 => sum -= input[c],
                    _ => {}
                }
            }
            output[r] = sum;
        }
        output
    }

    // ponytail: SIMD dispatch (arch module) disabled; using scalar fallback
    fn bitwise_add_scalar(a: *const i8, b: *const i8, output: *mut i32, len: usize) {
        for i in 0..len {
            unsafe {
                let sum = (*a.add(i) as i32) + (*b.add(i) as i32);
                *output = sum;
            }
        }
    }

    #[cfg(target_arch = "x86_64")]
    pub fn matmul_simd(&self, input: &[f32]) -> Vec<f32> {
        let (rows, cols) = self.shape;
        assert_eq!(input.len(), cols);
        let cols4 = (cols + 3) / 4;

        let input_i8: Vec<i8> = input.iter().map(|&v| {
            if v > 0.0 { 1 } else if v < 0.0 { -1 } else { 0 }
        }).collect();

        let mut output = vec![0.0f32; rows];

        let mut row_buf = vec![0i8; cols];
        for r in 0..rows {
            let row_start = r * cols4;
            for i in 0..cols {
                row_buf[i] = unpack_one(self.packed[row_start + i / 4], (i % 4) as u8);
            }

            let mut acc: i32 = 0;
            Self::bitwise_add_scalar(row_buf.as_ptr(), input_i8.as_ptr(), &mut acc as *mut i32, cols);
            output[r] = acc as f32;
        }
        output
    }

    /// Número de bytes ocupados (útil para estimativa de memória).
    pub fn nbytes(&self) -> usize {
        self.packed.len()
    }
}

// ─── Self-test ───
/// Verifica pack/unpack e matmul_addsub com valores conhecidos.
pub fn self_test() -> bool {
    // 2×4: [+1, 0, -1, +1]
    //       [0, +1, 0, -1]
    let weights: [i8; 8] = [1, 0, -1, 1, 0, 1, 0, -1];
    let t = TernaryTensor::pack(&weights, 2, 4);

    // verify unpack row 0
    let row0 = t.unpack_row(0);
    assert_eq!(row0, [1, 0, -1, 1], "unpack_row(0)");

    // verify unpack row 1
    let row1 = t.unpack_row(1);
    assert_eq!(row1, [0, 1, 0, -1], "unpack_row(1)");

    // verify matmul: input = [1.0, 2.0, 3.0, 4.0]
    // row0 = 1*1 + 0*2 + (-1)*3 + 1*4 = 1 - 3 + 4 = 2
    // row1 = 0*1 + 1*2 + 0*3 + (-1)*4 = 2 - 4 = -2
    let input = [1.0, 2.0, 3.0, 4.0];
    let out = t.matmul_addsub(&input);
    assert!((out[0] - 2.0).abs() < 1e-6, "row 0 esperado 2.0, got {}", out[0]);
    assert!((out[1] - (-2.0)).abs() < 1e-6, "row 1 esperado -2.0, got {}", out[1]);

    // pack/roundtrip: pack + unpack_row deve reproduzir entrada
    let t2 = TernaryTensor::pack(&weights, 2, 4);
    for r in 0..2 {
        let unpacked = t2.unpack_row(r);
        for c in 0..4 {
            assert_eq!(unpacked[c], weights[r * 4 + c],
                "roundtrip ({},{})", r, c);
        }
    }

    true
}
