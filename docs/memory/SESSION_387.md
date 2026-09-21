# SESSION_387 — Falcon3 LLM 1.58bit bughunt (1B/3B/7B/10B)

**Data:** 2026-09-21  
**Premissas:** ADR-0101 lab=3B · ADR-0085 header Observe · ADR-0084 rope fidelidade · ADR-0088 AIOS · ADR-0092 slog

## Achados → fixes (High→Low)

| ID | Sev | Fix |
|----|-----|-----|
| H1 | HIGH | `note_header_from_bytes` / `set_model_header` wired em `register_bytes` + `load_llm_v6`; `clear_model` limpa header |
| H2 | HIGH | load v3/v4/v6: `rope_theta: theta`; default Falcon3 shapes → **1000042** |
| H3 | HIGH | `convert_falcon3_bitnet.py` default rope=1000042; max_seq default 4096 (lab) |
| H4 | HIGH | `n3_cortex_gate` sev `ok`/`warn` (não TRACE) |
| H5 | HIGH | GGUF `try_build_transformer`: `rope_precompute` (antes cos/sin vazios) + `max_seq` de `context_length` |
| H6 | HIGH | `register_bytes`: Active → `set_model` (MODEL_LOADED+dim); Pro **não** sobrescreve CURRENT |
| M1–M5 | MED | `ctx_len`/`rope_theta` em `Falcon3Kind`; GGUF max_seq; testes nome→SKU; clamp slog |
| M6 | MED | `download_falcon3.py` default `--variant=3b` (ADR-0101) |
| L1–L4 | LOW | ADR-0101 lifecycle; upstream BitNet idea-only; `header_with_runtime_clamp` único; convert_gguf falconish θ |
| L5 | LOW | `slot_from_bitnet_bytes` ranges exclusivos (fim warn overlapping) |

## Upstream (idea-only)

- microsoft/BitNet: Falcon3 1B–10B I2_S (+ TL2); issue #532 crash 10B 2ª msg — não portar server
- tiiuae HF configs confirmados (rope 1000042; 3B ctx 4096)

## Residuals

- Metal tok/s WHPX (`measure-falcon3-toks.ps1`)
- IDEA #549 tokenizer/KV 32K GGUF
- Soft_stride ≠ early-exit treinado

## Gates

- `cargo test -p cortex --lib falcon3_kind` → pass
- `cargo check -p cortex --release` → 0 erros
- `cargo check -p neural-kernel --release` → 0 erros

## Canvas

`falcon3-llm-bughunt-s387.canvas.tsx` — 16/16 aplicados
