# crates/agent-core/

## Responsibility

No_std agent model + lifecycle primitives: the `Agent` trait, `AgentRegistry` (register/activate/schedule), and the cooperative scheduler loop. Consumed by every agent-hosting crate (`neural-kernel`, `k_nano`, `k_hal`, `k_ai`, `cortex`, `hermes`, `jarbas` — all depend on it). 5 source files: `lib.rs`, `budget.rs`, `hooks.rs`, `crew.rs`, `state_graph.rs`. Zero crate dependencies.

## Design

- **`Agent` trait** (`lib.rs`): `manifest()`, `tick(tick, tick_count) -> AgentTickResult` (`Pending|Done|Crashed`), optional `on_activate`/`on_deactivate`/`has_pending`; requires `Send`.
- **Scheduling**: `ScheduleKind` + `FlowTrigger`. `Listen`/`Router` usam `has_pending()` (agent faz bind EventBus — scheduler não subscribe sozinho).
- **`AgentInstance`**: tier, `affinity_ring`, goal-aware (`goal_urgency`, `novelty_score` + `boost_novelty`, `coherence_partner`), `paused_ticks` + `lifetime_paused_polls`.
- **`AgentRegistry::run()`**: respawn → poll R0→R1→R2 → FlowTrigger → budget → PreTick → tick → PostTick → novelty decay → halt. Display-first + UI overdue boost. Rate-limit Pending>50 (urgency=0). Watchdog crash >10k Pending sem urgency.
- **`budget.rs` (s370)**: overruns = **wall-clock** (`note_wall_overrun` quando tick > `TICK_WATCHDOG_MS`). `ticks_used` = polls/ciclo (stats). Recover @1000 paused polls; crash @`lifetime_paused_polls>=10000` (sobrevive recovers).
- **`tick_agent_by_index`**: AP path aplica `apply_tick_result` + `check_budget` (mesma semântica BSP).
- **`hooks.rs`**: Allow/Block efetivos; Modify ≡ Allow (reservado).
- **`crew.rs` / `state_graph.rs`**: API biblioteca — **não** wired em `run()` (honesty).

## Integration

Seed Agency: `hermes::register_agency_agents` ← `PackageHub::agency_specs()` (signed only). SpecialistAgent = EventDriven announce-once.
