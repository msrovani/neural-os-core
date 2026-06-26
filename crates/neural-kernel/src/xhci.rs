use alloc::vec::Vec;
use crate::pci::PciDevice;
use crate::memory::{PHYS_MEM_OFFSET};
use crate::serial_println;
use core::sync::atomic::Ordering;

pub struct UsbDevice {
    pub port: u8,
    pub speed: u8,
    pub vendor_id: u16,
    pub product_id: u16,
}

pub struct XhciDriver {
    op: u64,
    mmio: u64,
    capl: u64,
    db: u64,
    pub ports: u8,
    pub slots: u8,
}

impl XhciDriver {
    pub unsafe fn new(dev: &PciDevice) -> Option<Self> {
        if dev.class != 0x0C || dev.subclass != 0x03 { return None; }
        let pmoff = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        let mmio = ((dev.bar0 & !0xF) as u64) + pmoff;
        let capl = core::ptr::read_volatile(mmio as *const u8) as u64;
        let op = mmio + capl;
        let db_off = (core::ptr::read_volatile((mmio + 0x14) as *const u32) as u64) & !0x3;
        let hcs1 = core::ptr::read_volatile((mmio + 4) as *const u32);
        serial_println!("[XHCI] capl={} db_off=0x{:x} mmio=0x{:x}", capl, db_off, mmio);
        Some(XhciDriver { op, mmio, capl, db: mmio + db_off, ports: (hcs1 & 0xFF) as u8, slots: ((hcs1 >> 8) & 0xFF) as u8 })
    }

    pub unsafe fn init(&mut self) -> bool {
        // Map MMIO as uncacheable to prevent GPF on doorbell access
        crate::apic::set_page_uc(self.mmio - PHYS_MEM_OFFSET.load(Ordering::Relaxed), PHYS_MEM_OFFSET.load(Ordering::Relaxed));
        serial_println!("[XHCI] Init OK: {} portas, {} slots, db=0x{:x}", self.ports, self.slots, self.db);
        true
    }

    pub unsafe fn port_scan(&self) -> Vec<UsbDevice> {
        let mut devices = Vec::new();
        for port in 0..self.ports.min(8) {
            let portsc = core::ptr::read_volatile((self.op + 0x400 + port as u64 * 0x10) as *const u32);
            if portsc & 0x01 == 0 { continue; }
            let speed = ((portsc >> 20) & 0x0F) as u8;
            let desc = self.speed_name(speed);
            serial_println!("[USB] Porta {}: {} conectado (speed={})", port, desc, speed);
            devices.push(UsbDevice { port, speed, vendor_id: 0, product_id: 0 });
        }
        devices
    }

    fn speed_name(&self, speed: u8) -> &'static str {
        match speed { 1 => "Low 1.5M", 2 => "Full 12M", 3 => "High 480M", 4 => "Super 5G", 5 => "Super+ 10G", _ => "Unknown" }
    }
}
