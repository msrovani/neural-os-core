//! VirtIO — transporte OASIS/QEMU (ADR-0041 H4+).
//! NÃO é HalOffer (API R3). QUEUE_NOTIFY real em VirtIO-PCI após map UC.

use crate::device_cap::DeviceClass;
use crate::discovery;
use core::sync::atomic::Ordering;

/// Offset QUEUE_NOTIFY (VirtIO-MMIO transport).
pub const VIRTIO_MMIO_QUEUE_NOTIFY: u64 = 0x050;
pub const VIRTIO_MMIO_MAGIC: u32 = 0x7472_6976;

/// VirtIO PCI capability cfg_type.
pub const VIRTIO_PCI_CAP_COMMON_CFG: u8 = 1;
pub const VIRTIO_PCI_CAP_NOTIFY_CFG: u8 = 2;
// OASIS: 1=common, 2=notify — wait, jarbas uses 1 for common (notify in modern).
// VirtIO 1.1 §4.1.4: VIRTIO_PCI_CAP_COMMON_CFG=1, NOTIFY_CFG=2, ISR=3, DEVICE=4, PCI=5
// jarbas read_virtio_cap(..., 1) for common cfg. Notify = 2.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtioStage {
    Absent,
    LayoutReady,
    NotifySent,
    NotifySkipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtioBackendKind {
    None,
    VirtioMmio,
    VirtioPci,
    Native,
}

static mut LAST_STAGE: VirtioStage = VirtioStage::Absent;
static mut NET_BE: VirtioBackendKind = VirtioBackendKind::None;
static mut GPU_BE: VirtioBackendKind = VirtioBackendKind::None;
static mut LAST_NOTIFY_PHYS: u64 = 0;

pub fn last_stage() -> VirtioStage {
    unsafe { LAST_STAGE }
}

pub fn net_backend() -> VirtioBackendKind {
    unsafe { NET_BE }
}

pub fn gpu_backend() -> VirtioBackendKind {
    unsafe { GPU_BE }
}

pub fn last_notify_phys() -> u64 {
    unsafe { LAST_NOTIFY_PHYS }
}

/// Tenta QUEUE_NOTIFY em VirtIO-MMIO já mapeado.
pub unsafe fn try_queue_notify(mmio_bar: u64, queue_idx: u32) -> VirtioStage {
    if crate::cap_gate::check_map_bar(1, true) == crate::cap_gate::CapResult::Deny {
        LAST_STAGE = VirtioStage::NotifySkipped;
        return VirtioStage::NotifySkipped;
    }
    if mmio_bar == 0 {
        k_nano::slog_hal!("VirtIO", "notify", "Absent — sem BAR");
        LAST_STAGE = VirtioStage::Absent;
        return VirtioStage::Absent;
    }
    let magic = core::ptr::read_volatile(mmio_bar as *const u32);
    if magic != VIRTIO_MMIO_MAGIC {
        k_nano::slog_hal!(
            "VirtIO",
            "notify",
            "LayoutReady magic={:#x} — NotifySkipped (nao MMIO virt)",
            magic
        );
        LAST_STAGE = VirtioStage::NotifySkipped;
        return VirtioStage::NotifySkipped;
    }
    core::ptr::write_volatile((mmio_bar + VIRTIO_MMIO_QUEUE_NOTIFY) as *mut u32, queue_idx);
    k_nano::slog_hal!(
        "VirtIO",
        "notify",
        "QUEUE_NOTIFY mmio q={} @ {:#x}",
        queue_idx,
        mmio_bar
    );
    LAST_STAGE = VirtioStage::NotifySent;
    LAST_NOTIFY_PHYS = mmio_bar;
    VirtioStage::NotifySent
}

/// Map UC de uma página BAR (R1 only). Retorna false se mapper falhar.
unsafe fn map_uc_page(phys: u64) -> bool {
    if crate::cap_gate::check_map_bar(1, true) == crate::cap_gate::CapResult::Deny {
        return false;
    }
    if phys == 0 || (phys >> 48) != 0 {
        return false;
    }
    let pmoff = k_nano::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    if pmoff == 0 {
        return false;
    }
    // map_page_uc = walk 4 níveis; map_mmio_page antigo quebrava L1 → #PF
    k_nano::apic::map_page_uc(phys & !0xFFF, pmoff);
    true
}

/// Resolve endereço físico do registro notify (VirtIO-PCI modern).
/// Retorna (notify_phys_page_base, notify_virt_for_write) ou None.
pub unsafe fn resolve_pci_notify(
    bus: u8,
    device: u8,
    function: u8,
    queue_idx: u16,
) -> Option<(u64, *mut u16)> {
    // cfg_type 2 = NOTIFY_CFG (VirtIO 1.1)
    let cap = k_nano::pci::read_virtio_cap(bus, device, function, 2)?;
    let bar_phys = k_nano::pci::read_bar_value(bus, device, function, cap.bar);
    if bar_phys == 0 || bar_phys == !0 || (bar_phys >> 48) != 0 {
        k_nano::slog_hal!(
            "VirtIO",
            "notify",
            "NotifySkipped — BAR{} invalido {:#x}",
            cap.bar,
            bar_phys
        );
        return None;
    }
    // notify_off_multiplier at cap_ptr+16 — need ptr from cap search
    let mult = read_notify_multiplier(bus, device, function).unwrap_or(0);
    let notify_phys = bar_phys
        .wrapping_add(cap.offset as u64)
        .wrapping_add((queue_idx as u64).wrapping_mul(mult as u64));
    if !map_uc_page(notify_phys) {
        return None;
    }
    let pmoff = k_nano::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    let virt = (notify_phys + pmoff) as *mut u16;
    // Probe: dead BAR?
    let probe = core::ptr::read_volatile(virt as *const u32);
    if probe == 0xffff_ffff {
        k_nano::slog_hal!(
            "VirtIO",
            "notify",
            "NotifySkipped — notify MMIO dead @ {:#x}",
            notify_phys
        );
        return None;
    }
    Some((notify_phys, virt))
}

