//! GPU Direct Storage stub — IDEA #423 (NVMe→VRAM sem CPU).
//! Onda 5: path honesto AWAITING_HW; sem fake Ready / sem P2P inventado.

use core::sync::atomic::{AtomicBool, Ordering};

static LOGGED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GdsStatus {
    /// QEMU VirtIO / sem NVMe+dGPU.
    Unsupported,
    /// Hardware presente mas peer DMA nao wired.
    AwaitingHw,
}

/// Probe one-shot no boot — nunca promove Ready.
pub fn probe_gds() -> GdsStatus {
    let status = GdsStatus::AwaitingHw;
    if !LOGGED.swap(true, Ordering::Relaxed) {
        k_nano::slog_bin!(
            "GDS-HW",
            "info",
            "step=nvme_to_vram status=UNSUPPORTED detail=need_nvme_and_dgpu_p2p"
        );
        k_nano::slog_bin!(
            "GDS-HW",
            "info",
            "VERDICT=AWAITING_REAL_HW reason=gpu_direct_storage_unwired"
        );
    }
    status
}
