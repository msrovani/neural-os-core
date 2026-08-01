# crates/neural-kernel/src/task/

Minimal `no_std` async primitives (single `mod.rs`): a no-op `RawWakerVTable` +
`dummy_waker()` and a single-pass `yield_now()` future.

## Key symbols

`dummy_waker()`, `yield_now()`.

## Integration

Legacy async helper for bare futures. The modern async runtime is `k_nano::async_rt`
(ADR-0065 FASE 3.2, timer-queue + `process_wakes` from the timer IRQ); this module
remains as a small utility and has no boot-path dependencies.
