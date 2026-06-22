# SESSION_007 — Intent Router MLP

**Date:** 2026-06-21  
**Objective:** Estruturar as primitivas neurais em uma MLP real (camada Linear + ativação + argmax) que atua como roteador de intenção do AIOS, substituindo o escalonador de tarefas tradicional.

## Changes

### New
- `tensor.rs:transposed()` — reordena dados row-major para column-major (W^T)
- `nn.rs:Linear` struct — `weights: Tensor`, `bias: Option<Tensor>`
- `nn.rs:Linear::forward(&self, input) -> Tensor` — implementa Y = X·W^T + B
- `nn.rs:argmax(tensor) -> usize` — índice do maior valor (decisão final)

### Modified
- `src/main.rs` — Intent Router test: emb 1×3 → Linear(3→2) → SiLU → argmax → print

## Verification

QEMU output (serial):

```
[ROUTER] Intencao processada. Acao escolhida: 0 (0=Daemon, 1=Halt)
```

Input: `[1.0, -0.5, 0.3]`
Weights (2×3):
- W[0]: `[1.0, 0.0, 1.0]` → prioriza urgencia positiva
- W[1]: `[-1.0, 0.0, -1.0]` → prioriza urgencia negativa

Raw logits: `[1.3, -1.3]` → SiLU → `[1.022, -0.279]` → argmax = **0 (Daemon)** ✅

## Validation Criteria
- ✅ `cargo check --release` — 0 errors, 0 warnings
- ✅ QEMU boot — all 7 subsystems operational
- ✅ ADR-0007 documentado com arquitetura do Córtex Primitivo
- ✅ Heap allocation do Linear::forward (W^T + matmul output) cabe nos 100 KB

## Next Sprint (Sprint 8)
- PIC remap (8259A) — reencaminhar IRQs de hardware para vetores ≥ 32
- PIT timer handler — interrupção periódica para preempção
- Page Fault handler — capturar e tratar `#PF`
- Implement `FrameDeallocator` para reuso de frames
