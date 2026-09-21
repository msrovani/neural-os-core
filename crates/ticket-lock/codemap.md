# crates/ticket-lock/

## Responsibility

Minimal no_std **FIFO ticket spinlock** — sync primitivo do workspace. 1 arquivo:
`src/lib.rs`; zero dependencies.

## Design (SESSION_378 honesty)

`TicketLock<T>` — `#[repr(C)]` com `AtomicUsize ticket` @0, `serving` @8 (x86_64),
`UnsafeCell<T>`:

- `const fn new` — statics
- `lock()` — `fetch_add` ticket + spin `serving.load(Acquire)` + `spin_loop`
- `try_lock()` — CAS (sem ticket órfão)
- `get_mut` / `into_inner` / `ticket()` / `serving()` / `is_locked()`
- Guard: `Deref`/`DerefMut`; `Drop` → `serving.fetch_add(1, Release)`
- `Send`/`Sync` do lock: `T: Send`
- `Sync` do **guard**: `T: Sync` (como `MutexGuard`)

FIFO entre tickets emitidos. **Não** é MCS: todos os waiters fazem load no
**mesmo** `AtomicUsize serving` (false sharing clássico sob contenda alta).
“Sem starvation” só vale se o holder soltar o lock (não-reentrante; hold infinito
= fila parada). Wrap `usize` residual em x86_64.

## Contrato IRQ

Não usar em IRQ se a thread pode segurar o mesmo lock. Em IRQ: `IrqSafeLock`
(`k_nano::sync`, wrapper cli + este TicketLock) ou `try_lock`.

## Integration

`event-bus`, `k_nano` (PMM/`IrqSafeLock`), hermes globals, neural-kernel statics.
Upstream: **não** substituir por `spin` 0.12 (unfair) sem evidência — IDEA residual.
