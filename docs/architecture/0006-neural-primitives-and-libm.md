# ADR-0006: Neural Primitives and libm

**Status:** Accepted  
**Date:** 2026-06-21  
**Driver:** Necessidade de funções matemáticas não-lineares (SiLU, RMSNorm) em ambiente `no_std` para processamento de camadas Transformer.

## Context

LLMs usam operações não-lineares em cada camada do Transformer. As principais são:

| Operação | Fórmula | Função necessária |
|---|---|---|
| **SiLU** (Sigmoid Linear Unit) | `x / (1 + e^{-x})` | `expf(-x)` |
| **RMSNorm** (Root Mean Square Normalization) | `x_i / sqrt(mean(x²) + eps) * w` | `sqrtf` |

Em `no_std`, a crate `core` não expõe funções de ponto flutuante como `expf` ou `sqrtf` — elas ficam em `std` (que depende do sistema operacional). Para bare-metal, precisamos de uma implementação em software.

## Decision

Adicionar `libm = "0.2"` como dependência.

`libm` é a implementação de referência das funções matemáticas do `libc` (math.h) em Rust puro — sem dependência de sistema operacional. Ela fornece:

| Função | Equivalente C | Uso |
|---|---|---|
| `libm::expf(x)` | `expf(x)` | SiLU: `x / (1.0 + expf(-x))` |
| `libm::sqrtf(x)` | `sqrtf(x)` | RMSNorm: `sqrt(mean_sq + eps)` |

### Código (nn.rs)

```rust
pub fn silu(x: f32) -> f32 {
    x / (1.0 + libm::expf(-x))
}

pub fn rms_norm(tensor: &mut Tensor, weight: f32, eps: f32) {
    let sq_sum: f32 = tensor.data.iter().map(|x| x * x).sum();
    let rms = libm::sqrtf(sq_sum / tensor.data.len() as f32 + eps);
    for x in tensor.data.iter_mut() {
        *x = *x / rms * weight;
    }
}
```

### Integração

`nn::silu` é passada como closure para `Tensor::apply`:

```rust
tensor.apply(nn::silu);
nn::rms_norm(&mut tensor, 1.0, 1e-6);
```

### Validação (QEMU)

```
[TEST] SiLU([-1, 0, 1]) = [-0.26894143, 0.0, 0.7310586]
[TEST] RMSNorm(SiLU(...), weight=1.0) = [-0.59800255, 0.0, 1.6255394]
```

SiLU esperado: `[-1/(1+e¹), 0, 1/(1+e⁻¹)] = [-0.269, 0, 0.731]` ✅  
RMSNorm: `mean_sq = 0.2023, rms = 0.4498, [-0.269, 0, 0.731]/0.4498 = [-0.598, 0, 1.626]` ✅

## Consequences

**Positive:**
- `expf`, `sqrtf`, `sinf`, `cosf`, `tanhf` disponíveis para camadas futuras (GELU, LayerNorm, RoPE)
- Zero dependência de sistema operacional — `libm` é `no_std` puro
- `Tensor::apply<F>` genérico — qualquer ativação pode ser injetada

**Negative:**
- Desempenho de software FP (sem hardware acelerado) — não crítico para prototipação em QEMU
- `libm` v0.2.16 adiciona ~100KB ao binário (otimizado pelo LTO em release)

**Risks:**
- Nenhum — `libm` é amplamente usada na comunidade embedded Rust

## Alternatives Considered

1. **Micro-optimização manual com `unsafe`** — Implementar `expf` via série de Taylor ou Pade. Precisão menor e código maior. `libm` é a implementação de referência do musl/newlib.
2. **`micromath`** — Alternativa mais leve, mas não tem `expf` para `f32` no Transformer range.
3. **`fixed` point arithmetic** — Sem FPU, mas já ativamos SSE (Sprint 5), então `f32` nativo é melhor.

## References

- `libm` crate: https://crates.io/crates/libm
- SiLU (Sigmoid Linear Unit): https://arxiv.org/abs/1702.03118
- RMSNorm: https://arxiv.org/abs/1910.07467
