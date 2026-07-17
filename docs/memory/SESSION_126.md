# SESSION_126 — ADR-0047 MVP PoC (1A + 2A)

**Data:** 2026-07-16  
**Objetivo:** Fechar família ADR-0047 como MVP PoC honesto na ordem LatentBus → Evolve → Probe → GPU G1/G2 → HMI H1/H4.

## Entregas

### L1 LatentBus
- `event-bus/src/latent.rs` — `LatentPacket`, bus, `TOPIC_THOUGHT_LLM`
- `cortex/src/projection.rs` — f16 ad-hoc + mean-pool 256D + `publish_thought`
- Global `LATENT_BUS` em `k_nano/globals`
- Publish no fim de `generate_speculative` (crate + residual bin)
- Hermes drain `[HERMES-LATENT]`

### L2 Evolve
- `hermes/src/evolve.rs` — ledger, `hot_swap`, rollback, `execute_sandbox`
- `WasmSkillRuntime::force_load_skill` + `execute_sandbox`
- SleepCycle DREAM → `evolve_dream_tick`

### L3 NeuOS Probe
- `cortex/src/neuos_probe.rs` — weight stats Healthy/Degraded + soul stub
- Gate boot: model LOADED → OK; senão `NO_MODEL`

### GPU G1/G2
- `jarbas/gpu/work_queue.rs` — Nop/MatmulTernary queue
- `backend::adr0047_compute_gate` → `HW` | `CPU_FALLBACK`
- G3–G5 defer

### HMI H1/H4
- `jarbas/display/ui_spec.rs` — JSON WindowSpec mínimo
- DisplayAgent: `UI_SPEC` + avatar telemetria via LatentBus
- H2/H3/H5 defer

### Gates boot
```
[ADR-0047-L1] latent publish/recv OK|ABSENT
[ADR-0047-L2] evolve swap=OK|SKIP
[ADR-0047-L3] probe=OK|NO_MODEL
[ADR-0047-G] compute=HW|CPU_FALLBACK
[ADR-0047-H] ui_spec=OK avatar_telem=OK|ABSENT
```

## Evidência

```text
cargo check -p event-bus,cortex,hermes,jarbas --release  → 0 erros
cargo check -p neural-kernel --release → 0 erros
```

## Defer explícito
- LatentBus cross-modelo adapter treinável
- Evolve Genesis / agentes geram agentes
- NeuOS ISA decompilação plena
- GPU G3 SASOS, G4 H2O/PagedAttention, G5 pipeline pleno, N-gram DP4A
- HMI H2 embedding desktop, H3 neural compositor, H5 thought splats
- Benchmark empírico 2× n-gram

## Docs
- ADR-0047 / GPU / HMI → Accepted (MVP parcial)
- INDEX `completa`; IDEA #444–448; TECNOLOGIAS 7.5c–g; STATE pista
