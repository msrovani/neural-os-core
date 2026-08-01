# crates/neural-kernel/src/smp/

SMP bring-up orchestration over the k_nano trampoline (ADR-0055/0057). `init_smp()` is
gated by `k_nano::platform_probe::allow_smp()` + APIC presence; BSP-only paths fall back
to `percpu::init_bsp_percpu` + `corepools::init_from_boot`; SIPI path wakes APs and counts
them via `AP_COUNT` (from MADT, capped by `max_aps`).

## Key symbols

`init_smp()`, `AP_COUNT`, `ap_entry_count()` (delegates `k_nano::smp::ap_entry_count`),
`percpu::PerCpu` (BSP state), `spsc::SpscQueue` (lock-free cross-core queue),
`parallel_matmul` (re-export `cortex_crate::parallel_matmul`), `trampoline` +
`work_stealing` (re-exports of `k_nano::smp`).

## Integration

Called from `agents::init_platform_sync()`; `k_nano::smp::install_wake_fn(apic::send_ipi_reschedule)`
installed at boot; `cortex::parallel_*` matmul is gated on `k_nano::smp::ap_pollable()`
(APs as live workers require shared IDT + per-core IPI = residual HW).
