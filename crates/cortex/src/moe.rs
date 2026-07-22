use alloc::vec::Vec;
use alloc::vec;
use crate::nn::BitLinear;
use crate::tensor::{Tensor, PackedTernaryTensor};

pub struct Int8Router {
    pub weight: Vec<i8>,
    pub bias: Vec<i32>,
    pub in_features: usize,
    pub num_experts: usize,
}

impl Int8Router {
    pub fn new(in_features: usize, num_experts: usize) -> Self {
        Int8Router {
            weight: vec![0i8; in_features * num_experts],
            bias: vec![0i32; num_experts],
            in_features,
            num_experts,
        }
    }

    pub fn from_parts(weight: Vec<i8>, bias: Vec<i32>, in_features: usize, num_experts: usize) -> Self {
        Int8Router { weight, bias, in_features, num_experts }
    }

    pub fn forward(&self, x: &[f32]) -> Vec<f32> {
        let n = self.num_experts;
        let d = self.in_features;
        let mut scores = vec![0.0f32; n];
        for e in 0..n {
            let mut dot = 0i32;
            for j in 0..d {
                dot += self.weight[j * n + e] as i32 * (x[j] as i32);
            }
            scores[e] = (dot + self.bias[e]) as f32;
        }
        scores
    }

    pub fn top_k(scores: &[f32], k: usize) -> Vec<usize> {
        let k = k.min(scores.len());
        let mut indices: Vec<usize> = (0..scores.len()).collect();
        indices.sort_by(|&a, &b| scores[b].partial_cmp(&scores[a]).unwrap_or(core::cmp::Ordering::Equal));
        indices.truncate(k);
        indices
    }

    pub fn nbytes(&self) -> usize {
        self.weight.len() + self.bias.len() * 4
    }
}

pub struct MoEConfig {
    pub num_experts: usize,
    pub top_k: usize,
    pub hidden: usize,
}

impl MoEConfig {
    pub const fn new(num_experts: usize, top_k: usize, hidden: usize) -> Self {
        MoEConfig { num_experts, top_k, hidden }
    }
}

pub struct MoELayer {
    pub config: MoEConfig,
    pub shared_expert: BitLinear,
    pub router: Int8Router,
    pub experts: Vec<BitLinear>,
}

impl MoELayer {
    pub fn new(config: MoEConfig, shared: BitLinear, router: Int8Router, experts: Vec<BitLinear>) -> Self {
        MoELayer { config, shared_expert: shared, router, experts }
    }

    pub fn forward(&self, x: &Tensor) -> Tensor {
        let shared_out = self.shared_expert.forward(x);
        let scores = self.router.forward(&x.data);
        let indices = Int8Router::top_k(&scores, self.config.top_k);
        let mut routed_out = Tensor::new(shared_out.shape);
        for &idx in indices.iter() {
            if idx >= self.experts.len() { continue; }
            let expert_out = self.experts[idx].forward(x);
            for j in 0..routed_out.data.len().min(expert_out.data.len()) {
                routed_out.data[j] += expert_out.data[j];
            }
        }
        let mut output = Tensor::new(shared_out.shape);
        for j in 0..output.data.len() {
            output.data[j] = shared_out.data[j] + routed_out.data[j];
        }
        output
    }

    pub fn forward_sequence(&self, x: &Tensor) -> Tensor {
        let (seq_len, hidden) = x.shape;
        let mut output = Tensor::new((seq_len, hidden));
        for s in 0..seq_len {
            let start = s * hidden;
            let token_data = x.data[start..start + hidden].to_vec();
            let token_t = Tensor::from_row_major((1, hidden), token_data)
                .expect("forward_sequence: slice");
            let token_out = self.forward(&token_t);
            for j in 0..hidden {
                output.data[start + j] = token_out.data[j];
            }
        }
        output
    }
}

pub struct DynamicMoE {
    pub base: MoELayer,
    pub expert_hits: Vec<u64>,
    pub expert_confidence: Vec<f32>,
    pub expert_entropy: Vec<f32>,
    pub pending_births: Vec<BitLinear>,
    pub pending_merges: Vec<(usize, usize)>,
    pub pending_splits: Vec<usize>,
}

impl DynamicMoE {
    pub fn new(base: MoELayer) -> Self {
        let n = base.experts.len();
        DynamicMoE {
            base,
            expert_hits: vec![0; n],
            expert_confidence: vec![0.0; n],
            expert_entropy: vec![0.0; n],
            pending_births: Vec::new(),
            pending_merges: Vec::new(),
            pending_splits: Vec::new(),
        }
    }

    pub fn record_hit(&mut self, idx: usize, confidence: f32) {
        if idx < self.expert_hits.len() {
            self.expert_hits[idx] += 1;
            let n = self.expert_hits[idx] as f32;
            self.expert_confidence[idx] += (confidence - self.expert_confidence[idx]) / n;
        }
    }

    pub fn update_entropy(&mut self, idx: usize, entropy: f32) {
        if idx < self.expert_entropy.len() {
            self.expert_entropy[idx] = entropy;
        }
    }

    pub fn queue_birth(&mut self, expert: BitLinear) {
        self.pending_births.push(expert);
    }

    pub fn queue_merge(&mut self, i: usize, j: usize) {
        if i != j && i < self.base.experts.len() && j < self.base.experts.len() {
            self.pending_merges.push((i, j));
        }
    }

    pub fn queue_split(&mut self, idx: usize) {
        if idx < self.base.experts.len() {
            self.pending_splits.push(idx);
        }
    }