unsafe fn read_notify_multiplier(bus: u8, device: u8, function: u8) -> Option<u32> {
    let caps = k_nano::pci::read_pci_capabilities(bus, device, function);
    for (cap_id, ptr) in &caps {
        if *cap_id != 0x09 {
            continue;
        }
        let cfg_type = k_nano::pci::read_config_byte(bus, device, function, ptr + 3);
        if cfg_type == 2 {
            // le32 at ptr+16
            return Some(k_nano::pci::read_config_dword(
                bus, device, function, ptr + 16,
            ));
        }
    }
    None
}

/// QUEUE_NOTIFY VirtIO-PCI (modern). Escreve queue_idx no notify register.
pub unsafe fn try_pci_queue_notify(
    bus: u8,
    device: u8,
    function: u8,
    queue_idx: u16,
) -> VirtioStage {
    match resolve_pci_notify(bus, device, function, queue_idx) {
        None => {
            LAST_STAGE = VirtioStage::NotifySkipped;
            VirtioStage::NotifySkipped
        }
        Some((phys, virt)) => {
            core::ptr::write_volatile(virt, queue_idx);
            k_nano::slog_hal!(
                "VirtIO",
                "notify",
                "QUEUE_NOTIFY pci q={} @ {:#x} bus={}:{}:{}",
                queue_idx,
                phys,
                bus,
                device,
                function
            );
            LAST_STAGE = VirtioStage::NotifySent;
            LAST_NOTIFY_PHYS = phys;
            VirtioStage::NotifySent
        }
    }
}

pub fn select_backends_from_tree() {
    let tree = discovery::device_tree();
    let mut net = VirtioBackendKind::Native;
    let mut gpu = VirtioBackendKind::Native;
    let mut any_virtio = false;
    for c in &tree {
        if c.id.vendor_id != 0x1AF4 {
            continue;
        }
        any_virtio = true;
        match c.id.class {
            DeviceClass::Net | DeviceClass::Wifi => net = VirtioBackendKind::VirtioPci,
            DeviceClass::Gpu | DeviceClass::Display => gpu = VirtioBackendKind::VirtioPci,
            _ => {}
        }
    }
    if !any_virtio {
        k_nano::slog_hal!(
            "VirtIO",
            "select",
            "sem VID 1AF4 — BE nativo (silicio/QEMU NIC legado)"
        );
    }
    unsafe {
        NET_BE = net;
        GPU_BE = gpu;
    }
    k_nano::slog_hal!(
        "VirtIO",
        "select",
        "BE net={:?} gpu={:?} (FE unico hermes/jarbas)",
        net,
        gpu
    );
}

/// H4+: classifica BE; map UC + QUEUE_NOTIFY real (ou NotifySkipped honesto).
pub fn bring_up_h4() {
    select_backends_from_tree();
    let tree = discovery::device_tree();
    let mut virtio_pci_n = 0u32;
    let mut sent = 0u32;
    for c in &tree {
        if c.id.vendor_id != 0x1AF4 {
            continue;
        }
        virtio_pci_n += 1;
        let stage = unsafe {
            try_pci_queue_notify(c.id.pci_bus, c.id.pci_dev, c.id.pci_fn, 0)
        };
        k_nano::slog_hal!(
            "VirtIO",
            "pci",
            "{:04x}:{:04x} bar0={:#x} class={} — {:?}",
            c.id.vendor_id,
            c.id.device_id,
            c.id.bar0,
            c.id.class.as_str(),
            stage
        );
        if stage == VirtioStage::NotifySent {
            sent += 1;
        }
    }
    if virtio_pci_n == 0 {
        k_nano::slog_hal!("VirtIO", "h4", "WARN: nenhum VirtIO PCI (layout-only OK)");
        unsafe {
            LAST_STAGE = VirtioStage::LayoutReady;
        }
    } else if sent > 0 {
        k_nano::slog_hal!(
            "VirtIO",
            "h4",
            "OK: {}/{} VirtIO-PCI QUEUE_NOTIFY enviado",
            sent,
            virtio_pci_n
        );
    } else {
        k_nano::slog_hal!(
            "VirtIO",
            "h4",
            "WARN: {} VirtIO-PCI — todos NotifySkipped (BAR/cap)",
            virtio_pci_n
        );
        unsafe {
            LAST_STAGE = VirtioStage::NotifySkipped;
        }
    }
    k_nano::slog_hal!(
        "SCL",
        "map",
        "control=k_ai cognition=cortex action=hermes hal=k_hal virtio=h4+"
    );
}

pub fn init_h4_log() {
    bring_up_h4();
}
