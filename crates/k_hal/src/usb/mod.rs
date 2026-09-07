//! USB host BE (R1) — xHCI bring-up de silício sob HalOffer UsbHost.
//!
//! `k_nano::xhci` expõe só primitivos R0 (MMIO/rings/TRB). Política de
//! enumeração (root + hub route/TT → MSC) vive aqui — ADR-0041 / AIOS.
//!
//! Padrão validado em Redox (`xhcid`+`usbhubd`) e Chitti: em notebooks o
//! stick USB-A quase sempre está atrás de hub interno, não em root port.

pub mod hub_msc;

use core::sync::atomic::{AtomicBool, Ordering};

static HOOKS_INSTALLED: AtomicBool = AtomicBool::new(false);

/// Instala o bring-up MSC do R1 no hook R0 (`usb_msc` / DriverInit).
/// Idempotente — chamar cedo no boot (após `k_hal::init` / pré-probe USB).
pub fn install_bringup_hooks() {
    if HOOKS_INSTALLED.swap(true, Ordering::AcqRel) {
        return;
    }
    k_nano::xhci::register_msc_bringup(hub_msc::bringup_boot_msc);
    k_nano::slog_hal!(
        "USB",
        "ok",
        "R1 MSC bringup hook installed (hub+route+TT)"
    );
}

/// Probe + instala `USB_MSC` global (DriverInit / HalOffer UsbHost).
pub unsafe fn probe_and_install() -> bool {
    install_bringup_hooks();
    let msc = k_nano::usb_msc::UsbMassStorage::probe();
    let ok = msc.is_some();
    if ok {
        *k_nano::globals::USB_MSC.lock() = msc;
        crate::unlock_dag::grant(crate::unlock_dag::CapToken::UsbHostSched);
        crate::unlock_dag::grant(crate::unlock_dag::CapToken::UsbPortReady);
        k_nano::slog_hal!("USB", "ok", "MSC installed via k_hal::usb");
    } else {
        k_nano::slog_hal!("USB", "warn", "MSC probe FAIL (hub/root)");
    }
    k_nano::display::fb::boot_ckpt(
        190,
        if ok { "USB-MSC k_hal OK (hub+root)" } else { "USB-MSC k_hal FAIL (hub/root)" },
    );
    ok
}
