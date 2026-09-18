# SESSION_358 — cortex análise + bughunt profundo (R2 honesty)

## Goal
Mapear papel/premissas do `crates/cortex` (Ring 2: BitNet/Trinity/InferQueue/GGUF) e bughunt em 3 ondas: GGUF IDs + forward_hidden; nbytes/dequant/DECODER/HwExpert; slog ADR-0092 + FFN/unembed refuse.

## Papel (resumo)
Cortex = motor de inferência: Transformer BitNet, Trinity MoE (fonte única + `router_trained`), InferQueue D+B+C, GGUF parse/dequant, ModelHub, BPE, compute dispatch NPU→GPU→SMP→CPU. Dep só `k-nano`. GPU/NPU/W2A8/SMP-AP gated ou stub. Integra via hermes CortexAgent + bin bootstrap.

## HIGH corrigidos
| Bug | Fix |
|---|---|
| `GgufType::from_u32` 2=BF16 / 25=TQ2_0 (SESSION_309 errado) | ggml: 2=Q4_0, 30=BF16, 35=TQ2_0; teste + `gen_test_gguf.py` |
| `forward_hidden` path paralelo OOM→panic | delega `forward_with_kv` → `apply_one_layer` |
| `finalize_logits` `.unwrap` | refuse + `f32_zeros` |
| nn/moe `.expect`; Trinity router `.unwrap` | zero/keyword fallback |
| Federated `return true` pós-broadcast fail | `return sent` |
| nbytes Q4_1/Q5_1/Q8_1/Q8_K/TQ1_0 inventados | tamanhos ggml |
| dequant Q4_0/Q5_0/Q8_0 trunc→zeros Ok | `None` + slog fail |
| `rows*cols` overflow | `checked_mul` |
| matmul QKV/FFN/unembed None→zeros | abort/`(0,0)` |
| soft_stride pad OOM → KV desalinhado | abort fwd/job |
| `DECODER_CELL` dangling (slot vazio) | `take()` residual |
| HwExpert `mark(true)` descartava modelo | `set_hwexpert_v4_model` |
| CoarseFallback cancel só no início | warn uncancellable + check pós-generate |
| `has_avx512` CPUID cru | `allow_avx512()` |
| `MODEL_LOADED` sticky | `clear_model()` |
| `load_gguf` trunc mid-parse | Err metadata/tensor dims |

## MED
- slog `"info"` → `ok`/`warn`/`fail` em 15 arquivos (0 residual `info`)
- BPE `chars().next().unwrap` → `let Some`

## Verify
- `cargo check -p cortex --release` — 0 erros
- `gguf_type_ids_match_ggml` + `bughunt_s353` 12/12

## Aberto
- InferQueue decode: 1 tok/slice sob lock (latência UI)
- TQ2_0 layout local 24B/32 ≠ ggml 66B/256 (só GGUF ternário oficial)

## Lições
1. Type IDs GGUF = contrato ggml (`gguf-py/constants.py`); SESSION_309 “fix” que inventa 2=BF16 corrompe Q4_0 reais.
2. Path paralelo a `apply_one_layer` = regressão de honesty (s353) — delete, não patch.
3. `mark(slot,true)` sem armazenar o modelo = Ready mentiroso (mesma classe SSID fabricado).
4. Ponteiro em `DECODER_CELL` sem `take` no exit = UAF se generate não consome.
5. slog `"info"` = TRACE mudo (ADR-0092); progresso/fail precisa `ok`/`warn`/`fail`.
