# crates/ticket-lock/

## Responsibility

Minimal no_std ticket spinlock — the workspace's core synchronization primitive. 1 file: `src/lib.rs`; zero dependencies.

## Design

`TicketLock<T>` holds two `AtomicUsize` counters (`ticket`, `serving`) around an `UnsafeCell<T>`:
- `const fn new(value)` — usable in statics.
- `lock() -> TicketLockGuard` — `fetch_add(1, Relaxed)` takes a ticket, then spins on `spin_loop()` until `serving.load(Acquire) == my_ticket`.
- `TicketLockGuard` derefs to `&T`/`&mut T`; `Drop` does `serving.fetch_add(1, Release)` to hand off to the next waiter.
- `unsafe impl Send/Sync for TicketLock<T> where T: Send` — safe because all access goes through the guard.

FIFO ticket ordering guarantees fairness (no starvation) and each waiter spins on its own cache line via the monotonically increasing `serving` counter.

## Flow

`lock()` → acquire ticket → spin until served → guard exposes `&mut T` → `drop` releases the next waiter. Contention is handled entirely by atomics + `spin_loop`; no OS support needed (bare-metal friendly).

## Integration

Used by `event-bus` (EventBus subscriber map, `Receiver` queues, `BoundedChannel`, `MessageBus`, LatentBus) and other no_std crates in the workspace. `ponytail:` nothing else needed — upgrade to a sleeping lock only if a core ever blocks on a long-held lock.
