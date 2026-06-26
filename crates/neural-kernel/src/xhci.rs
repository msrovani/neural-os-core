use alloc::vec::Vec;
use crate::pci::PciDevice;
use crate::memory::PHYS_MEM_OFFSET;
use crate::serial_println;
use core::sync::atomic::Ordering;

const PORTSC_OFFSET: u64 = 0x400; // PORTSC starts at reg offset 0x400
const PORTSC_SIZE: u64 = 0x10;    // Each PORTSC is 16 bytes
const PORTSC_CCS: u32 = 0x0000_0001;  // Current Connect Status
const PORTSC_PED: u32 = 0x0000_0002;  // Port Enabled/Disabled
const PORTSC_SPEED_SHIFT: u32 = 20;   // Port Speed bits
const PORTSC_SPEED_MASK: u32 = 0x0F;  // 4 bits

#[repr(C, packed)]
struct CapReg {
    cap_length: u8,
    _rsvd: u8,
    hci_version: u16,
    hcs_params1: u32,
    hcs_params2: u32,
    hcs_params3: u32,
    cap_params: u32,
}

#[repr(C, packed)]
struct OpReg {
    usb_cmd: u32,
    usb_sts: u32,
    page_size: u32,
    _rsvd1: [u8; 4],
}

pub struct UsbDevice {
    pub port: u8,
    pub speed: u8,
    pub vendor_id: u16,
    pub product_id: u16,
}

pub struct XhciDriver {
    mmio_base: u64,
    cap_reg: u64,
    op_reg: u64,
    pub slots: u8,
    pub ports: u8,
}

impl XhciDriver {
    pub unsafe fn new(dev: &PciDevice) -> Option<Self> {
        if dev.class != 0x0C || dev.subclass != 0x03 {
            return None;
        }
        let pmoff = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        let bar0 = (dev.bar0 & !0xF) as u64;
        let mmio_base = bar0 + pmoff;
        let cap = &*(mmio_base as *const CapReg);
        let cap_length = cap.cap_length as u64;
        serial_println!("[XHCI] Controller: {:02x}:{:02x}.{} {:04x}:{:04x}",
            dev.bus, dev.device, dev.function, dev.vendor_id, dev.device_id);
        Some(XhciDriver {
            mmio_base,
            cap_reg: mmio_base,
            op_reg: mmio_base + cap_length,
            slots: 0,
            ports: 0,
        })
    }

    pub unsafe fn init(&mut self) -> bool {
        let caps = &*(self.cap_reg as *const CapReg);
        self.slots = ((caps.hcs_params1 >> 8) & 0xFF) as u8;
        self.ports = (caps.hcs_params1 & 0xFF) as u8;
        serial_println!("[XHCI] Init: {} slots, {} portas", self.slots, self.ports);
        true
    }

    unsafe fn read_portsc(&self, port: u8) -> u32 {
        let addr = self.op_reg + PORTSC_OFFSET + (port as u64) * PORTSC_SIZE;
        core::ptr::read_volatile(addr as *const u32)
    }

    pub unsafe fn port_scan(&self) -> Vec<UsbDevice> {
        let mut devices = Vec::new();
        for port in 0..self.ports {
            let portsc = self.read_portsc(port);
            if portsc & PORTSC_CCS == 0 {
                continue;
            }
            let speed = ((portsc >> PORTSC_SPEED_SHIFT) & PORTSC_SPEED_MASK) as u8;
            let speed_name = match speed {
                1 => "Low (1.5 Mbps)",
                2 => "Full (12 Mbps)",
                3 => "High (480 Mbps)",
                4 => "Super (5 Gbps)",
                5 => "Super+ (10 Gbps)",
                _ => "Unknown",
            };
            serial_println!("[USB] Porta {}: conectado, velocidade {}", port, speed_name);
            devices.push(UsbDevice {
                port,
                speed,
                vendor_id: 0,
                product_id: 0,
            });
        }
        devices
    }
}
