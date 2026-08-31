# SESSION_298 — Falcon3-3B lab: gap analysis ternário-nativo + ADR-0101

**Sprint:** v1.9.99-s298 TEST  
**Data:** 2026-08-31  
**Objetivo:** Formalizar o shift DeepSeek V4-Flash / 7B-as-lab → **Falcon3-3B Instruct 1.58-bit** como laboratório; provar se o forward cortex é ternário-nativo; **não** implementar os 16 itens.

## IDEA / ADR

- **#544** → ADR-0101 (`docs/architecture/0101-falcon3-3b-cognitive-lab.md`)
- **#540** (7B alvo SESSION_291): lab **superseded** por #544; `llm_boot_plan` 7B-opcional permanece como GeneratorPro
- Colisão pré-existente: o changelog IDEA também usa **#540** para GGUF i2_s — não renumerar; i2_s fica residual ADR-0046/0085

## Arquitetura verificada (HuggingFace raw config.json)

| Modelo | L | hidden | Q/KV | head | FFN | vocab | ctx |
|---|---|---|---|---|---|---|---|
| Falcon3-3B Instruct | 22 | 3072 | 12/4 | 256 | 9216 | 131072 | **32768** |
| Falcon3-3B Instruct **1.58bit** | 22 | 3072 | 12/4 | 256 | 9216 | 131072 | **4096** |
| Falcon3-7B Instruct 1.58bit | 28 | 3072 | 12/4 | 256 | 23040 | 131080 | 32768 |
| Falcon3-1B Instruct 1.58bit | 18 | 2048 | 8/4 | 256 | 8192 | 131072 | 8192 |
| Falcon-E-3B Instruct | 32 | 2048 | 16/2 | 128 | 13312 | 32768 | 32768 |

AGENTS.md (SESSION_288) dizia 3B=7B=28L/23040 — **falso**. Corrigido nesta sessão.

## Prova kernel (não tok/s)

- Scalar / SSE: ADD/SUB/SKIP de ativações f32 sobre packed 2-bit → **nativo algébrico**.
- AVX2 host: unpack → f32 FMA (LUT ou cvtepi8) → **não** skip-native SIMD.
- Bare-metal (`target_os=none`): `avx2_ternary_matmul_impl` **é o scalar**.
- W2A8: `GENERATION_GAPS_RESOLVED=false` → dispatch inerte.
- ATLAS 7.1 / 10.1 tok/s = **deles** (i7-7700T). Neural OS: sem número.

## Mudanças de código (incremento pequeno, 3B-first inventário)

- `falcon3_boot_names` / `fat_names_for(Active)`: 3B v6 primeiro.
- `hub_slot_for_kind(Daily3B)` → `Active` (era `Agent`).
- `llm_boot_plan`: pick = 3B se couber; 7B continua `load_pro_7b_resident` se houver RAM.
- Comentário honestidade em `bitnet_avx2.rs` + teste host ADD/SUB/SKIP.
- AGENTS.md: factos 3B vs 7B + pack lab 3B-first.

**Não feito:** rewrite AVX2 no `none`; conversão HF→v6 (precisa download); early-exit/speculative 3B; commit git.

## Conversão (quando houver pesos)

```
python tools/convert_falcon3_bitnet.py --hf-repo tiiuae/Falcon3-3B-Instruct-1.58bit --output target1/FALCON3.V6
```

`PRO.v6` 7B já shipped (SESSION_288) **não** substitui o lab 3B.

## Limites

- Não medimos tok/s no QEMU/metal nesta sessão.
- Não listamos o FAT do pendrive atual; gap 3B-no-FAT é possível e deve ser verificado com `PACK_LLM=falcon3` + `find_falcon3()`.
- Falcon-Edge ≠ Falcon3-3B.

## Próximo

Onda 0 real: SIMD ADD/SUB/SKIP compilado para `x86_64-unknown-none` + `FALCON3.V6` no FAT + paridade scalar.
