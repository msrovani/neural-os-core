//! Int8Router — Roteador MoE em INT8 (lei 2).
//!
//! O roteador **não** usa BitNet/b1.58 — seus pesos são INT8 para preservar a
//! granularidade necessária ao cálculo de softmax. A saída são scores por expert
//! (f32), sobre os quais `top_k` seleciona os especialistas ativos.
//!
//! # Arquitetura
//! - `weight: Vec<i8>` — (in_features × num_experts), row-major
//! - `bias: Vec<i32>` — (num_experts), termo de viés em i32 (acumulado no dot)
//! - `forward(x) → Vec<f32>` — INT8 dot product + bias → softmax
//!
//! # Uso
//! ```ignore
//! let router = Int8Router::new(hidden, num_experts);
//! let scores = router.forward(&hidden_state);
//! let top = Int8Router::top_k(&scores, 2);
//! ```

use alloc::vec::Vec;
use alloc::vec;

/// Roteador INT8 para Mixture of Experts.
/// Pesos lineares em INT8 (não BitNet) para softmax precisa.
pub struct Int8Router {
    pub weight: Vec<i8>,         // (in_features, num_experts), row-major
    pub bias: Vec<i32>,          // (num_experts)
    pub in_features: usize,
    pub num_experts: usize,
}

impl Int8Router {
    /// Cria roteador com pesos zero e bias zero.
    pub fn new(in_features: usize, num_experts: usize) -> Self {
        Self {
            weight: vec![0i8; in_features * num_experts],
            bias: vec![0i32; num_experts],
            in_features,
            num_experts,
        }
    }

    /// Cria roteador a partir de pesos INT8 e bias i32 pré-definidos.
    pub fn from_parts(weight: Vec<i8>, bias: Vec<i32>, in_features: usize, num_experts: usize) -> Self {
        assert_eq!(weight.len(), in_features * num_experts);
        assert_eq!(bias.len(), num_experts);
        Self { weight, bias, in_features, num_experts }
    }

    /// Forward: INT8 dot product + bias → Vec<f32> scores.
    ///
    /// score[e] = Σⱼ weight[j * num_experts + e] * x[j] + bias[e]
    /// Onde j percorre in_features, x é o hidden state de entrada.
    ///
    /// Usa i32 para acumulação (sem overflow para dimensões típicas < 4096).
    pub fn forward(&self, x: &[f32]) -> Vec<f32> {
        let n = self.num_experts;
        let d = self.in_features;
        assert_eq!(x.len(), d, "Int8Router::forward: input len != in_features");

        let mut scores = vec![0.0f32; n];

        for e in 0..n {
            let mut acc = 0i32;
            for j in 0..d {
                // weight[j * n + e] * x[j] em INT8 × f32 → i32
                let w = self.weight[j * n + e] as i32;
                let xq = (x[j] * 127.0) as i32; // quantiza f32 → i8 range
                acc += w * xq;
            }
            acc += self.bias[e];
            // de-quantiza de volta para f32
            scores[e] = acc as f32 / (127.0 * 127.0);
        }

        // softmax
        Self::softmax(&mut scores);
        scores
    }

    /// Softmax in-place sobre um slice de scores.
    /// Subtrai o máximo para estabilidade numérica.
    pub fn softmax(scores: &mut [f32]) {
        let n = scores.len();
        if n == 0 { return; }

        // max para estabilidade
        let mut max_val = scores[0];
        for &s in scores.iter() { if s > max_val { max_val = s; } }

        // exp(x_i - max) e soma
        let mut sum = 0.0f32;
        for s in scores.iter_mut() {
            *s = libm::expf(*s - max_val);
            sum += *s;
        }
        if sum > 0.0 {
            for s in scores.iter_mut() {
                *s /= sum;
            }
        }
    }

    /// Retorna os índices dos top-k maiores scores (ordenados descending).
    /// Se k > scores.len(), retorna todos.
    pub fn top_k(scores: &[f32], k: usize) -> Vec<usize> {
        let n = scores.len();
        let k = k.min(n);
        if k == 0 { return Vec::new(); }

        // Coleta índices
        let mut indices: Vec<usize> = (0..n).collect();
        // Selection sort parcial para top-k (ponytail: n é pequeno, tipicamente ≤ 256 experts)
        for i in 0..k {
            let mut best = i;
            for j in i + 1..n {
                if scores[indices[j]] > scores[indices[best]] {
                    best = j;
                }
            }
            indices.swap(i, best);
        }
        indices.truncate(k);
        indices
    }

    /// Carga útil: número total de bytes dos pesos + bias.
    pub fn nbytes(&self) -> usize {
        self.weight.len() + self.bias.len() * 4
    }
}

// ─── Self-test ───
pub fn self_test() -> bool {
    // 2 experts, 3 features
    let weight = vec![
        1i8, 2,   // expert 0: [+1, +1], expert 1: [+2, -1]
        -1, 1,    // (continuação para 3 features × 2 experts = 6 pesos)
        1, -1,
    ];
    let bias = vec![0i32, 0];
    let router = Int8Router::from_parts(weight, bias, 3, 2);

    let input = vec![1.0, 0.5, -0.5];
    let scores = router.forward(&input);

    // scores deve ter 2 elementos (softmax)
    assert_eq!(scores.len(), 2, "scores len");
    // Soma dos scores ≈ 1.0 (softmax)
    let sum: f32 = scores.iter().sum();
    assert!((sum - 1.0).abs() < 1e-4, "softmax sum={}", sum);

    // top-k
    let top = Int8Router::top_k(&scores, 1);
    assert_eq!(top.len(), 1, "top_k len");
    assert!(top[0] < 2, "top_k index out of bounds");

    // softmax vazia não quebra
    Int8Router::softmax(&mut []);
    Int8Router::softmax(&mut [1.0]);

    true
}
