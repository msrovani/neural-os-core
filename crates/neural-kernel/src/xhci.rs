use alloc::vec::Vec;
use crate::pci::PciDevice;
use crate::memory::{GLOBAL_ALLOCATOR, PHYS_MEM_OFFSET};
use crate::serial_println;
use core::sync::atomic::Ordering;

const PORTSC: u64 = 0x400;
const PORTSC_CCS: u32 = 0x01;

pub struct UsbDevice {
    pub port: u8,
    pub speed: u8,
    pub vendor_id: u16,
    pub product_id: u16,
}

pub struct XhciDriver {
    mmio: u64,
    cap: u64,
    op: u64,
    pub slots: u8,
    pub ports: u8,
}

impl XhciDriver {
    pub unsafe fn new(dev: &PciDevice) -> Option<Self> {
        if dev.class != 0x0C || dev.subclass != 0x03 { return None; }
        let pmoff = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        let mmio = ((dev.bar0 & !0xF) as u64) + pmoff;
        let capl = core::ptr::read_volatile(mmio as *const u8) as u64;
        let op = mmio + capl;
        let hcs1 = core::ptr::read_volatile((mmio + 4) as *const u32);
        let ports = (hcs1 & 0xFF) as u8;
        let slots = ((hcs1 >> 8) & 0xFF) as u8;
        Some(XhciDriver { mmio, cap: mmio, op, slots, ports })
    }

    pub unsafe fn init(&mut self) -> bool {
        serial_println!("[XHCI] Controller: {} slots, {} portas", self.slots, self.ports);
        true
    }

    pub unsafe fn port_scan(&self) -> Vec<UsbDevice> {
        let mut devices = Vec::new();
        for port in 0..self.ports.min(8) {
            let portsc = core::ptr::read_volatile((self.op + PORTSC + port as u64 * 0x10) as *const u32);
            if portsc & PORTSC_CCS == 0 { continue; }
            let speed = ((portsc >> 20) & 0x0F) as u8;
            let speed_name = match speed {
                1 => "Low (1.5 Mbps)", 2 => "Full (12 Mbps)", 3 => "High (480 Mbps)",
                4 => "Super (5 Gbps)", 5 => "Super+ (10 Gbps)", _ => "Unknown",
            };
            serial_println!("[USB] Porta {}: {} ({})", port, speed_name, self.speed_desc(speed));
            devices.push(UsbDevice { port, speed, vendor_id: 0, product_id: 0 });
        }
        devices
    }

    fn speed_desc(&self, speed: u8) -> &'static str {
        match speed {
            1 => "dispositivo USB 1.x (teclado, mouse)",
            2 => "dispositivo USB 2.0 (full speed)",
            3 => "dispositivo USB 2.0 (high speed, pendrive)",
            4 => "dispositivo USB 3.0 (super speed, SS)",
            5 => "dispositivo USB 3.1 (super speed+),",
            _ => "dispositivo USB de velocidade desconhecida",
        }
    }
}
