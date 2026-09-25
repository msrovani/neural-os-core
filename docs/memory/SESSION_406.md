# SESSION_406 — Heap auto-fracionado advisory (carve por core + quota por ScheduleKind)

**Motivo:** ordem "memória descoberta e fracionada automaticamente, com folga por processo/núcleo/necessidade". Design via oráculo (2 tiers, enforcement por grow-gate advisory, nunca reserva/partição).

## Implementação (commit 2b650c56, +231)
- `allocator.rs`: `CORE_QUOTA[256]` + `split_core_carve` (BSP 40%, APs 60%, atomics-only) + `AGENT_MEM_QUOTA` (Continuous 8MB, PollEvery 2MB, Oneshot/EventDriven 512KB, Inference→0 spill MHI) + grow-gate (`can_alloc_bytes` + `note_alloc_refused`, sem panic) + 3 testes host.
- `runqueue.rs`: backpressure em `enqueue_agent` (overflow honesto), `enqueue_agent_with_schedule`, steal prefere menor pressão, hook `init_core_carves()` em `init_roles_from_pools`.
- Marcadores: `carve cpu<N>=<MB>MB`, `grow refuse need/uncovered/headroom (advisory quota gate)`.

## Verificação
`cargo check --release` 0 erros · 3 testes auto_fractioning + 28 runqueue verdes.
