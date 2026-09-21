# SESSION_388 — ModelHub + Trinity MoE bughunt (honesty AIOS)

**Data:** 2026-09-21  
**Premissas:** ADR-0085 register_bytes único · SESSION_273 TRINITY único · ADR-0083 router_trained · ADR-0101 lab=3B Active · ADR-0059 Efeito Matrix CapGate · ADR-0088 Observe · ADR-0092 slog

## Achados → fixes (High→Low)

| ID | Sev | Fix |
|----|-----|-----|
| H1 | HIGH | `register_model` Active/RustCoder/HwExpert → `set_*` (antes mark+drop) |
| H2 | HIGH | `get_or_mmap_expert` só `Some` se `weight`; slot loaded + weight=None → `warn` residual |
| H3 | HIGH | `slot_from_bitnet_bytes` 450–1200MB → Active (Falcon3-3B lab); Pro ≥1200MB |
| H4 | HIGH | `register_bytes` LLM@HwExpert → Active+warn (não drop+mark mentiroso) |
| H5 | HIGH | `generate_from_slot(Active)` → CURRENT_MODEL (HUB array vazio); RustCoder static |
| M1 | MED | `hub_status` Pro = `alias`\|`1`\|`0` |
| M2 | MED | `expert_device_class` só HW; DiskDiag→Block; CapGate Block→DeviceIo |
| M3 | MED | slog TRINITY/ROUTE `info`→`ok` (ADR-0092) |
| M4 | MED | `set_router_weights` sucesso sev `ok` |
| M5 | MED | Vision fora de generator_fast / fallback_generate / pick_fit |
| L1 | LOW | docs populate_trinity = fill gaps (re-export cortex) |
| L2 | LOW | BitNet MoE community / ruvector = idea-only (≠ Trinity intent router) |
| L3 | LOW | simplify assert tautológico `expert_resident_count>=0` |

## Upstream (idea-only)

- microsoft/BitNet = dense 1.58 (não MoE de layers)
- Community MOE-BitNet158 / ruvector BitNet MoE = idea residual, não port

## Residuals

- mmap FAT→`PackedTernaryTensor` no Expert (Efeito Matrix completo)
- Metal tok/s WHPX (s387)

## Gates

- `cargo test -p hermes --lib trinity_inject` → **22/22**
- `cargo test -p cortex --lib untrained_lcg` → pass
- `cargo check -p cortex|hermes|neural-kernel --release` → 0 erros

## Canvas

`hub-trinity-bughunt-s388.canvas.tsx` — 13/13 aplicados
