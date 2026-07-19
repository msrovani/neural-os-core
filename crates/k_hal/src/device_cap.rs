//! DeviceCap — identidade e classe de dispositivo (ADR-0041 H1).
//! Sem MMIO aqui; só metadados e autoridade lógica.
//! Classes são HalOffer-shaped (API R3); VirtIO é só transporte BE.

/// Classe lógica ofertável via HalOffer (não confundir com VirtIO OASIS).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DeviceClass {
    Unknown = 0,
    Gpu = 1,
    Net = 2,
    Wifi = 3,
    Block = 4,
    Snd = 5,
    Input = 6,
    Display = 7,
    /// Câmera / UVC (não confundir com host xHCI genérico).
    Video = 8,
    /// Host USB xHCI (ADR-0056) — filhos HID/MSC/UAC/BT após EP0.
    UsbHost = 9,
    /// Bluetooth HCI (combo WiFi ou dongle USB).
    Bluetooth = 10,
}

impl DeviceClass {
    pub fn as_str(self) -> &'static str {
        match self {
            DeviceClass::Unknown => "unknown",
            DeviceClass::Gpu => "gpu",
            DeviceClass::Net => "net",
            DeviceClass::Wifi => "wifi",
            DeviceClass::Block => "block",
            DeviceClass::Snd => "snd",
            DeviceClass::Input => "input",
            DeviceClass::Display => "display",
            DeviceClass::Video => "video",
            DeviceClass::UsbHost => "usbhost",
            DeviceClass::Bluetooth => "bluetooth",
        }
    }
}

/// Identidade PCI / lógica.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceId {
    pub vendor_id: u16,
    pub device_id: u16,
    pub class: DeviceClass,
    pub pci_bus: u8,
    pub pci_dev: u8,
    pub pci_fn: u8,
    pub bar0: u64,
    pub is_integrated: bool,
}

/// Capability publicada no DeviceTree (não é Cap bitflag de syscall).
#[derive(Debug, Clone, Copy)]
pub struct DeviceCap {
    pub id: DeviceId,
    pub name: &'static str,
    /// Backend nativo ou VirtIO FE/BE (transporte).
    pub virtio_bound: bool,
    pub compute_candidate: bool,
    pub has_display: bool,
    /// Bound = BE registrado; ≠ Ready/golden.
    pub bound: bool,
    /// ADR-0056: RecipePromote as u8 (0=Ok … 3=None).
    pub recipe_promote: u8,
}

impl DeviceCap {
    pub const fn unbound(id: DeviceId, name: &'static str) -> Self {
        DeviceCap {
            id,
            name,
            virtio_bound: false,
            compute_candidate: false,
            has_display: false,
            bound: false,
            recipe_promote: 3, // None
        }
    }
}
