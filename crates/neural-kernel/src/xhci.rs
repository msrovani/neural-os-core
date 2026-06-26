// xHCI USB 3.0 Controller Driver — Sprint 29
// Register definitions for Intel xHCI (QEMU q35 + nec-usb-xhci)

use crate::pci::PciDevice;
use crate::memory::PHYS_MEM_OFFSET;
use crate::serial_println;
use core::sync::atomic::Ordering;

pub const XHCI_VENDOR: u16 = 0x1B21; // ASMedia (QEMU default)
pub const XHCI_DEVICE: u16 = 0x7023; // xHCI Controller

const XHCI_CLASS: u8 = 0x0C;
const XHCI_SUBCLASS: u8 = 0x03;
const XHCI_PROG_IF_XHCI: u8 = 0x30;

// Capability Registers
#[repr(C, packed)]
struct CapReg {
    cap_length: u8,        // 0x00
    _rsvd: u8,             // 0x01
    hci_version: u16,      // 0x02
    hcs_params1: u32,      // 0x04 — HCSPARAMS1 (ports, slots)
    hcs_params2: u32,      // 0x08 — HCSPARAMS2 (ist, erst)
    hcs_params3: u32,      // 0x0C — HCSPARAMS3 (u2, psic)
    cap_params: u32,       // 0x10 — CAPARAMS (ext cap pointer)
}

// Operational Registers
#[repr(C, packed)]
struct OpReg {
    usb_cmd: u32,          // 0x00 — USBCMD
    usb_sts: u32,          // 0x04 — USBSTS
    page_size: u32,        // 0x08 — PAGESIZE
    _rsvd1: [u8; 4],       // 0x0C
    dn_ctrl: u32,          // 0x10 — DNCTRL
    crcr: u64,             // 0x18 — Command Ring Control
    _rsvd2: [u8; 8],       // 0x20
    dcbaap: u64,           // 0x28 — Device Context Base Array Pointer
    config: u32,           // 0x30 — CONFIG
}

pub struct XhciDriver {
    mmio_base: u64,
    slots: u8,
    ports: u8,
}

impl XhciDriver {
    pub unsafe fn new(dev: &PciDevice) -> Option<Self> {
        if dev.class != XHCI_CLASS || dev.subclass != XHCI_SUBCLASS {
            return None;
        }
        let pmoff = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        let bar0 = (dev.bar0 & !0xF) as u64;
        let mmio_base = bar0 + pmoff;
        serial_println!("[XHCI] Controller detectado: {:02x}:{:02x}.{} {:04x}:{:04x}",
            dev.bus, dev.device, dev.function, dev.vendor_id, dev.device_id);
        Some(XhciDriver { mmio_base, slots: 0, ports: 0 })
    }

    pub unsafe fn init(&mut self) -> bool {
        let cap = &*(self.mmio_base as *const CapReg);
        self.slots = ((cap.hcs_params1 >> 8) & 0xFF) as u8;
        self.ports = (cap.hcs_params1 & 0xFF) as u8;
        serial_println!("[XHCI] Init: {} slots, {} portas", self.slots, self.ports);
        true
    }
}
