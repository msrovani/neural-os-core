# ADR-0007: Intent Router MLP — Primeiro Córtex Primitivo

**Status:** Accepted  
**Date:** 2026-06-21  
**Driver:** Substituir o escalonador de tarefas legado (baseado em interrupções de software) por uma rede neural MLP que decide qual ação o kernel deve tomar com base em um embedding de intenção.

## Context

Sistemas operacionais tradicionais usam escalonamento por prioridade fixa, round-robin ou CFS (Completely Fair Scheduler). Estas abordagens são determinísticas e não adaptativas: um processo "importante" tem prioridade fixa, independentemente do contexto semântico.

O AIOS propõe um **Roteador de Intenção** baseado em MLP:

1. Cada "solicitação ao kernel" é convertida em um embedding vetorial de intenção (ex: `[urgencia, tipo_carga, contexto]`)
2. O embedding passa por uma camada `Linear` (densa) com ativação SiLU
3. `argmax` sobre os logits de saída seleciona a ação do kernel

### Arquitetura da Camada Linear

```
Y = X · W^T + B
```

Onde:
- `X` é o embedding de entrada (shape: `1 × N`)
- `W` são os pesos da camada (shape: `M × N`), armazenados em row-major
- `W^T` é a transposição de `W` (shape: `N × M`), calculada por `Tensor::transposed()`
- `B` é o bias opcional (shape: `1 × M`)
- `Y` são os logits de saída (shape: `1 × M`)

### Transposição de Tensor

`transposed()` reordena os dados de row-major para column-major:

```
data_original[i * cols + j] → data_transposta[j * rows + i]
```

### Forward Pass Completo

Para um embedding `[1.0, -0.5, 0.3]` com pesos `[[1, 0, 1], [-1, 0, -1]]`:

1. `W^T` = `[[1, -1], [0, 0], [1, -1]]`
2. `X · W^T` = `[1.3, -1.3]`
3. `SiLU([1.3, -1.3])` = `[1.022, -0.279]`
4. `argmax` = 0 → **Acionar Daemon Ring 2**

## Decision

### Tensor (tensor.rs)

- Adicionar `transposed() -> Self` — aloca novo `Vec<f32>` e reordena dados sem modificar o tensor original
- `matmul` permanece inalterado — a transposição é explícita no call site

### Linear layer (nn.rs)

```rust
pub struct Linear {
    pub weights: Tensor,          // shape: (out_features, in_features)
    pub bias: Option<Tensor>,     // shape: (1, out_features)
}

impl Linear {
    pub fn forward(&self, input: &Tensor) -> Tensor {
        let w_t = self.weights.transposed();
        let mut output = input.matmul(&w_t).expect("shape mismatch");
        if let Some(ref bias) = self.bias {
            for j in 0..output.shape.1 {
                output.data[j] += bias.data[j];
            }
        }
        output
    }
}
```

### argmax (nn.rs)

Percorre o vetor de dados e retorna o índice do maior valor.

### Roteador de Intenção (main.rs)

O embedding e os pesos são hardcoded para prototipação. Em produção, o embedding virá do Ring 0 (NPU) e os pesos serão carregados de contexto de memória semântica.

## Consequences

**Positive:**
- Substitui escalonamento baseado em interrupção por decisão neural
- MLP extensível: mais neurônios → mais ações, mais camadas → decisões hierárquicas
- `argmax` é $O(n)$, trivial em bare-metal
- `transposed()` é $O(n \cdot m)$, executado uma vez por forward pass

**Negative:**
- Pesos hardcoded — sem treinamento ainda (futuro: carregar de armazenamento semântico)
- Heap allocation dupla por forward pass (`transposed()` + `matmul()` = 2 novos `Vec<f32>`)

**Risks:**
- Heap de 100 KB é suficiente para MLPs pequenos (1×3 → 1×2 usa ~50 bytes)
- MLP com muitas camadas pode estourar o heap — mitigação: aumentar heap ou usar allocação estática

## Alternatives Considered

1. **Multiplicação in-place sem transposição** — Pesos armazenados em formato transposto. Economiza allocation mas quebra a semântica `W^T` da equação.
2. **Tabela de decisão (if/else)** — Simples, mas não escalável para embeddings de alta dimensão.

## Future Work

- Carregar pesos do sistema de arquivos semântico
- Múltiplas camadas `Linear` empilhadas (MLP profundo)
- Embedding extraído diretamente de requisição de usuário via Ring 0

## References

- ADR-0005: SIMD and FPU Enablement
- ADR-0006: Neural Primitives and libm
