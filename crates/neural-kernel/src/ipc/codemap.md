# crates/neural-kernel/src/ipc/

Cross-address-space IPC proof-of-concept (ADR-0041 MVP-C, 2 files): `ring_buffer.rs`
provides `SharedSpscRing` (lock-free SPSC ring initialized inside a shared page);
`mod.rs` wires `demo_two_spaces()` — clones two CR3 spaces, maps a shared ring + private
pages, switches between them, exchanges bytes, and verifies private-page isolation.

## Key symbols

`SharedSpscRing`, `demo_two_spaces()`.

## Integration

`demo_two_spaces()` called from `kernel_boot()` (non-fatal, logs WARN on Err); uses
`address_space::{clone_current, activate, restore_cr3}` and `syscall::{Cap, SYS_PING}`.
The real IPC/runtime layer is `k_nano` (EventBus, SPSC, async_rt) and hermes `ipc_bus`.
