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
    pub interface: u8,
    pub ep_in: u8,
}

fn mmio32(base: u64, off: u64) -> *mut u32 { (base as *mut u32).wrapping_add(off as usize / 4) }
unsafe fn r32(base: u64, off: u64) -> u32 { mmio32(base, off).read_volatile() }
unsafe fn w32(base: u64, off: u64, v: u32) { mmio32(base, off).write_volatile(v) }

pub struct XhciDriver {
    base: u64,
    op: u64,
    pub ports: u8,
    pub slots: u8,
    dcbaa_va: u64,
    er_va: u64,
    er_used: u16,
    db_off: u64,
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
        let hcc1 = r32(base + capl, 8);
        let db_off = ((hcc1 >> 16) as u64 & !0x3) + base - pmoff;

        // Stop controller
        w32(op, 0, r32(op, 0) & !0x01);
        for _ in 0..1000 { if r32(op, 0) & 0x01 == 0 { break; } core::hint::spin_loop(); }

        // DCBAA
        let dcbaa_frames = alloc_phys(1)?;
        core::ptr::write_bytes(dcbaa_frames.1, 0, 4096);
        w32(op, 0x10, dcbaa_frames.0 as u32);
        w32(op, 0x14, (dcbaa_frames.0 >> 32) as u32);

        // Event Ring
        let er_frames = alloc_phys(2)?;
        core::ptr::write_bytes(er_frames.1, 0, 8192);
        w32(base + capl, 0x38, er_frames.0 as u32);
        w32(base + capl, 0x3C, (er_frames.0 >> 32) as u32);
        w32(base + capl, 0x30, 0);
        w32(base + capl, 0x34, er_frames.0 as u32 | 0x01);

        // Max slots + start
        w32(op, 0x38, slots as u32);
        w32(op, 0, r32(op, 0) | 0x01);
        for _ in 0..1000 { if r32(op, 0) & 0x01 != 0 { break; } core::hint::spin_loop(); }

        serial_println!("[XHCI] Init: {} portas, {} slots, db={:#x}", ports, slots, db_off);

        if ports == 0 { return None; }
        Some(XhciDriver {
            base, op, ports, slots,
            dcbaa_va: dcbaa_frames.0 + pmoff,
            er_va: er_frames.0 + pmoff,
            er_used: 0, db_off,
        })
    }

    pub unsafe fn port_scan(&self) -> Vec<XhciDev> {
        let mut found = Vec::new();
        for p in 0..self.ports.min(8) {
            let portsc = r32(self.op, 0x400 + p as u64 * 0x10);
            if portsc & 0x01 == 0 { continue; }
            let speed = ((portsc >> 20) & 0x0F) as u8;
            serial_println!("[XHCI] Porta {}: device connected speed={}", p, speed);
            found.push(XhciDev { port: p, slot: 0, speed, is_keyboard: true, interface: 0, ep_in: 0 });
        }
        found
    }

    pub unsafe fn init(&self) -> bool {
        serial_println!("[XHCI] Driver pronto: {} portas.", self.ports);
        true
    }

    pub unsafe fn poll(&mut self) -> Option<u8> {
        // Ler Event Ring para TRB de transferência
        let er_base = self.er_va as *mut u32;
        let idx = self.er_used as usize;
        let trb = er_base.add(idx * 4);
        let status = trb.add(2).read_volatile();
        if status & 0x01 == 0 { return None; } // não completado

        let len = trb.add(2).read_volatile() >> 24;
        if len >= 8 {
            let report_buf = (self.er_va + 0x1000) as *const u8;
            for i in 0..8 { let _ = report_buf.add(i).read_volatile(); }
        }
        self.er_used = (self.er_used + 1) & 0xFF;
        trb.add(2).write_volatile(0);
        // Atualizar dequeue pointer
        let erdp = self.er_va + 16 + self.er_used as u64 * 16;
        w32(self.op, 0x18, erdp as u32);
        w32(self.op, 0x1C, (erdp >> 32) as u32);
        None
    }
}

unsafe fn alloc_phys(n: usize) -> Option<(u64, *mut u8)> {
    use x86_64::structures::paging::FrameAllocator;
    let mut g = GLOBAL_ALLOCATOR.lock();
    let a = (*g).as_mut()?;
    let frame = a.allocate_contiguous(n)?;
    let pa = frame.start_address().as_u64();
    Some((pa, (pa + PHYS_MEM_OFFSET.load(Ordering::Relaxed)) as *mut u8))
}
