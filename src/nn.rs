use crate::tensor::Tensor;
use crate::tensor::TernaryTensor;

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
            let (_, out_features) = output.shape;
            for j in 0..out_features {
                output.data[j] += bias.data[j];
            }
        }
        output
    }
}

pub struct BitLinear {
    pub weights: TernaryTensor,
    pub bias: Option<Tensor>,
}

impl BitLinear {
    pub fn new(weights: TernaryTensor, bias: Option<Tensor>) -> Self {
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

pub fn rms_norm(tensor: &mut Tensor, weight: f32, eps: f32) {
    let len = tensor.data.len() as f32;
    let sq_sum: f32 = tensor.data.iter().map(|x| x * x).sum();
    let rms = libm::sqrtf(sq_sum / len + eps);
    for x in tensor.data.iter_mut() {
        *x = *x / rms * weight;
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
