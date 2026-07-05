# SESSION_080 — Sprint 80: AVX2 Debug + WHPX Detection + Forward Pass Speed

**Data:** 2026-07-05 | **Sprint:** 80 — Bloco 24 | **v0.80.0**

## Objective
Corrigir AVX2 BitNet forward pass (gargalo FFN), adicionar WHPX detection (scalar fallback), instrumentar timing por layer, e reduzir generate_speculative para 8 tokens máx.

## Modified Files
- **`bitnet_avx2.rs`** (+42/−18): `unpack_all()` removido, `unpack_row_into()` adicionado (descompacta uma linha por vez em buffer reutilizável de n bytes, eliminando alocação de 17.7 MB). `avx2_ternary_matmul_impl()` reescrito: row buffer + acumulação direta em out_row. WHPX detection via CPUID leaf 0x40000000: `"Micr"` → AVX2 desabilitado ("Microsoft Hv" emula VEX lentamente).
- **`tensor.rs`** (+10/−4): `has_avx2()` com WHPX detection (mesma lógica: CPUID leaf 1 bit 31 hipervisor + leaf 0x40000000 vendor check).
- **`cortex.rs`** (+30/−1): Per-layer timing (`[FWD] L0 qkv:... attn:... proj:... ffn_gateup:... down:... total:...`). `generate_speculative()` max 8 tokens. `forward_hidden()` layer loop instrumentado com `[FWD] layer N/30:` + unembed timing.
- **`agents.rs`** (+6/−1): Timing instrumentation `[CORTEX-LLM] generate_via_model took X ticks (~Ys)`.

## Detailed Timing (WHPX, modelo 2.4B, seq_len=64, layer 0)

| Operação | ticks | % | Descrição |
|---|---|---|---|
| QKV (3 matmuls) | 180 | 8% | Q(2560×640) + K(2560×100) + V(2560×100) |
| Attention (GQA) | 12 | 0.5% | 5 KV heads × 4 Q heads |
| O proj | 186 | 8% | 640×2560 |
| **FFN gate+up** | **1148** | **52%** | 2× PackedTernaryTernaryMatmul (2560×6912) |
| FFN down | 591 | 27% | 6912×640 |
| Total/layer | 2218 | | |

## AVX2 vs Scalar sob WHPX

| Modo | ticks/layer | tempo/layer (estimado) |
|---|---|---|
| **AVX2 (VEX emulado pelo WHPX)** | ~4443 | ~4.4s |
| **Scalar puro (WHPX nativo)** | ~2218 | ~2.2s |

**Resultado:** AVX2 sob WHPX é **2x MAIS LENTO** que scalar. Cada instrução VEX/AVX2 causa VM exit (~10k+ ciclos para emular). Scalar usa instruções GP que WHPX executa nativamente.

## Key Discoveries

1. **Root cause of 17.7 MB allocation fix:** `unpack_all()` alocava Vec\<i8\> de `k*n` = 2560×6912 = 17.7 MB a cada `ternary_matmul()` call no BitFFN. Substituído por `unpack_row_into()` que descompacta 1 linha por vez em buffer de `n` = 6912 bytes. **Mas isso NÃO acelerou** — o gargalo real é a emulação VEX pelo WHPX.

2. **WHPX emula AVX2/VEX como VM exits:** CPUID reporta AVX2 disponível (pass-through do host), mas cada instrução VEX causa vmexit. FFN gate+up com AVX2: 2352 ticks vs scalar: 1148 ticks.

3. **WHPX detection confiável:** CPUID leaf 0x40000000 retorna vendor string. WHPX = "Microsoft Hv". Quando detectado, `has_avx2()` retorna false, forçando scalar path.

4. **Timer tick rate:** ~100 ticks/s (LAPIC timer count=2097152 a 1240 MHz). 1 tick ≈ 1ms.

5. **Generate_speculative com WHPX inviável para 2.4B:** ~60s por forward pass de 64 tokens → 8 tokens = ~6h.

## Blockers
- **WHPX forward pass ~2s/layer** — 30 layers × 2s = 60s por forward pass. Generate_speculative com 8 tokens: ~6h.
- **Solução definitiva:** Bare metal ou QEMU + KVM (Linux) onde AVX2 roda nativamente (~5-20ms/layer).

## Next Steps
1. KV cache para autogeneração: só processar novo token, reutilizar K/V anteriores
2. Testar em hardware real com UEFI boot
3. Sprint 81: JARVIS Persona (SOUL.md, IPW, Compression, Notification Gate)
