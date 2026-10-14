# SESSION_473 — #700b I2_S AVX2 medição QEMU WHPX

Data: 2026-10-13. Medição do path AVX2 do I2_S no QEMU WHPX.

## Implementação (s473)
- `bitnet_w2a8.rs`: `I2S_AVX2_ENABLED` flag + `repack_col_major_i8` + `i2s_ternary_gemv_avx2` (INT8 absmax, sign-select blend/vpsrav, hadd sum, vmulps scale, scalar tail k%32) + dispatch `i2s_ternary_gemv_dispatch` (gate `allow_avx2`: WHPX `-cpu host` passa, TCG falha) + parity test `i2s_avx2_parity_vs_scalar` (bit-exact vs scalar 6 shapes)
- `tensor.rs`: `matmul_hybrid` usa dispatch
- `lib.rs`: re-export
- Check release 0 erros; cortex 152/152 lib tests pass (incl. AVX2 parity)

## Medição QEMU WHPX
- Build: `cargo build --release -p boot` ✓
- Imagem: Falcon3-1B (693MB .V7) via `build_image.py --hw --unified --build-boot --size 4096`
- QEMU: WHPX 4G/4c `-cpu Haswell` (estável; `-cpu host` causa OVMF #GP)
- One-liner: `cortex_crate::bitnet_w2a8::set_i2s_avx2(true)` em `main.rs` (revertido após)

## Resultados

| Métrica | Resultado |
|---------|-----------|
| Boot | ✅ Runtime, BOOT SCORE OK, smp_online=4 |
| LLM carregado | ✅ Falcon3-1B (18L, h=2048) via QEMU loader @0x100000000 |
| I2S AVX2 enabled | ✅ `i2s_avx2 set enabled=1` logado T+0 |
| AVX2 gate | ✅ `avx2=true` no ENV, `allow_avx2()` passa WHPX |
| Matmul dispatch | SSE2 p/ matrizes pequenas (m=8,k=8,n=8); AVX2 gated p/ k≥8,n≥8,n%4==0 |
| **Decode tok/s** | ❌ **Não mensurável** — benchmark pulado no QEMU |

## Por que tok/s não medido
O `measure-falcon3-toks.ps1` espera `decode_tok/s=` do inference queue, mas o benchmark é **explicitamente pulado no QEMU** em `main.rs:5055-5060` ("Falcon3 tok/s skip boot (sync hang)"). Só roda em HW real onde `generate_via_model()` é chamado no boot greeting. No QEMU, nenhum job de inferência é submetido automaticamente.

## AVX2 vs FP legacy
Não foi possível comparar porque nenhuma das configurações dispara inferência no QEMU. Ambas mostram comportamento idêntico de boot (modelo carrega, Runtime alcançado, sem decode). O path AVX2 está corretamente gated (`i2s_avx2_enabled() && allow_avx2()`) e seria selecionado para workloads de inferência em produção.

## Log excerpt (AVX2 enabled)
```
[T+0] [R2] [cortex] [I2S] [ok] - i2s_avx2 set enabled=1
[T+0] [R0] [k-nano] [ENV] [ok] - hv=WHPX sandbox=true smp=true avx2=true avx512=false fma=true max_aps=255 isa=avx2+fma
[T+0] [R0] [nk] [SIMD] [ok] - AVX2=SIM AVX512=NAO isa=avx2+fma
[T+95] [R0] [nk] [BOOT] [ok] - === BOOT SCORE qemu=true ram_mb=5120 smp_online=4 ===
[T+95] [R0] [nk] [BOOT] [ok] - llm           ok
```

## Conclusão
- **I2_S AVX2 implementado e gated corretamente** — pronto para HW real
- **Medição de speedup requer HW real** (ou QEMU com benchmark habilitado, o que exige investigar o "sync hang")
- **TL2 LUT (#702) só justifica se I2_S AVX2 provar ganho em HW real**

## Próximo
#702 TL2 LUT kernel — aguardar medição em HW real com AVX2. Ou despachar paralelas (#696/697/693/694/695/692).