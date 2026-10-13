# SESSION_470 — QEMU I2_S + 2B4T boot (medidas reais)

Data: 2026-10-13. Medição de tok/s do I2_S scalar e boot do 2B4T no QEMU WHPX 4G/4c.

## I2_S scalar (Falcon3-3B, 22L/h=3072)

- **tok/s: ~0.013** (1 token / 78s) — **não usável** para workloads interativos.
- Prefill: ~123s (22 layers). Decode: 75-78s/token.
- Causa raiz: kernel `x86_64-unknown-none` soft-float; sem AVX2. Ambos os caminhos (FP legacy e I2_S) são loops escalares puros. 3B é grande demais para CPU-only soft-float.
- Paridade I2_S vs FP: testes unitários passam (bit-exact vs referência quantizada), mas **zero speedup mensurável** no soft-float.
- **Recomendação**: manter `I2S_ENABLED` default **OFF**. Habilitar só para medição em HW/AVX2 futuro.

## 2B4T boot (BITNET2B4T.V7 → BITNET2B.BIN 760MB)

- Boot: chegou ao Runtime (8 fases, SMP online=4) ✓
- Model load: **FALHOU** — header v7 detectado (`v7 LLM h=2560 L=30 q_dim=2560 vocab=128256 act=1 emb=1 feat=0x07`) mas `load_model` rejeitou com `h=0 L=0` (mismatch v7 header vs v6 loader).
- Erro: `load_model REJECT header invalido h=0 L=0 - skip (evita hang K49)` → `llm=ABSENT`.
- **Status**: 2B4T v7 **não boot-ready**. Faltam:
  1. v7 header parsing em `cortex::model::parse_model_header`
  2. v7 tensor loading em `load_model_v7`
  3. ReLU² (act=1) wired em todos os 4 FFN forwards
  4. Q6_K embed dequant (emb=1)

## Decisões

| Item | Decisão |
|------|---------|
| I2_S default | OFF (mantido) |
| 2B4T boot | Não pronto — requer loader v7 completo |
| Falcon3 tok/s QEMU | Não representativo — usar HW real ou modelo 1B |
| Runtime toggle I2_S | Não necessário agora |

## Evidências

- `logs/boot_whpx_i2s_falcon3_20261013_131546.txt` — LLM LOADED, prefill 123s, decode 78s/tok
- `logs/boot_whpx_2b4t_20261013_130135.txt` — v7 header detectado, load_model REJECT h=0 L=0
- `logs/falcon3_toks_whpx.txt` — timeout 600s, decode_tok/s nunca emitido

## Próximos passos (IDEAs)

- #699: v7 loader completo (header + tensores + ReLU² + Q6_K embed) para 2B4T boot
- #700: I2_S AVX2 path (quando HW com AVX2 disponível)
- #701: Modelo 1B para medição QEMU representativa
- #702: TL2 LUT kernel (se I2_S scalar provar ganho em HW)