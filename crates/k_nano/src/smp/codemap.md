# crates/k_nano/src/smp/ — SMP Bring-Up & AP Workers

**Responsibility**: multi-core enablement — real-mode trampoline, directed AP wake,
per-AP PerCpu/TSS/IST, and a barrier-based AP work queue (ADR-0055/0057).

**Key symbols**: `init_smp()`, `wake_aps_sequential()` (INIT-SIPI-SIPI ×3 retry per LAPIC
ID from `acpi::BOOT_APIC_IDS`), `ap_entry` (pub — bin reuses it), `AP_ENTRY_COUNTER`,
`AP_COUNT`, `ap_entry_count()`, `total_cores()`, `ap_pollable()`/`set_ap_pollable()`
(gate for `cortex::parallel_*`), `install_wake_fn()` (WS-F seam); `percpu::{MAX_APS,
AP_PCPU, CPU_COUNT, init_bsp_percpu, this_cpu}`, `trampoline::init_trampoline`,
`ap_work::{ap_idle_loop, enqueue, wait_barrier}`, `corepools::init_from_boot`,
`spsc` SPSC queue, `work_stealing::init_global_pool`.

**Integration**: called from bin `init_platform_sync` after `apic::init_apic`; APs load
shared IDT + per-AP TSS via `interrupts::{init_ap_tss, ap_load_idt_and_tss}` and signal
`AP_IDT_READY`; `core_pinning::init_pools` runs after. Gated BSP-only when
`platform_probe::allow_smp()` is false. See crate root map, "SMP AP wake".
