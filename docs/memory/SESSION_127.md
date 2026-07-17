# SESSION_127 — ADR-0047 wave2 (bench + Genesis + G3–G5 + H2/H5)

**Data:** 2026-07-16  
**Objetivo:** Implementar defer plausível; descartar ISA / LatentBus adapter / H3 diffusion.

## Descartes (explícitos)
| Item | Motivo |
|------|--------|
| NeuOS ISA decompilação plena | BitNet ternário sem ISA confiável |
| LatentBus cross-modelo adapter | Sem hot-path multi-modelo |
| HMI H3 diffusion compositor | Inviável soft-float / modelo grande |

## Entregas
| Item | Arquivo / gate |
|------|----------------|
| N-gram bench | `ngram_spec` counters + `[ADR-0047-NGRAM]` |
| Genesis | `evolve::genesis_spawn` + `[ADR-0047-GENESIS]` |
| G3 SASOS-lite | `gpu/sasos.rs` |
| G4 H2O + pages | `cortex/kv_h2o.rs` |
| G5 pipeline CPU | `gpu/pipeline_g5.rs` (50μs = aspiracional) |
| H2 + H5 | `display/embed_viz.rs` wired no DisplayAgent |

## Evidência
```text
cargo check -p cortex,hermes,jarbas,neural-kernel --release → 0 erros
```

## Aberto
- DP4A n-gram verify (precisa G2 HW)
- G5 50μs/token real (shader + firmware)
