//! DeviceTree — snapshot para k_ai / boot (ADR-0041 H1).
//! Alimentado por PCI scan + binds; sem política Trust aqui.

use crate::device_cap::{DeviceCap, DeviceClass, DeviceId};
use alloc::vec::Vec;
use k_nano::pci::PciDevice;
use spin::Mutex;

const MAX_DEVICES: usize = 64;

static DEVICE_TREE: Mutex<Vec<DeviceCap>> = Mutex::new(Vec::new());

/// Snapshot atual (cópia).
pub fn device_tree() -> Vec<DeviceCap> {
    DEVICE_TREE.lock().clone()
}

pub fn device_count() -> usize {
    DEVICE_TREE.lock().len()
}

pub fn clear_tree() {
    DEVICE_TREE.lock().clear();
}

/// Registra ou atualiza por (bus,dev,fn).
pub fn register_device(cap: DeviceCap) {
    let mut tree = DEVICE_TREE.lock();
    if let Some(slot) = tree.iter_mut().find(|c| {
        c.id.pci_bus == cap.id.pci_bus
            && c.id.pci_dev == cap.id.pci_dev
            && c.id.pci_fn == cap.id.pci_fn
    }) {
        *slot = cap;
        return;
    }
    if tree.len() < MAX_DEVICES {
        tree.push(cap);
    }
}

fn class_from_pci(dev: &PciDevice) -> DeviceClass {
    match (dev.class, dev.subclass) {
        (0x03, _) => DeviceClass::Gpu,
        (0x02, 0x80) => DeviceClass::Wifi, // network other — often wifi
        (0x02, _) => DeviceClass::Net,
        (0x01, _) => DeviceClass::Block,
        (0x04, _) => DeviceClass::Snd,
        // xHCI: UsbHost (ADR-0056); UVC continua DeviceClass::Video via oferta derivada
        (0x0C, 0x03) => DeviceClass::UsbHost,
        (0x0C, _) => DeviceClass::Input,
        _ => DeviceClass::Unknown,
    }
}

fn name_hint(vid: u16, class: DeviceClass) -> &'static str {
    match (vid, class) {
        (0x8086, DeviceClass::Gpu) => "Intel GPU",
        (0x10DE, DeviceClass::Gpu) => "NVIDIA GPU",
        (0x1002, DeviceClass::Gpu) => "AMD GPU",
        (0x1AF4, DeviceClass::Gpu) => "VirtIO-GPU",
        (0x1AF4, DeviceClass::Net) => "VirtIO-net",
        (_, DeviceClass::Wifi) => "WiFi",
        (_, DeviceClass::Net) => "Ethernet",
        (_, DeviceClass::Snd) => "Audio",
        (_, DeviceClass::Block) => "Block",
        (_, DeviceClass::UsbHost) => "xHCI USB host",
        (_, DeviceClass::Video) => "UVC / camera",
        (_, DeviceClass::Bluetooth) => "Bluetooth",
        (_, DeviceClass::Input) => "USB/HID",
        _ => "PCI device",
    }
}

/// Enumera PCI display/net/storage/multimedia → DeviceTree (H1; bind residual).
pub fn populate_from_pci() -> usize {
    clear_tree();
    let devices = unsafe { k_nano::pci::scan_pci() };
    let mut n = 0usize;
    for dev in &devices {
        let class = class_from_pci(dev);
        if matches!(
            class,
            DeviceClass::Unknown
        ) && dev.class != 0x03
            && dev.class != 0x02
            && dev.class != 0x01
            && dev.class != 0x04
        {
            continue;
        }
        let class = if dev.class == 0x03 {
            DeviceClass::Gpu
        } else {
            class
        };
        let bar0 = crate::pci_bar::decode_bar(dev.bar0, dev.bar1);
        let virtio = dev.vendor_id == 0x1AF4;
        let id = DeviceId {
            vendor_id: dev.vendor_id,
            device_id: dev.device_id,
            class,
            pci_bus: dev.bus,
            pci_dev: dev.device,
            pci_fn: dev.function,
            bar0,
            is_integrated: matches!(dev.vendor_id, 0x8086 | 0x1002) && dev.class == 0x03,
        };
        let mut cap = DeviceCap::unbound(id, name_hint(dev.vendor_id, class));
        cap.virtio_bound = virtio;
        cap.has_display = class == DeviceClass::Gpu;
        cap.compute_candidate = class == DeviceClass::Gpu && !virtio;
        let promote = crate::device_recipe::log_match(dev.vendor_id, dev.device_id, class);
        cap.recipe_promote = promote as u8;
        register_device(cap);
        n += 1;
    }
    k_nano::slog_hal!("DeviceTree", "populate", "devices={}", n);
    n
}

/// Marca dispositivo bound (BE registrado — ≠ golden Ready).
pub fn mark_bound(bus: u8, dev: u8, func: u8, bound: bool) {
    let mut tree = DEVICE_TREE.lock();
    if let Some(c) = tree.iter_mut().find(|c| {
        c.id.pci_bus == bus && c.id.pci_dev == dev && c.id.pci_fn == func
    }) {
        c.bound = bound;
    }
}
