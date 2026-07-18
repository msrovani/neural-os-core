//! SMP trampoline — ADR-0055: usa blob k_nano (global_asm) quando FeatureGate.allow_smp.
//! Em WHPX/VBox `sipi_ready()` = false → BSP only.

use core::sync::atomic::Ordering;

/// SIPI só se FeatureGate permitir SMP (WHPX/VBox = false).
#[inline]
pub fn sipi_ready() -> bool {
    k_nano::platform_probe::allow_smp()
}

pub unsafe fn init_trampoline(
    phys_addr: u64,
    cr3_value: u64,
    ap_stack: u64,
    percpu_addr: u64,
    entry_fn: extern "C" fn(u64) -> !,
) {
    k_nano::smp::trampoline::init_trampoline(
        phys_addr,
        cr3_value,
        ap_stack,
        percpu_addr,
        entry_fn,
    );
    k_nano::slog_nano!(
        "SMP",
        "info",
        "trampoline k_nano ready (FeatureGate allow_smp={})",
        sipi_ready()
    );
    let _ = Ordering::Relaxed;
}

pub unsafe fn trampoline_size() -> usize {
    k_nano::smp::trampoline::trampoline_size()
}
