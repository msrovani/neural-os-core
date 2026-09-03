# SESSION_308 — SMP anti-churn + papéis N≥5 + steal_burst

**Data:** 2026-09-03 | **Sprint:** v1.9.99-s308 TEST | **Status:** wired (host tests PASS)

---

## Premissa AIOS (ADR-0088)

Unidade de trabalho = **tick de Agent**. Survey bare-metal (Tokio/st3/smp-nostd/
Plinth/ArceOS) filtrado: adotar mecânica RQ/IPI/honesty; rejeitar EEVDF/preempt/RT/deps.

## Problema (s307 QEMU 4c)

```text
roles n=4 … worker=0 memory=1
runqueue: 12 agents → APs   # a cada tick
```

## Fix

| Onda | Mudança |
|------|---------|
| A | `should_redistribute`; inflight bitmap; IPI 0→1; slog rate-limit; overflow→blocked |
| B | Memory só N≥5; N=4 ≥1 Worker; `network_agent` ring=3 → Memory/fallback Worker |
| C | `steal_burst` half∩4; stats slog /64 ticks; HUD até 32 cores |
| D | ADR-0089 §6–8; IDEA #324/#492 |

## Aceite

- [x] Host: 28 testes `smp::runqueue` (n4_keeps_worker, n5_has_memory, steal_burst, inflight)
- [x] QEMU `-smp 4` TCG (`logs/qemu4c_s308.txt`):
  - `roles n=4 sys=1 compute=2 worker=1 memory=0` (Memory N≥5)
  - `ap_pollable=true`; Runtime; **2×** `runqueue:` (não 1/tick)
  - `stats tick=` ainda não (boot/timer cedo no log capturado; host + rate-limit OK)
- [ ] Metal K23 residual

## Arquivos

- `crates/k_nano/src/smp/runqueue.rs`
- `crates/agent-core/src/lib.rs` (doc AGENT_TICK_BUSY)
- `crates/neural-kernel/src/main.rs` (Net ring 3)
- `crates/jarbas/src/display/{gauges,compositor}.rs`
- `docs/architecture/0089-smp-per-cpu-runqueue.md`
