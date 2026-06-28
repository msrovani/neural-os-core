use alloc::vec::Vec;
use core::sync::atomic::Ordering;
use crate::pci::PciDevice;
use crate::memory::{PHYS_MEM_OFFSET, GLOBAL_ALLOCATOR};
use crate::serial_println;

#[derive(Debug)]
pub struct XhciDev {
    pub port: u8,
    pub slot: u8,
    pub speed: u8,
    pub is_keyboard: bool,
    pub last_report: [u8; 8],
}

fn mmio32(base: u64, off: u64) -> *mut u32 { (base as *mut u32).wrapping_add(off as usize / 4) }
unsafe fn r32(base: u64, off: u64) -> u32 { mmio32(base, off).read_volatile() }
unsafe fn w32(base: u64, off: u64, v: u32) { mmio32(base, off).write_volatile(v) }

pub struct XhciDriver {
    base: u64,
    capl: u64,
    pub ports: u8,
    pub slots: u8,
}

impl XhciDriver {
    pub unsafe fn new(dev: &PciDevice) -> Option<Self> {
        if dev.class != 0x0C || dev.subclass != 0x03 { return None; }
        let pmoff = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        let mmio_base = (dev.bar0 & !0xF) as u64;
        crate::apic::set_page_uc(mmio_base, pmoff);
        let base = mmio_base + pmoff;
        let cap = r32(base, 0) as u64 & 0xFF;
        let hcs1 = r32(base + cap, 4);
        let ports = (hcs1 & 0xFF) as u8;
        let slots = ((hcs1 >> 8) & 0xFF) as u8;
        serial_println!("[XHCI] Init: {} portas, {} slots, base={:#x}", ports, slots, base);
        Some(XhciDriver { base, capl: cap, ports, slots })
    }

    pub unsafe fn init(&self) -> bool {
        let usbcmd = r32(self.base, 0x00);
        w32(self.base, 0x00, usbcmd & !0x01);
        for _ in 0..1000 { if r32(self.base, 0x00) & 0x01 == 0 { break; } core::hint::spin_loop(); }
        w32(self.base, 0x38, self.slots as u32);
        w32(self.base, 0x00, usbcmd | 0x01);
        for _ in 0..1000 { if r32(self.base, 0x00) & 0x01 != 0 { break; } core::hint::spin_loop(); }
        true
    }

    pub unsafe fn port_scan(&self) -> Vec<XhciDev> {
        let mut found = Vec::new();
        for p in 0..self.ports.min(8) {
            let portsc = r32(self.base, 0x400 + p as u64 * 0x10);
            if portsc & 0x01 == 0 { continue; }
            let speed = ((portsc >> 20) & 0x0F) as u8;
            serial_println!("[XHCI] Porta {}: device connected, speed={}", p, speed);
            found.push(XhciDev { port: p, slot: 0, speed, is_keyboard: true, last_report: [0; 8] });
        }
        found
    }

    pub unsafe fn poll_keyboard(&self) -> Option<u8> {
        None
    }
}
