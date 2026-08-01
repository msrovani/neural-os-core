# crates/agent-core/

## Responsibility

No_std agent model + lifecycle primitives: the `Agent` trait, `AgentRegistry` (register/activate/schedule), and the cooperative scheduler loop. Consumed by every agent-hosting crate (`neural-kernel`, `k_nano`, `k_hal`, `k_ai`, `cortex`, `hermes`, `jarbas` — all depend on it). 5 source files: `lib.rs`, `budget.rs`, `hooks.rs`, `crew.rs`, `state_graph.rs`.

## Design

- **`Agent` trait** (`lib.rs`): `manifest()`, `tick(tick, tick_count) -> AgentTickResult` (`Pending|Done|Crashed`), optional `on_activate`/`on_deactivate`; requires `Send`. `AgentManifest` = name/kind/schedule/auto_start/persist.
- **Scheduling**: `ScheduleKind` (Oneshot/Continuous/PollEvery/EventDriven) is complemented by `FlowTrigger` (`Schedule`, `Start`, `Listen(topic)`, `Router(topic)`) — semantic wake triggers wired to EventBus topics; `should_poll_flow()` decides per tick.
- **`AgentInstance`** wraps a boxed agent with runtime state: tier, `affinity_ring` (ADR-0055: 0=BSP/critical, 1=compute, 2=event/WASM), goal-aware fields (`goal_urgency`, `novelty_score`, `coherence_partner`, ADR-0076), and `paused_ticks` for the budget watchdog.
- **`AgentRegistry`**: `register`/`activate`/`get`; `init_phase()` drains boot Oneshots in round-robin rounds (10 000-round cap so boot can never hang on cross-agent waits); `run()` is the infinite scheduler (returns `!`) with injected `halt`, `check_respawns` and `spawn_agent` closures for platform hooks. Poll order = affinity ring R0→R1→R2, sorted by `2*goal_urgency + novelty_score`. Pending agents are rate-limited (skip 80% of ticks after 50 consecutive Pending) unless urgency > 0; >10 000 consecutive Pending ⇒ `Crashed`.
- **`budget.rs`**: `BudgetManager` per-agent tick budget (default 100); watchdog states Normal→Warning (>1 overrun)→Paused (>3 overruns); auto-recover at 1000 paused ticks, crash at 10 000. Global `BUDGET_MGR_PTR` + `agent_budget_stats()` for Hermes monitoring.
- **`hooks.rs`**: `HookRegistry` of fn-pointers for `PreTick`/`PostTick`/`OnCrash`/`OnSpawn` returning `Allow|Block|Modify`; a `Block` short-circuits the chain.
- **`crew.rs`**: CrewAI-inspired `Crew`/`CrewPool` — Sequential/Hierarchical processes, `ScheduledTask` with `depends_on`, `kickoff`/`next_ready_task`/`complete_task`.
- **`state_graph.rs`**: LangGraph-inspired `StateGraph` — agents as nodes, `EdgeCondition`-guarded edges; `advance()` picks the first satisfiable transition.

## Flow

Kernel init registers agents → `init_phase()` runs boot Oneshots to Done → `run()` loops: respawn queue → `poll_order_by_affinity()` → FlowTrigger check → budget watchdog → PreTick hooks → `tick()` → PostTick/OnCrash hooks → novelty decay → `halt()`. Watchdog/budget events are logged through optional hooks (`set_sched_metrics_hook`, `set_budget_event_hook`, `set_bei_tick_hook`).

## Integration

Scheduler metrics feed the Jarbas HUD via atomics (`LAST_SCHED_AGENTS`, `LAST_SCHED_POLLED`). Seed agents are **not** built here: `hermes::package_hub::seed_embedded_agents()` registers the ~41 native agents, and for `tier == "native"` skips runtime Ed25519 signing + VFS persistence (trusted-by-compilation; see `package_hub.rs` ~L847 and SESSION_230). `Cargo.toml` has zero dependencies.
