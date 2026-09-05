//! k-hal — Ring 1 / L1 sensório-motor (ADR-0041 §9).
//! Descoberta + DeviceCap + ports + HalOffer (API R3); backends MMIO + VirtIO transporte.
//! Sem persona, sem LLM, sem Trust — só silício e filas.
//!
//! Log: `slog_hal!(Item, subitem, "…")` → `[T+n] [R1] [k-hal] [Item] [subitem] - …`

#![cfg_attr(not(test), no_std)]
#![feature(abi_x86_interrupt)]
#![allow(dead_code)]
#![allow(unused_unsafe)]
#![allow(static_mut_refs)]
#![allow(unused_variables)]
#![allow(unused_imports)]

extern crate alloc;

pub mod device_cap;
pub mod device_recipe;
pub mod fat_assets;
pub mod lego_boot;
pub mod unlock_dag;
pub mod hw_gate;
pub mod discovery;
pub mod pci_bar;
pub mod compute_port;
pub mod net_port;
pub mod display_port;
pub mod audio_port;
pub mod video_port;
pub mod offer;
pub mod cap_gate;
pub mod virtio;
pub mod net;
pub mod audio;
pub mod gpu;
pub mod npu;
pub mod usb;

use core::sync::atomic::{AtomicBool, Ordering};

static H1_RAN: AtomicBool = AtomicBool::new(false);

/// Bring-up H1: DeviceTree + UnlockDAG tokens + HalOffer (ADR-0056).
/// Idempotente: o 1º call popula PCI; calls seguintes só refrescam a oferta
/// (não `clear_tree` — senão o plano k_ai do boot some).
pub fn init_h1() -> usize {
    if H1_RAN.load(Ordering::Relaxed) {
        offer::refresh_from_tree();
        crate::usb::install_bringup_hooks();
        return discovery::device_count();
    }
    let n = discovery::populate_from_pci();
    let fat = device_recipe::fat_readable_hint();
    unlock_dag::boot_platform_tokens(n > 0, fat);
    offer::refresh_from_tree();
    H1_RAN.store(true, Ordering::Relaxed);
    // USB host BE: hub→MSC vive em k_hal; registra hook antes do DriverInit probe.
    crate::usb::install_bringup_hooks();
    k_nano::slog_hal!(
        "DeviceCap",
        "ready",
        "devices={} fat={} unlock={:#x} compute={:?} net={:?}",
        n,
        fat,
        unlock_dag::token_mask(),
        compute_port::status().status,
        net_port::status()
    );
    n
}

/// Alias estável.
pub fn init() -> usize {
    init_h1()
}

pub fn device_tree() -> alloc::vec::Vec<device_cap::DeviceCap> {
    discovery::device_tree()
}

pub fn compute() -> compute_port::ComputeStatus {
    compute_port::status()
}
