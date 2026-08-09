# bitnet-b1.58-2B-4T runs the FFN with SiLU instead of relu² — wrong logits on every backend

## Summary

The official `microsoft/bitnet-b1.58-2B-4T` model declares `"hidden_act": "relu2"` in its `config.json`, but the BitNet.cpp runtime never reads it: the FFN graph is built with a hardcoded `LLM_FFN_SILU`. The model therefore runs with the wrong activation function on **every** backend (CPU and GPU), producing wrong-but-finite output — the same failure class as #586.

## Root cause

- `src/models/bitnet.cpp:133` — `build_ffn(...)` is called with a hardcoded `LLM_FFN_SILU`.
- `src/llama-model.cpp` — the `LLM_FFN_OP_TYPES_FROM_STRING` map has `"relu"` but no `"relu2"`, and `llm_ffn_op_type_from_string()` is never wired into the bitnet arch.
- `utils/convert-hf-to-gguf-bitnet.py` / `utils/convert-ms-to-gguf-bitnet.py` — neither converter writes `<arch>.hidden_activation`, so the existing plumbing (`llama_hparams::llm_ffn_op`, `LLM_KV_HIDDEN_ACT`, `LLM_FFN_RELU_SQR` in `src/llama-graph.cpp`) is never exercised for bitnet models.

The graph builder already implements `LLM_FFN_RELU_SQR` (relu → sqr, `src/llama-graph.cpp:1690`); with `LLM_FFN_PAR` gating it produces exactly `relu²(gate) * up`, matching the HF `BitnetMLP` (`act_fn(gate) * up`).

## Fix (attached patches)

1. `src/llama-model.cpp`: add `{ "relu2", LLM_FFN_RELU_SQR }` to `LLM_FFN_OP_TYPES_FROM_STRING`.
2. `src/models/bitnet.cpp`: read `<arch>.hidden_activation` in `load_arch_hparams` (mirroring `llama_model_modern_bert`), defaulting to `LLM_FFN_SILU` for backward compatibility with existing GGUF files; use `hparams.llm_ffn_op` in the graph.
3. Converters: write `add_hidden_act("relu2")`.

## Verification

- Convert `microsoft/bitnet-b1.58-2B-4T` with the patched converter; `gguf-dump` should show `bitnet_b158.hidden_activation = "relu2"`.
- Compare logits before/after: with the fix the model produces coherent text; without it, output is wrong-but-finite.
- Old GGUF files (no key) keep the previous SiLU behavior — no regression.

## Related

- #586 — same wrong-but-finite class (ARM i2_s layout).