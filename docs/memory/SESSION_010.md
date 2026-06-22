# SESSION_010 — 2-bit Packing and Ternary Quantization

**Date:** 2026-06-21  
**Objective:** Sprint 10 closes the storage and calibration gap in Phase 3 (BitNet). Pack 4 ternary weights per byte (2-bit per weight), implement the f32→ternary calibration pass, and refactor BitLinear to use packed storage.

## Changes

### New
- `tensor.rs::PackedTernaryTensor` — stores 4 ternary weights per `u8` byte
- `pack_weights(weights: &[i8]) → Vec<u8>` — iterates in groups of 4, shifts by `(i%4)*2`, ORs into byte
- `get_weight(index: usize) → i8` — extracts 2-bit field via `(byte >> bit_pos) & 0b11` → match decode
- `tensor.rs::quantize_to_packed(tensor: &Tensor, threshold: f32) → PackedTernaryTensor` — calibration pass
- ADR-0012: 2-bit Packing and Ternary Quantization

### Modified
- `tensor.rs` — `PackedTernaryTensor` + `quantize_to_packed` appended after `TernaryTensor`
- `nn.rs` — `BitLinear` now uses `PackedTernaryTensor` (import changed from `TernaryTensor`)
- `main.rs` — BitNet test: f32 weights `[1.5, -1.8, 0.2, 2.1, -3.0, 0.0]` → quantize(0.5) → packed(2 bytes) → forward

## Verification

QEMU output:

```
[BITNET] Inferencia 2-bit concluida. Tamanho comprimido: 2 bytes. Output: [-0.5, -2.0]
```

Calibration with Δ=0.5:
```
1.5 > 0.5  →  1   |  2.1 > 0.5  →  1
-1.8 < -0.5 → -1  |  -3.0 < -0.5 → -1
0.2 in [-0.5,0.5] → 0  |  0.0 in [-0.5,0.5] → 0
```

Packing 6 weights→2 bytes: `[1,-1,0,1,-1,0]` → `ceil(6/4)=2 bytes` ✅  
Forward: input `[1.5, -0.5, 2.0]` × quantized weights → same result as Sprint 9 ✅  
Compression: 24 bytes (f32) → 2 bytes (packed) = **12×** ✅

## Validation Criteria
- ✅ `cargo check --release` — 0 errors, 0 warnings
- ✅ Bitwise operations correct: `encode_weight(-1) = 0b10`, `decode_weight(0b10) = -1`
- ✅ Non-multiple-of-4 padded silently with zeros in unused bits
- ✅ `PackedTernaryTensor` replaces `TernaryTensor` in `BitLinear` — no functional regression
- ✅ ADR-0012 documentado

## Next Sprint (Sprint 11)
- Bitmap FrameDeallocator
- Slab allocator
- Benchmark ternary vs f32 performance
