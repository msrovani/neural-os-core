# SESSION_390 — SelfHeal / self-health honesty bughunt

**Data:** 2026-09-21  
**Premissas:** ADR-0027 budgeted recovery · ADR-0088 Observe→Act→Verify · ADR-0092 sev · SESSION_356/374 GLOBAL_SELF_HEAL

## Achados → fixes (High→Low)

| ID | Sev | Fix |
|----|-----|-----|
| H1 | HIGH | I3: `loaded=false` → FwGpu `load_status`; outros VID observe-only |
| H2 | HIGH | AI restart: `record_failure` só se bridge_missing; sucesso→heartbeat |
| H3 | HIGH | `HEALING_LLM_REQUEST` só em `AwaitLLM` (sem double-act) |
| H4 | HIGH | `BudgetedRecovery::canonical()` (=10, não 60k) |
| M1 | MED | SilentFailure I5 só se `watched_count()>1` |
| M2 | MED | `pending_diagnosis=None` pós-apply |
| M3 | MED | SKILL_CREATE: `published_no_consumer` + sem notify falso |
| M4 | MED | checkpoint `heap_size` via `heap_budget_mb(TOTAL_RAM_MB)` |
| L1 | LOW | BootSelfHeal `HEAL`/`n2` → ok\|warn |
| L2 | LOW | Cortex/NSGDB slog → ok |
| L3 | LOW | `check_disk_health` parênteses |
| L4 | LOW | 5 unit tests self_heal |

## Upstream (idea-only)

- ryu-healing: diagnose→propose + loop-prevention (já espelhado em AwaitLLM+budget)
- RavenClaws: circuit breaker → residual SilentFailure fleet HB
- harness-orchestrator: tokio/std — não portar; redução = manter GLOBAL_SELF_HEAL

## Residuals

- Verifier `fn()` em RestartDaemon ainda `None`
- Fleet heartbeats → SilentFailureDetector
- I3 slots iwlwifi/rtl load_status
- Checkpoint P09 page tables/drivers

## Gates

- `cargo test -p k_ai --lib self_heal` → **5/5**
- `cargo check -p hermes|neural-kernel --release` → 0 erros

## Canvas

`selfheal-bughunt-s390.canvas.tsx`
