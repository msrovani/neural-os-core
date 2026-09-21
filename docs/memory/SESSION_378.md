# SESSION_378 — ticket-lock bughunt AIOS

**Sprint:** v1.9.99-s378 TEST  
**Data:** 2026-09-21  
**Foco:** Soundness Sync + try_lock + IrqSafe dedupe + honesty docs  
**Canvas:** `ticket-lock-bughunt-s378.canvas.tsx`

## Premissas

- FIFO ticket spinlock bare-metal; não-reentrante (SESSION_316)
- `#[repr(C)]` ticket@0 / serving@8 (x86_64) p/ instrumentos
- IRQ path → IrqSafeLock (cli), não TicketLock puro
- AIOS ADR-0088; não trocar por spin unfair sem evidência

## Aplicado

| ID | Fix |
|----|-----|
| H1 | `unsafe impl<T: Sync> Sync for TicketLockGuard` (+ IrqSafeGuard) |
| H2 | 7 testes host (exclusão, try_lock, FIFO threads, Cell, getters) |
| H3 | codemap: FIFO ≠ MCS / own cache line |
| M1 | rustdoc contrato IRQ/reentrância |
| M2 | `try_lock` CAS, `get_mut`, `into_inner`, `ticket`/`serving`/`is_locked` |
| M3 | `IrqSafeLock` = wrapper `TicketLock` + ManuallyDrop (fonte única) |
| L1 | delete stub `src/codemap.md` |
| L2 | spin 0.12 / lock_api = IDEA #602 residual |

## Verificação

- `cargo test -p ticket-lock` → 7/7
- `cargo check -p k-nano` / `event-bus` / `neural-kernel --release` → 0 erros

## Residual

- IDEA #602: política spin vs ticket no workspace; lock_api opcional
