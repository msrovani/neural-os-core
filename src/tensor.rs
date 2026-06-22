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
}

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

    pub fn from_row_major(shape: (usize, usize), data: Vec<i8>) -> Option<Self> {
        if data.len() != shape.0 * shape.1 {
            return None;
        }
        Some(TernaryTensor { shape, data })
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
