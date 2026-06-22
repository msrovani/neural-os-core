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
}
