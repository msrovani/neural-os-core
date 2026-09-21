# SESSION_371 — cortex bughunt AIOS (GGUF TQ2_0 + honesty)

**Foco:** Premissas ADR-0088 / 0085 / 0057 / Ring 2 cortex — motor BitNet/Trinity/InferQueue/GGUF.

## Achados → fixes

| Sev | Achado | Fix |
|-----|--------|-----|
| HIGH | TQ2_0 PrismML **24B/32** ≠ ggml **66B/256** (type 35); comment mentia ID=25 | dequant+nbytes ggml; codes `(c−1)*d` |
| HIGH | `Unknown` nbytes=`ne*4` inventava bounds p/ IQ*/MXFP4 | nbytes=0 + bits=0 |
| MED | `else if false` weather skinny dead | removido — só `argmax_row_hf_vocab` |
| MED | brightness “stub” fingia path útil | slog `warn` DEGRADED + texto AWAITING_HW |
| LOW | burn_flex WIP/stub overclaim | facade honesty → Tensor/matmul_hybrid |
| LOW | neuos `SoulVector stub` | doc = weight-health 7-D ≠ AffectVector |

## Upstream

- ggml/llama.cpp: TQ2_0=35, 66B/256 confirmado (gguf.md + rlx-gguf).
- MXFP4=39 → Unknown (não inventar dequant).
- `cortex` deps path-local + spin/libm/x86_64 workspace — sem bump sem necessidade (no_std estável).

## Aberto (não inventar)

- TQ1_0 nbytes OK, dequant ainda `None` (raro; falha honesta).
- DSD mesh verifier MLP (speculative.rs) — local only documentado.
- InferQueue 1 tok/slice UI (s358 residual).

## Verify

- `cargo test -p cortex --lib` → 81 pass (TQ2_0 1×; `#[test]` duplicado removido)
- `cargo check -p cortex --release` + neural-kernel

## Lições

1. Layout “que funciona no nosso teste sintético” ≠ contrato ggml — nbytes deve seguir o arquivo real.
2. Type ID no comentário errado (25 vs 35) = mesma classe SESSION_309.
3. `Unknown → F32 size` = OOB bounds mentiroso; deny size=0.
