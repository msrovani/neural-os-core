use alloc::vec::Vec;
use core::sync::atomic::Ordering;
use crate::pci::PciDevice;
use crate::memory::{PHYS_MEM_OFFSET, GLOBAL_ALLOCATOR};
use crate::serial_println;

pub struct XhciDev {
    pub port: u8,
    pub slot: u8,
    pub speed: u8,
    pub is_keyboard: bool,
}

fn mmio32(base: u64, off: u64) -> *mut u32 { (base as *mut u32).wrapping_add(off as usize / 4) }
unsafe fn r32(base: u64, off: u64) -> u32 { mmio32(base, off).read_volatile() }
unsafe fn w32(base: u64, off: u64, v: u32) { mmio32(base, off).write_volatile(v) }

pub struct XhciDriver {
    base: u64,
    op: u64,
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
        let capl = r32(base, 0) as u64 & 0xFF;
        let op = base + capl;
        let hcs1 = r32(op, 4);
        let ports = (hcs1 & 0xFF) as u8;
        let slots = ((hcs1 >> 8) & 0xFF) as u8;

        w32(op, 0, r32(op, 0) & !0x01);
        for _ in 0..1000 { if r32(op, 0) & 0x01 == 0 { break; } core::hint::spin_loop(); }
        let dcbaa = alloc_phys(1)?;
        core::ptr::write_bytes(dcbaa.1, 0, 4096);
        w32(op, 0x10, dcbaa.0 as u32); w32(op, 0x14, (dcbaa.0 >> 32) as u32);
        let er = alloc_phys(2)?;
        core::ptr::write_bytes(er.1, 0, 8192);
        w32(base + capl, 0x38, er.0 as u32); w32(base + capl, 0x3C, (er.0 >> 32) as u32);
        w32(base + capl, 0x30, 0); w32(base + capl, 0x34, er.0 as u32 | 0x01);
        w32(op, 0x38, slots as u32);
        w32(op, 0, r32(op, 0) | 0x01);
        for _ in 0..1000 { if r32(op, 0) & 0x01 != 0 { break; } core::hint::spin_loop(); }
        serial_println!("[XHCI] {} portas, {} slots", ports, slots);
        if ports == 0 { return None; }
        Some(XhciDriver { base, op, ports, slots })
    }

    pub unsafe fn init(&self) -> bool { true }

    pub unsafe fn port_scan(&self) -> Vec<XhciDev> {
        let mut found = Vec::new();
        for p in 0..self.ports.min(8) {
            let portsc = r32(self.op, 0x400 + p as u64 * 0x10);
            if portsc & 0x01 == 0 { continue; }
            let speed = ((portsc >> 20) & 0x0F) as u8;
            serial_println!("[XHCI] Porta {}: device speed={}", p, speed);
            found.push(XhciDev { port: p, slot: 0, speed, is_keyboard: true });
        }
        found
    }
}

unsafe fn alloc_phys(n: usize) -> Option<(u64, *mut u8)> {
    use x86_64::structures::paging::FrameAllocator;
    let mut g = GLOBAL_ALLOCATOR.lock();
    let a = (*g).as_mut()?;
    let f = a.allocate_contiguous(n)?;
    let pa = f.start_address().as_u64();
    Some((pa, (pa + PHYS_MEM_OFFSET.load(Ordering::Relaxed)) as *mut u8))
}
