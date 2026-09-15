# SESSION_348 — ADR-0101 Onda 0 residual + Ondas 1–3 (Falcon3 Cognitive Lab)

**Sprint:** v1.9.99-s348 · **Bloco:** cortex / Falcon3-3B · **Data:** 2026-09-15

## Goal

Fechar residual Onda 0 (SIMD ADD/SUB/SKIP real no path metal) e avançar Ondas 1–3 com honesty (sem KL treinado, sem tok/s inventado).

## Feito

### Onda 0 residual
- `bitnet_sse::sse2_ternary_matmul_add_sub_skip` — ADD/SUB/SKIP sem `W*x` mul; acc `_mm_add_ps`
- Metal AVX2 stub → SSE path (não scalar-only)
- Testes: semantics + parity + Falcon3-shaped 64×32 — **3/3 PASS**

### Onda 1
- `vocab_shortlist` — top-K + score via `embed_lookup_pub`; skip full unembed entre refreshes
- Medusa heads → draft + `verify_draft` (se pack tiver heads)
- H2O telemetria + InferQueue evict near `max_seq`

### Onda 2
- `difficulty_gate` — Cheap/Normal/Full → soft_stride + max_gen
- Wired em `forward_with_kv`, `generate_speculative`, InferQueue prefill
- **Não** early-exit KL (honesty ADR)

### Onda 3
- `cognitive_runtime::generate_with_policy` — composição gate+generate+telem
- `generate_text` usa CogRT; `/cog` inclui `status_line`

## Honesty restante
| Item | Status |
|------|--------|
| AVX2 bare-metal FMA skip-native | ainda host-only (soft-float) — SSE cobre metal |
| Early-exit KL treinado | **não** — só policy budget |
| tok/s Neural OS | **não medido** nesta sessão |
| SGDB no matmul | continua prompt-side |

## Check
- `cargo test -p cortex --lib ternary_native_contract` ✅
- `cargo check -p hermes -p neural-kernel --release` (esta sessão)
