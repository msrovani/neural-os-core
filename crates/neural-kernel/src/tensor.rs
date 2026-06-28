use alloc::vec;
use alloc::vec::Vec;

pub struct Tensor {
    pub shape: (usize, usize),
    pub data: Vec<f32>,
}

impl Tensor {
    pub fn new(shape: (usize, usize)) -> Self {
        let len = shape.0 * shape.1;
        Tensor {
            shape,
            data: vec![0.0; len],
        }
    }

    pub fn from_row_major(shape: (usize, usize), data: Vec<f32>) -> Option<Self> {
        if data.len() != shape.0 * shape.1 {
            return None;
        }
        Some(Tensor { shape, data })
    }

    pub fn matmul(&self, other: &Tensor) -> Option<Tensor> {
        let (m, k) = self.shape;
        let (k2, n) = other.shape;
        if k != k2 {
            return None;
        }
        let mut result = Tensor::new((m, n));
        for i in 0..m {
            for j in 0..n {
                let mut sum = 0.0_f32;
                for t in 0..k {
                    sum += self.data[i * k + t] * other.data[t * n + j];
                }
                result.data[i * n + j] = sum;
            }
        }
        Some(result)
    }

    #[allow(dead_code)]
    pub fn add_scalar(&mut self, scalar: f32) {
        for x in self.data.iter_mut() {
            *x += scalar;
        }
    }

    #[allow(dead_code)]
    pub fn mul_scalar(&mut self, scalar: f32) {
        for x in self.data.iter_mut() {
            *x *= scalar;
        }
    }

    pub fn apply<F>(&mut self, f: F)
    where
        F: Fn(f32) -> f32,
    {
        for x in self.data.iter_mut() {
            *x = f(*x);
        }
    }

    pub fn transposed(&self) -> Self {
        let (rows, cols) = self.shape;
        let mut data = vec![0.0_f32; rows * cols];
        for i in 0..rows {
            for j in 0..cols {
                data[j * rows + i] = self.data[i * cols + j];
            }
        }
        Tensor { shape: (cols, rows), data }
    }

    pub fn add(&self, other: &Tensor) -> Option<Tensor> {
        if self.shape != other.shape { return None; }
        let mut data = self.data.clone();
        for (a, b) in data.iter_mut().zip(other.data.iter()) {
            *a += b;
        }
        Some(Tensor { shape: self.shape, data })
    }

    pub fn element_mul(&self, other: &Tensor) -> Option<Tensor> {
        if self.shape != other.shape { return None; }
        let mut data = self.data.clone();
        for (a, b) in data.iter_mut().zip(other.data.iter()) {
            *a *= b;
        }
        Some(Tensor { shape: self.shape, data })
    }
}

#[allow(dead_code)]
pub struct TernaryTensor {
    pub shape: (usize, usize),
    pub data: Vec<i8>,
}

impl TernaryTensor {
    #[allow(dead_code)]
    pub fn new(shape: (usize, usize)) -> Self {
        let len = shape.0 * shape.1;
        TernaryTensor {
            shape,
            data: vec![0_i8; len],
        }
    }

    #[allow(dead_code)]
    pub fn from_row_major(shape: (usize, usize), data: Vec<i8>) -> Option<Self> {
        if data.len() != shape.0 * shape.1 {
            return None;
        }
        Some(TernaryTensor { shape, data })
    }

    #[allow(dead_code)]
    pub fn matmul_hybrid(&self, input: &Tensor) -> Option<Tensor> {
        let (k, n) = self.shape;
        let (m, k2) = input.shape;
        if k != k2 {
            return None;
        }
        let mut result = Tensor::new((m, n));
        for i in 0..m {
            for j in 0..n {
                let mut sum = 0.0_f32;
                for t in 0..k {
                    match self.data[t * n + j] {
                        1 => sum += input.data[i * k + t],
                        -1 => sum -= input.data[i * k + t],
                        _ => {}
                    }
                }
                result.data[i * n + j] = sum;
            }
        }
        Some(result)
    }
}

