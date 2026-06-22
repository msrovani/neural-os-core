# ADR-0005: SIMD and FPU Enablement

**Status:** Accepted  
**Date:** 2026-06-21  
**Driver:** Necessidade de operações com ponto flutuante (`f32`/`f64`) e instruções SIMD para processamento de tensores e pesos de LLM no Ring 0.

## Context

O microkernel neural precisa executar operações matemáticas com `f32` para:
1. Multiplicação de matrizes (matmul) entre tensores
2. Processamento de pesos de modelos de IA (tipicamente `f16`/`f32`)
3. Operações algébricas lineares no Tensor Engine

Sem a FPU e SSE habilitadas, qualquer instrução de ponto flutuante dispara uma exceção `#NM` (Device Not Available — vector 7), tornando impossível o cálculo matemático.

## Decision

### Registradores de Controle

Dois registradores de controle da CPU precisam ser configurados:

#### CR0 (Control Register 0)

| Flag | Bit | Ação | Efeito |
|---|---|---|---|
| `EMULATE_COPROCESSOR` | 2 | **Clear** | Desabilita emulação de FPU; instruções x87/MMX/SSE executam nativamente |
| `MONITOR_COPROCESSOR` | 1 | **Set** | Habilita monitoramento do coprocessador para x87 |
| `NUMERIC_ERROR` | 5 | **Set** | Habilita relatório nativo de erro de ponto flutuante |

#### CR4 (Control Register 4)

| Flag | Bit | Ação | Efeito |
|---|---|---|---|
| `OSFXSR` | 9 | **Set** | Habilita SSE instructions e `FXSAVE`/`FXRSTOR` para salvar/restaurar estado SIMD |
| `OSXMMEXCPT_ENABLE` | 10 | **Set** | Habilita exceção `#XF` para erros SIMD de ponto flutuante não mascarados |

### Código

```rust
pub fn enable_simd() {
    unsafe {
        Cr0::update(|flags| {
            flags.remove(Cr0Flags::EMULATE_COPROCESSOR);
            flags.insert(Cr0Flags::MONITOR_COPROCESSOR);
            flags.insert(Cr0Flags::NUMERIC_ERROR);
        });
    }
    unsafe {
        Cr4::update(|flags| {
            flags.insert(Cr4Flags::OSFXSR);
            flags.insert(Cr4Flags::OSXMMEXCPT_ENABLE);
        });
    }
}
```

O método `Cr0::update()` / `Cr4::update()` da crate `x86_64` lê o registrador atual, aplica a closure de modificação e escreve de volta — preservando bits reservados.

### Tensor Engine

Com SIMD ativo, implementamos `Tensor` (matriz 2D) com `Vec<f32>` no heap:

```
Tensor { shape: (m, n), data: Vec<f32> }
    └─ matmul(&self, other) → Option<Tensor>
        └─ dot product linear: O(m * n * k)
        └─ retorna None se shape.cols != other.rows
```

Testado com multiplicação `1×3` por `3×1` → `1×1` = `[32.0]`.

## Consequences

**Positive:**
- `f32` e `f64` disponíveis sem exceções
- Instruções SSE (addps, mulps, etc.) podem ser usadas pelo compilador para autovetorização
- `Tensor::matmul` funcional — base para camadas densas de redes neurais
- Código mínimo: 0 novas dependências (usa `x86_64` já existente)

**Negative:**
- Nenhum mecanismo de lazy save/restore de estado FPU — interrupções que usam FPU podem corromper estado
- Matmul O(n³) naive — sem otimização BLAS, suficiente para prototipação
- `f16` (half-precision) não é suportado nativamente sem AVX-512_FP16

**Risks:**
- `Cr0::update` escreve o CR0 completo — se interrompido no momento errado, pode causar instabilidade
- SSE não é thread-safe para estado FPU: single-core mitiga o risco

## Alternatives Considered

1. **Lazy FPU (via `CR0.TS` bit)** — Marca flag `TASK_SWITCHED` para adiar save/restore. Mais complexo e desnecessário em single-core.
2. **AVX/AVX-512** — Mais potente, mas requer `OSXSAVE` (CR4 bit 18) e `XSAVE` instruction family. Não necessário para primeira versão do Tensor Engine.

## References

- Intel SDM Vol. 1, Chapter 13: Managing State Using the FXSAVE and FXRSTOR Instructions
- Intel SDM Vol. 3, Chapter 2.5: Control Registers
- AMD APM Vol. 2, Sections 7.1–7.3: x87 FPU, SSE, Media Instructions
