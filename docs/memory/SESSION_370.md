# SESSION_370 — agent-core bughunt AIOS (scheduler honesty)

**Foco:** Premissas ADR-0088 + Agent/Skill-first aplicadas ao crate `agent-core` (e Agency hermes).

## Achados → fixes (High→Low)

| Sev | Achado | Fix |
|-----|--------|-----|
| HIGH | `paused_ticks>=1000` recover tornava `>=10000` crash **dead code** | `lifetime_paused_polls`; crash antes de recover |
| HIGH | Budget count `consume(1)` + `reset_all`/tick **nunca** pausava (budget=100 inerte) | Overruns = wall-clock `note_wall_overrun` no tick > `TICK_WATCHDOG_MS` |
| HIGH | `tick_agent_by_index` (AP) ignorava Done/Pending/Crashed/budget | `apply_tick_result` + `check_budget` compartilhado |
| HIGH | `register_agency_agents` usava `Agency::new()` **vazio** | `PackageHub::agency_specs()` + from_specs |
| MED | Display early / UI boost sem consecutive_pending | `apply_tick_result` nos paths UI |
| MED | SpecialistAgent Continuous+Pending spam | EventDriven announce-once + `has_pending` |
| MED | `novelty_score` só decay | `boost_novelty(name, amount)` |
| MED | FlowTrigger Listen/Router mentiam subscribe | Doc: sinal = `has_pending()` |
| LOW | `skill_map` morto | removido |
| LOW | crew/state_graph overclaim "wired no run" | honesty comments |
| LOW | HookResult::Modify sem efeito | documentado ≡ Allow |
| LOW | `active_agent_count` dup | delega a `active_count` |

## Upstream

`agent-core` = 0 deps. CrewAI-RS / AgnosAI / post-haste = std/Tokio ou outro modelo — **não** bump; padrões DAG/crew ficam API lib até Hermes wire.

## Verificação

- `cargo test -p agent-core --lib` → 5/5
- `cargo check -p hermes --lib` → 0 erros
- `cargo check -p neural-kernel --release` → 0 erros

## Lições

1. Recover + crash no mesmo contador com `if/else if` na ordem errada = safety morta.
2. Budget por contagem de poll com 1 poll/ciclo = telemetria mentirosa — AIOS mede wall-clock.
3. Path AP/BSP sem `apply_tick_result` = duas verdades de lifecycle.
4. Agency empty registry fingindo “N agents” viola ADR-0052 deny-by-default.
