# SESSION_006 — Neural Primitives and libm Dependency

**Date:** 2026-06-21  
**Objective:** Adicionar funções de ativação não-lineares (SiLU) e normalização (RMSNorm) usando `libm` para suporte matemático em `no_std`.

## Changes

### New
- `libm = "0.2"` em `Cargo.toml` — `expf`, `sqrtf` disponíveis em ambiente bare-metal
- `src/nn.rs` — `silu(x: f32) -> f32` via `x / (1.0 + libm::expf(-x))`
- `src/nn.rs` — `rms_norm(tensor: &mut Tensor, weight: f32, eps: f32)` via `libm::sqrtf`

### Modified
- `src/tensor.rs` — adicionados `add_scalar`, `mul_scalar`, `apply<F>` (closure genérica)
- `src/main.rs` — teste: tensor 1×3 `[-1, 0, 1]` → `apply(nn::silu)` → `rms_norm` → print
- `docs/memory/STATE.md` — Sprint 6 status, tabelas atualizadas
- `docs/architecture/0006-neural-primitives-and-libm.md` — ADR completo

## Verification

QEMU output:

```
[TEST] SiLU([-1, 0, 1]) = [-0.26894143, 0.0, 0.7310586]
[TEST] RMSNorm(SiLU(...), weight=1.0) = [-0.59800255, 0.0, 1.6255394]
```

SiLU esperado: `-1/(1+e¹) ≈ -0.269`, `0/(1+e⁰) = 0`, `1/(1+e⁻¹) ≈ 0.731` ✅  
RMSNorm esperado: `mean_sq = 0.2023`, `rms = 0.4498`, `[-0.269, 0, 0.731]/0.4498 ≈ [-0.598, 0, 1.626]` ✅

## Validation Criteria
- ✅ `cargo check --release` — 0 errors, 0 warnings
- ✅ QEMU boot — todos os 6 sistemas (VGA + serial + IDT + heap + SIMD + tensor + SiLU/RMSNorm)
- ✅ ADR-0006 documentado com fórmulas e expected values
- ✅ `libm` adicionado — `no_std` compatível, sem dependência de sistema operacional

## Next Sprint (Sprint 7)
- PIC remap (8259A) — reencaminhar IRQs de hardware para vetores ≥ 32
- PIT timer handler — interrupção periódica para preempção
- Page Fault handler — capturar e tratar `#PF`
- Implement `FrameDeallocator` para reuso de frames