    pub fn flush_births(&mut self) {
        for expert in self.pending_births.drain(..) {
            self.base.experts.push(expert);
            self.base.config.num_experts += 1;
            self.expert_hits.push(0);
            self.expert_confidence.push(0.0);
            self.expert_entropy.push(0.0);
        }
    }

    pub fn flush_merges(&mut self) {
        for &(i, j) in &self.pending_merges {
            if i >= self.base.experts.len() || j >= self.base.experts.len() { continue; }
            let merged = Self::merge_pair(&self.base.experts[i], &self.base.experts[j]);
            let (hi, ci, ei) = (self.expert_hits[i], self.expert_confidence[i], self.expert_entropy[i]);
            let (hj, cj, ej) = (self.expert_hits[j], self.expert_confidence[j], self.expert_entropy[j]);
            let max_i = i.max(j);
            let min_i = i.min(j);
            self.base.experts.remove(max_i);
            self.base.experts.remove(min_i);
            self.base.experts.insert(min_i, merged);
            self.base.config.num_experts -= 1;
            self.expert_hits.remove(max_i);
            self.expert_hits.remove(min_i);
            self.expert_confidence.remove(max_i);
            self.expert_confidence.remove(min_i);
            self.expert_entropy.remove(max_i);
            self.expert_entropy.remove(min_i);
            self.expert_hits.insert(min_i, (hi + hj) / 2);
            self.expert_confidence.insert(min_i, (ci + cj) / 2.0);
            self.expert_entropy.insert(min_i, (ei + ej) / 2.0);
        }
        self.pending_merges.clear();
    }

    pub fn flush_splits(&mut self) {
        for &idx in &self.pending_splits {
            if idx >= self.base.experts.len() { continue; }
            let original = &self.base.experts[idx];
            let cloned = Self::clone_with_noise(original, 0.05);
            let split_a = Self::clone_with_noise(original, -0.05);
            self.base.experts[idx] = split_a;
            self.base.experts.push(cloned);
            self.base.config.num_experts += 1;
            self.expert_hits.push(self.expert_hits[idx] / 2);
            self.expert_confidence.push(self.expert_confidence[idx]);
            self.expert_entropy.push(self.expert_entropy[idx] / 2.0);
            self.expert_hits[idx] /= 2;
            self.expert_entropy[idx] /= 2.0;
        }
        self.pending_splits.clear();
    }

    pub fn flush_all(&mut self) {
        self.flush_births();
        self.flush_merges();
        self.flush_splits();
    }

    fn merge_pair(a: &BitLinear, b: &BitLinear) -> BitLinear {
        let weights = PackedTernaryTensor {
            shape: a.weights.shape,
            packed_data: a.weights.packed_data.clone(),
        };
        let bias = match &a.bias {
            Some(ref ta) => {
                let mut avg = Tensor::new(ta.shape);
                for j in 0..avg.data.len() {
                    avg.data[j] = (ta.data[j] + b.bias.as_ref().map_or(0.0, |bt| bt.data[j])) / 2.0;
                }
                Some(avg)
            }
            None => None,
        };
        BitLinear::new(weights, bias)
    }

    fn clone_with_noise(original: &BitLinear, _noise: f32) -> BitLinear {
        let weights = PackedTernaryTensor {
            shape: original.weights.shape,
            packed_data: original.weights.packed_data.clone(),
        };
        let bias = match original.bias {
            Some(ref t) => Some(Tensor::new(t.shape)),
            None => None,
        };
        BitLinear::new(weights, bias)
    }

    pub fn stale_indices(&self, min_hits: u64) -> Vec<usize> {
        self.expert_hits.iter().enumerate()
            .filter(|(_, &h)| h < min_hits)
            .map(|(i, _)| i)
            .collect()
    }

    pub fn high_entropy_indices(&self, threshold: f32) -> Vec<usize> {
        self.expert_entropy.iter().enumerate()
            .filter(|(_, &e)| e > threshold)
            .map(|(i, _)| i)
            .collect()
    }

    pub fn self_test() -> bool {
        let hidden = 8;
        let n = 4;
        let top_k = 2;

        let make_linear = || BitLinear::new(
            PackedTernaryTensor { shape: (hidden, hidden), packed_data: vec![0u8; (hidden * hidden + 3) / 4] },
            None,
        );
        let shared = make_linear();
        let router = Int8Router::new(hidden, n);
        let mut experts = Vec::with_capacity(n);
        for _ in 0..n { experts.push(make_linear()); }

        let config = MoEConfig::new(n, top_k, hidden);
        let layer = MoELayer::new(config, shared, router, experts);
        let mut dmoe = DynamicMoE::new(layer);

        let input = Tensor::new((1, hidden));
        let output = dmoe.base.forward(&input);
        if output.shape != (1, hidden) { return false; }

        let batch = Tensor::new((3, hidden));
        let batch_out = dmoe.base.forward_sequence(&batch);
        if batch_out.shape != (3, hidden) { return false; }

        dmoe.queue_split(0);
        dmoe.flush_splits();
        if dmoe.base.experts.len() != n + 1 { return false; }

        dmoe.queue_merge(0, 1);
        dmoe.flush_merges();
        if dmoe.base.experts.len() != n { return false; }

        let extra = BitLinear::new(
            PackedTernaryTensor { shape: (hidden, hidden), packed_data: vec![0u8; (hidden * hidden + 3) / 4] },
            None,
        );
        dmoe.queue_birth(extra);
        dmoe.flush_births();
        if dmoe.base.experts.len() != n + 1 { return false; }

        true
    }
}
