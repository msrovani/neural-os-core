use crate::tensor::PackedTernaryTensor;
use crate::tensor::Tensor;

pub struct Linear {
    pub weights: Tensor,
    pub bias: Option<Tensor>,
}

impl Linear {
    pub fn new(weights: Tensor, bias: Option<Tensor>) -> Self {
        Linear { weights, bias }
    }

    pub fn forward(&self, input: &Tensor) -> Tensor {
        let w_t = self.weights.transposed();
        let mut output = input.matmul(&w_t).expect("Linear::forward: shape mismatch");
        if let Some(ref bias) = self.bias {
            let (batch_size, out_features) = output.shape;
            for i in 0..batch_size {
                for j in 0..out_features {
                    output.data[i * out_features + j] += bias.data[j];
                }
            }
        }
        output
    }
}

pub struct BitLinear {
    pub weights: PackedTernaryTensor,
    pub bias: Option<Tensor>,
}

impl BitLinear {
    pub fn new(weights: PackedTernaryTensor, bias: Option<Tensor>) -> Self {
        BitLinear { weights, bias }
    }

    pub fn forward(&self, input: &Tensor) -> Tensor {
        let mut output = self.weights.matmul_hybrid(input)
            .expect("BitLinear::forward: shape mismatch");
        if let Some(ref bias) = self.bias {
            for j in 0..output.shape.1 {
                output.data[j] += bias.data[j];
            }
        }
        output
    }
}

pub fn silu(x: f32) -> f32 {
    x / (1.0 + libm::expf(-x))
}

/// ReLU² — ativação FFN do 2B4T (ADR-0084 M1): `max(x,0)²`. Só mul, sem exp.
pub fn relu2(x: f32) -> f32 {
    let r = if x > 0.0 { x } else { 0.0 };
    r * r
}

pub fn rms_norm(tensor: &mut Tensor, weight: &[f32], eps: f32) {
    if weight.is_empty() { return; }
    let len = tensor.data.len() as f32;
    let sq_sum: f32 = tensor.data.iter().map(|x| x * x).sum();
    let rms = libm::sqrtf(sq_sum / len + eps);
    for (i, x) in tensor.data.iter_mut().enumerate() {
        let w = weight[i % weight.len()];
        *x = *x / rms * w;
    }
}

pub fn argmax(tensor: &Tensor) -> usize {
    let mut max_idx = 0;
    for i in 1..tensor.data.len() {
        if tensor.data[i] > tensor.data[max_idx] {
            max_idx = i;
        }
    }
    max_idx
}
