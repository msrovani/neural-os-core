# crates/k_nano/src/sync/ — Sync Primitives

**Responsibility**: IRQ-safe and lock-free synchronization for the kernel — FIFO ticket
lock that temporarily disables IRQs, a Lamport logical clock for P2P ordering, and an
MPMC channel.

**Key symbols**: `IrqSafeLock<T>` (+ `IrqSafeGuard`, exported at `sync::IrqSafeLock`),
`clock::{LogicalClock, VectorClock}` (re-exported via `p2p::clock`),
`mpmc::MpmcQueue<T>` (`try_send`/`try_recv`).

**Integration**: `IrqSafeLock` backs `mhi::MHI_REGISTRY`/`MIGRATION_QUEUE` (used by
disk_agent tiering); complements `ticket_lock::TicketLock` (spin) used by the global
allocator and skill registry.
