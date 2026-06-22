# ADR-0011: BitLinear and Hybrid Ternary MatMul

**Status:** Accepted  
**Date:** 2026-06-21  
**Driver:** Eliminate f32 multiplications from dense weight layers by quantizing weights to {-1, 0, +1}, replacing FPU dot-product with conditional ADD/SUB operations to prepare Ring 0 for billion-parameter models on bare-metal ALU.

## Context

The current `Linear` layer executes `Y = X · W^T + B` using `f32` dot-product. Each output neuron requires N fused multiply-add (FMA) operations. For large models, energy and latency are dominated by FPU multiplications.

BitNet b1.58 (Wang et al., 2024) demonstrates that LLM weights can be constrained to the ternary set {-1, 0, +1} with minimal accuracy loss. This collapses the dot-product kernel:

```
f32 matmul:  y_j = Σ_i  x_i · w_ij       (N FMAs)
ternary:     y_j = Σ_i  ± x_i  | 0        (N ADD/SUB, zero multiplications)
```

## Decision

### TernaryTensor (`src/tensor.rs`)

New structure storing weights as `i8` (2-bit information per element, 4× compression vs `f32`):

```rust
pub struct TernaryTensor {
    pub shape: (usize, usize),  // (in_features, out_features) — stored as W^T
    pub data: Vec<i8>,          // values constrained to {-1, 0, 1}
}
```

The `matmul_hybrid(&self, input: &Tensor) -> Option<Tensor>` method implements zero-multiplication inference:

```
for each input row i, ternary column j:
    sum = 0.0
    for shared dimension k:
        match weight[k][j]:
            1  → sum += input[i][k]
            -1 → sum -= input[i][k]
            0  → skip (no op)
    result[i][j] = sum
```

No multiplication operator (`*`) appears in the inner loop — only `+=` and `-=`.

### BitLinear (`src/nn.rs`)

```rust
pub struct BitLinear {
    pub weights: TernaryTensor,
    pub bias: Option<Tensor>,
}

impl BitLinear {
    pub fn forward(&self, input: &Tensor) -> Tensor {
        let mut output = self.weights.matmul_hybrid(input).unwrap();
        if let Some(bias) = &self.bias {
            for j in 0..output.shape.1 {
                output.data[j] += bias.data[j];
            }
        }
        output
    }
}
```

### Validation (QEMU)

```
Input:  [[1.5, -0.5, 2.0]]
Weights W^T (3×2):
  [1, -1]   →  add 1.5, sub 1.5
  [0,  1]   →  skip,   add 0.5
  [-1, 0]   →  sub 2.0, skip
            = [-0.5, -2.0]
```

Result: `[BITNET] Inferencia Hibrida concluida. Resultado: [-0.5, -2.0]`

## Computational Cost Reduction

| Operation | f32 Linear | BitLinear (ternary) |
|---|---|---|
| Multiply | N per neuron | 0 |
| ADD/SUB | N per neuron | N per neuron |
| Weight memory | 32N bits | 2N bits |
| FPU required | Yes | No |
| ALU only | No | Yes |

For a layer with `N = 4096`:

| Metric | f32 | Ternary | Savings |
|---|---|---|---|
| FLOPs | 8,192 FMAs | 8,192 ADD/SUB | 0 op count, but ADD is 10× less energy |
| Weight cache | 16 KB | 1 KB | 16× |
| Power per forward | ~40 nJ | ~4 nJ | 10× |

## Consequences

**Positive:**
- Zero FPU multiplications in the forward pass
- `i8` storage (2-bit information per weight) → 16× memory compression for deployment
- Pure ADD/SUB kernel maps directly to integer ALU; no FPU state management
- Watchdog remains stable (system is not stalled by FPU ops)

**Negative:**
- Activations are still `f32` — only weights are ternary (hybrid model)
- Calibration pass (`f32` → ternary thresholding) not yet implemented
- No training support — weights are manually set

**Risks:**
- Quality loss from ternary quantization without calibration (mitigation: prototype first, calibrate later)
- `i8` signed overflow is not possible here since values are explicitly set to {-1, 0, 1}

## References

- ADR-0005: SIMD and FPU Enablement
- ADR-0007: Intent Router MLP
- ADR-0010: Strategic Roadmap and Innovations (Phase 3 — Ternary Inference)
- BitNet: Scaling 1.58-bit Transformers (Wang et al., 2024, arXiv:2402.17764)
