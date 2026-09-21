# SESSION_390b — SelfHeal residual wire (KERNEL_ERROR + Safety + checkpoint)

**Foco:** Fechar HIGHs remanescentes do deep bughunt SelfHeal pós-s390 (subagent residual).

## Premissas

- ADR-0088 AIOS Observe→Plan→Act→Verify
- ADR-0027 exception→EventBus→SelfHeal + budgeted recovery
- ADR-0092 slog honesty
- Upstream: NØNOS/HALOS fail-closed Ring0; heal = respawn supervisado

## Achados → fixes (High→Low)

| Sev | Achado | Fix |
|-----|--------|-----|
| HIGH | KERNEL_ERROR RX morto | `note_exception_irq` + `drain_exception_notes` → publish + `KERNEL_EVENT_LOG` |
| HIGH | `save_checkpoint` sem wire útil | pós-AgentFleet + `begin_orderly_shutdown/reboot` |
| HIGH | `push_respawn` true≠spawn | `normalize_respawn_name` + `register_can_spawn` + `respawn_refused` |
| HIGH | SafetyAgent órfão | `pub mod safety` + register + `check_all` no SecurityAgent |
| HIGH | boot `analyze(true)` Act descartado | `analyze(..., false)` BootLog/BootSelfHeal |
| HIGH | SKILL_CREATE 0 consumers | HermesAgent subscribe → skill_observer/skill_gen/self_evolve |
| MED | nome SelfHealAgent≠arm | manifest `self_heal` + heartbeat canónico |
| MED | restore overclaim | status `partial_bitmap` |
| MED | EventKind::KernelError stub | `KERNEL_EVENT_LOG.push` no drain |
| MED | #GP/#UD halt sem honesty | slog observe-only fatal ASCII |
| LOW | semantic_snapshot 0 callers | `allow(dead_code)` residual P09 |
| LOW | migrate disk / Optimizer | OPEN (honesty já; próxima sessão) |

## Gates

- `cargo test -p k_ai --lib` → **55 pass / 0 fail / 1 ignored**
- `cargo check -p hermes --release` → 0 erros
- `cargo check -p neural-kernel --release` → 0 erros

## Canvas

`canvases/selfheal-bughunt-s390b.canvas.tsx`

## Residuais

- disk `migrate_to_another_disk` wire AutoInstaller
- OptimizerAgent órfão (A-020)
- P09 full checkpoint restore
- RecoveryAction verifier `Option<fn()>` ainda None
