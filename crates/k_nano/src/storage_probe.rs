//! Probe de storage na ordem do plano `boot_bind` (ADR-0088 / IDEA #513).
//! Bin só chama `probe_storage_drivers()` — sem ATA-first hardcoded.

use crate::boot_bind::{storage_probe_order, StorageKind};
use crate::globals::{AHCI_DRIVER, ATA_DRIVER, USB_MSC};

/// Traz NVMe/AHCI/USB/ATA na ordem observada. Idempotente por backend.
pub unsafe fn probe_storage_drivers() {
    let (order, n) = storage_probe_order();
    crate::slog_nano!(
        "Disk",
        "bind",
        "storage plan n={} [0]={} [1]={} [2]={} [3]={}",
        n,
        order[0].as_str(),
        order[1].as_str(),
        order[2].as_str(),
        order[3].as_str()
    );
    for i in 0..n {
        match order[i] {
            StorageKind::Nvme => probe_nvme(),
            StorageKind::Ahci => {
                let _ = crate::ahci::AhciDriver::probe_first();
            }
            StorageKind::UsbHost => probe_usb_msc(),
            StorageKind::Ata => probe_ata(),
            StorageKind::None => {}
        }
    }
}

unsafe fn probe_ata() {
    if ATA_DRIVER.lock().is_some() {
        return;
    }
    *ATA_DRIVER.lock() = crate::ata::AtaDriver::probe();
    let ok = ATA_DRIVER.lock().is_some();
    crate::slog_nano!("Disk", "bind", "ata-pio ok={}", ok);
}

unsafe fn probe_nvme() {
    if crate::disk_agent::nvme::NVME_DRIVER.lock().is_some() {
        return;
    }
    if let Some(nvme) = crate::disk_agent::nvme::NvmeDriver::probe() {
        *crate::disk_agent::nvme::NVME_DRIVER.lock() = Some(nvme);
        crate::slog_nano!("Disk", "bind", "nvme ok=true");
    } else {
        crate::slog_nano!("Disk", "bind", "nvme ok=false");
    }
}

unsafe fn probe_usb_msc() {
    if USB_MSC.lock().is_some() {
        return;
    }
    // Early path ja tentou; QEMU Enable Slot timeout nao muda no retry.
    let qemu = crate::platform_probe::probe_done()
        && !matches!(
            crate::platform_probe::hypervisor(),
            crate::platform_probe::HypervisorKind::None
        );
    if qemu {
        crate::slog_nano!("USB", "msc", "plan probe skip (qemu, early ja tentou)");
        return;
    }
    let msc = crate::usb_msc::UsbMassStorage::probe();
    let ok = msc.is_some();
    *USB_MSC.lock() = msc;
    crate::slog_nano!("USB", "msc", "plan probe ok={}", ok);
}

/// AHCI: 1º controlador 01:06. Idempotente.
#[allow(dead_code)]
pub unsafe fn ahci_occupied() -> bool {
    AHCI_DRIVER.lock().is_some()
}
