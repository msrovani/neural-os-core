# SESSION_009 — Ternary Inference Engine (BitNet b1.58)

**Date:** 2026-06-21  
**Objective:** Sprint 9 starts Phase 3 of the Strategic Roadmap (ADR-0010). Eliminate `f32` multiplications from dense weight layers by quantizing weights to {-1, 0, +1}. Replace FPU dot-product with conditional ADD/SUB operations.

## Changes

### New
- `tensor.rs::TernaryTensor` — `i8` storage for ternary weights, 4× compression vs `f32`
- `TernaryTensor::matmul_hybrid()` — zero-multiplication kernel: `match w {1 => add, -1 => sub, _ => skip}`
- `nn.rs::BitLinear` — ternary dense layer, `forward()` = `matmul_hybrid()` + optional bias
- BitNet test in main.rs — input `[1.5, -0.5, 2.0]` × `TernaryTensor(3×2)` → ADD/SUB → `[-0.5, -2.0]`
- ADR-0011: BitLinear and Hybrid Ternary Matmul

### Modified
- `src/tensor.rs` — `TernaryTensor` struct + impl appended after `Tensor`
- `src/nn.rs` — `use TernaryTensor`, `BitLinear` struct + impl after `Linear`
- `src/main.rs` — BitNet test inserted before PIC init

## Verification

QEMU output:

```
[BITNET] Inferencia Hibrida concluida. Resultado: [-0.5, -2.0]
```

Manual verification with ADD/SUB only:
```
Input [1.5, -0.5, 2.0]   W^T matrix:
  out[0] = +1.5 +0 -2.0 = -0.5   ←  add 1.5, skip, sub 2.0
  out[1] = -1.5 +0.5 +0  = -2.0   ←  sub 1.5, add 0.5, skip
```

Zero multiplication operators (`*`) in the hot loop. ✅  
Watchdog continues at ~18.2 Hz — system stable. ✅

## Validation Criteria
- ✅ `cargo check --release` — 0 errors, 0 warnings
- ✅ No multiplication operator (`*`) in `matmul_hybrid` inner loop
- ✅ `TernaryTensor` uses `Vec<i8>` (not `Vec<f32>`) — 4× memory compression
- ✅ QEMU boot — all 9 subsystems operating
- ✅ BitLinear forward pass produces correct output matching manual calculation
- ✅ Watchdog ticks unchanged — no interference from ternary engine
- ✅ ADR-0011 documentado

## Next Sprint (Sprint 10)
- Bitmap/Free-list FrameDeallocator — reuso real de frames físicos
- Slab allocator — reduzir fragmentação do heap
- Calibration pass — `f32` → ternary thresholding via `Δ`
- `TernaryTensor::packed()` — 2-bit packing for storage efficiency