pub struct PackedTernaryTensor {
    pub shape: (usize, usize),
    pub packed_data: Vec<u8>,
}

impl PackedTernaryTensor {
    fn encode_weight(v: i8) -> u8 {
        match v {
            -1 => 0b10,
            0 => 0b00,
            1 => 0b01,
            _ => 0b00,
        }
    }

    fn decode_weight(bits: u8) -> i8 {
        match bits & 0b11 {
            0b00 => 0,
            0b01 => 1,
            0b10 => -1,
            _ => 0,
        }
    }

    pub fn pack_weights(weights: &[i8]) -> Vec<u8> {
        let packed_len = (weights.len() + 3) / 4;
        let mut packed = vec![0u8; packed_len];
        for (i, &w) in weights.iter().enumerate() {
            let byte_idx = i / 4;
            let bit_pos = (i % 4) * 2;
            packed[byte_idx] |= Self::encode_weight(w) << bit_pos;
        }
        packed
    }

    pub fn get_weight(&self, index: usize) -> i8 {
        let byte_idx = index / 4;
        let bit_pos = (index % 4) * 2;
        let bits = (self.packed_data[byte_idx] >> bit_pos) & 0b11;
        Self::decode_weight(bits)
    }

    pub fn matmul_hybrid(&self, input: &Tensor) -> Option<Tensor> {
        let (k, n) = self.shape;
        let (m, k2) = input.shape;
        if k != k2 {
            return None;
        }
        let mut result = Tensor::new((m, n));
        for i in 0..m {
            for j in 0..n {
                let mut sum = 0.0_f32;
                for t in 0..k {
                    let w = self.get_weight(t * n + j);
                    match w {
                        1 => sum += input.data[i * k + t],
                        -1 => sum -= input.data[i * k + t],
                        _ => {}
                    }
                }
                result.data[i * n + j] = sum;
            }
        }
        Some(result)
    }
}

const CODEBOOK_SIZE: usize = 16;

pub struct CodebookVQ {
    pub codebook: Vec<f32>,
    pub codes: Vec<u8>,
}

impl CodebookVQ {
    pub fn train(data: &[f32], size: usize) -> Vec<f32> {
        let mut cb = vec![0.0f32; size];
        let step = data.len() / size;
        for i in 0..size {
            let start = i * step;
            let end = (i + 1) * step;
            cb[i] = data[start..end.min(data.len())].iter().sum::<f32>() / (end - start).max(1) as f32;
        }
        cb
    }

    pub fn new(data: &[f32]) -> Self {
        let codebook = Self::train(data, CODEBOOK_SIZE);
        let mut codes = Vec::with_capacity(data.len());
        for &v in data {
            let mut best = 0;
            let mut best_d = (v - codebook[0]).abs();
            for (j, &c) in codebook.iter().enumerate().skip(1) {
                let d = (v - c).abs();
                if d < best_d { best_d = d; best = j; }
            }
            codes.push(best as u8);
        }
        CodebookVQ { codebook, codes }
    }

    pub fn compress(&self) -> &[u8] { &self.codes }

    pub fn decompress(&self) -> Vec<f32> {
        self.codes.iter().map(|&c| self.codebook[c as usize]).collect()
    }

    pub fn ratio(&self) -> f32 {
        (self.codes.len() as f32 * core::mem::size_of::<u8>() as f32)
            / (self.codes.len() as f32 * core::mem::size_of::<f32>() as f32)
    }
}

pub fn quantize_to_packed(tensor: &Tensor, threshold: f32) -> PackedTernaryTensor {
    let mut ternary = Vec::with_capacity(tensor.data.len());
    for &val in tensor.data.iter() {
        let q = if val > threshold {
            1_i8
        } else if val < -threshold {
            -1_i8
        } else {
            0_i8
        };
        ternary.push(q);
    }
    let packed = PackedTernaryTensor::pack_weights(&ternary);
    PackedTernaryTensor {
        shape: tensor.shape,
        packed_data: packed,
    }
}
