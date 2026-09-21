# SESSION_377 — skill-registry bughunt AIOS

**Sprint:** v1.9.99-s377 TEST  
**Data:** 2026-09-21  
**Foco:** Honesty SkillRegistry + Hermes Trust/cache + trim mortos  
**Canvas:** `skill-registry-bughunt-s377.canvas.tsx`

## Premissas

- Skill = manifesto MCP + tokens + CapGate deny-by-default
- Singleton `k_nano::SKILL_REGISTRY` (SESSION_217)
- Trust Contain/Enforce não pode ser ignorado
- WASM F5 ≠ execute até bridge wasmi (ADR-0059)
- slog ADR-0092; AIOS ADR-0088

## Aplicado

| ID | Fix |
|----|-----|
| H1 | `DynamicSkill` wasm → `Err(wasm_runtime_unwired)`; contracts no manifest |
| H2 | Hermes honra `check_or_cache`; usa `execute_skill` |
| H3 | Cache só `idempotent`; hits/misses; cap 64; get `&mut` |
| H4 | jarbas TTS/STT/settings/context/ser `required_tokens: [1]` |
| H5 | DYNSKILL_TOKEN reservado; Legacy(1) documentado (Hermes path) |
| M1 | `SkillListEntry` + callers hermes/k_ai/k_nano |
| M2 | `unregister` + RmSkill |
| M3 | schema validate no execute; Retry → Err honesto |
| M4 | delete index/task/fanout + FANOUT_POOL |
| L1 | 10 testes host |
| L3 | codemap honesty |

## Verificação

- `cargo test -p skill-registry` → 10/10
- `cargo check -p hermes` + `neural-kernel --release` → 0 erros

## Residual

- IDEA #599: wasmi 0.47→2.x no hermes (soft-float)
- IDEA #600: CapGate tokens reais vs Legacy(1) DynSkill
- WASM execute bridge (register runner → wasmi)
