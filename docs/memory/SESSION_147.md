# SESSION_147 — Trilha R: soft-float / hardfloat (VITS + GEN PT)

**Data:** 2026-07-18  
**Tipo:** pesquisa paralela (não feature Onda 4)  
**Check:** docs only + referências código existentes

## Problema

Target `x86_64-unknown-none` + nightly Windows: SSE/AVX hardfloat no kernel quebra ou força soft-float (`cargo nk`). Piper VITS/HiFi-GAN e fluency GEN PT no BitNet 2B dependem de FPU/latency que o path soft-float não entrega bem.

## Achados (ground truth do repo)

| Item | Estado | Evidência |
|------|--------|-----------|
| Soft-float build | ✅ path oficial | `.cargo/config.toml` alias `cargo nk`; SESSION_108 |
| Piper neural-lite | ✅ executável | `audio/piper.rs` — não VITS pleno |
| VITS/HiFi-GAN | ⏳ blocker | ADR-0045; não fakear |
| GEN PT fluency | ⏳ residual | soft-float logits fracos (STATE) |
| OSXSAVE/XCR0 | ✅ parcial SMP | SESSION_141 / ADR-0055 — FPU enable ≠ hardfloat codegen |

## Opções (não implementar agora)

1. **Manter soft-float + neural-lite** (status quo honesto) — gate voz OK parcial.
2. **Hardfloat seletivo** (`-C target-feature=+sse2` + soft-float off) só em crates cortex/audio — risco ABI/`x86_64` crate; precisa PoC isolado.
3. **Offload VITS** para host/WASM CapGate — fora bare-metal primário.

## Decisão Onda R

- **Defer explícito** VITS/hardfloat até PoC medido (latency + check 0 erros).
- Onda 4 já marcou soft-float ⏳; esta sessão só documenta pesquisa.
- Não misturar com Onda 6/7.

## Aceite

Trilha R fechada como **pesquisa** (não feature). Próximo: Onda 6/7 código.
