# SESSION_273 — Adequação AIOS: cortex + hermes + jarbas

**Sprint:** v1.9.99 TEST  
**Data:** 2026-08-17  
**Premissas:** ADR-0088 · emagrecer · Agent/Skill · Trust · HITL Escalate · honesty · memorize

## Por que a 272 não bastava

SESSION_271–272 fecharam o **boot bind**. Cortex/Hermes/Jarbas ainda mentiam no runtime:

| Premissa | Achado |
|----------|--------|
| Emagrecer / fonte única | Dois `TRINITY`: bin carregava o router; `hermes::globals` era `TrinityRouter::new()` vazio. SleepCycle/cognitive_bridge liam o errado (padrão SESSION_237). |
| Honesty MoE | `moe_router_loaded()` = `Some(embed)+Some(weight)` inclusive LCG seed=42. `classify_intent` fazia matmul no ruído. Boot chamava `generate_random_router_weights` se ROUTER.BITNET falhasse. |
| Honesty HUD | `NET` via `is_online()`: HwReal sempre true; I225 fora de `nic_globals`. `LLM` = `llm_busy()`, não modelo carregado. `NETWORK_DEGRADED` nunca publicado. |
| HITL | HEALTH_ISSUE (I5 SLIP, recipe Escalate) virava `USER_INTENT` “diagnostique e corrija” → spam LLM. |
| Observe→Act | Greeting/HUD não consumiam postura Cortex nem NIC medida. |

## Decisão

| Anel | Mudança |
|------|---------|
| `cortex::trinity` | `router_trained`; LCG **não** entra no matmul; `moe_router_loaded` = treinado; `TRINITY` lazy_static canônico; `CORTEX_POSTURE` no EventBus. |
| `hermes::globals` | `pub use cortex::trinity::TRINITY` (remove static vazio). |
| `hermes::runtime_observe` | `should_escalate_health_to_llm` / `ingest_health_issue` / `hud_line`. |
| bin | `pub use trinity::TRINITY`; load só arquivo; `note_physical_nic` (inclui I225); `note_slip_degraded`. |
| `k_nano::env` | `net_link_ok` / `net_hud_label` (`NET`/`slip`/`off`). |
| Jarbas compositor | HUD no `render()`: `no-llm`/`idle`/`LLM` + `MoE`/`kw` + net honesto. |

## O que NÃO foi feito (honesto)

- Unificar `TRUST_CACHE` bin vs hermes.
- LLM decidindo bind (ainda pós-DriverInit; sem pesos = keyword).
- CapGate syscall T+0.
- `measure_bandwidth` (#513).

## Testes

- `cargo test -p cortex --target-dir target/check-s273-cx trinity`
- `cargo test -p hermes --target-dir target/check-s273-hm runtime_observe`
- `cargo test -p k-nano --target-dir target/check-s273-nano env`
- `cargo check -p neural-kernel --features fat-boot-log --target-dir target/check-s273-nk`

## Lição

Dois Trinity = o mesmo bug dos BGE duplicados: o boot alimenta um static e o runtime lê outro. HUD que trata HwReal como “NET” e `llm_busy` como “modelo” é mentira visual. HEALTH_ISSUE degradado não é prompt.
