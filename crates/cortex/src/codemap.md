# crates/cortex/src/

`no_std` Ring 2 (R2) intelligence layer — BitNet ternary LLM inference, Trinity MoE routing, tensors, speculative/structured decoding, GGUF loading, compute dispatch. 32 `.rs` files; entry `lib.rs` re-exports all modules.

See the full map at [`crates/cortex/codemap.md`](../codemap.md) — responsibility, design patterns (ternary 2-bit packing, runtime AVX2 gate, honest dispatch ladder, MoE replay), data/control flow (inference pipeline, routing, compute dispatch, model load), integration points (hermes, neural-kernel, k_ai/k_hal backends), and the per-module map.

Key entry symbols: `cortex::{load_model, generate_via_model, TransformerModel, Model}`, `tensor::{Tensor, PackedTernaryTensor}`, `trinity::TrinityRouter`, `compute::dispatch_ternary`, `decode::{set_allow_mask, argmax_constrained}`, `model_hub::ModelSlot`, `gguf::load_gguf`, `bpe` facade.
