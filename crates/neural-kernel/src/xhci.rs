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
    pub last_report: [u8; 8],
}

fn mmio32(base: u64, off: u64) -> *mut u32 { (base as *mut u32).wrapping_add(off as usize / 4) }
unsafe fn r32(base: u64, off: u64) -> u32 { mmio32(base, off).read_volatile() }
unsafe fn w32(base: u64, off: u64, v: u32) { mmio32(base, off).write_volatile(v) }

/// USB HID Usage → PS/2 scancode (set 1, make)
fn hid_to_scancode(usage: u8) -> Option<u8> {
    match usage {
        0x04 => Some(0x1E), 0x05 => Some(0x30), 0x06 => Some(0x2E), 0x07 => Some(0x20), // A B C D
        0x08 => Some(0x12), 0x09 => Some(0x21), 0x0A => Some(0x22), 0x0B => Some(0x23), // E F G H
        0x0C => Some(0x17), 0x0D => Some(0x24), 0x0E => Some(0x25), 0x0F => Some(0x26), // I J K L
        0x10 => Some(0x32), 0x11 => Some(0x31), 0x12 => Some(0x18), 0x13 => Some(0x19), // M N O P
        0x14 => Some(0x10), 0x15 => Some(0x13), 0x16 => Some(0x1F), 0x17 => Some(0x14), // Q R S T
        0x18 => Some(0x16), 0x19 => Some(0x2F), 0x1A => Some(0x11), 0x1B => Some(0x2D), // U V W X
        0x1C => Some(0x15), 0x1D => Some(0x2C),                                         // Y Z
        0x1E => Some(0x02), 0x1F => Some(0x03), 0x20 => Some(0x04), 0x21 => Some(0x05), // 1 2 3 4
        0x22 => Some(0x06), 0x23 => Some(0x07), 0x24 => Some(0x08), 0x25 => Some(0x09), // 5 6 7 8
        0x26 => Some(0x0A), 0x27 => Some(0x0B),                                         // 9 0
        0x28 => Some(0x1C), 0x29 => Some(0x01),                                         // ENTER ESC
        0x2A => Some(0x0E), 0x2B => Some(0x0F),                                         // BACKSP TAB
        0x2C => Some(0x39), 0x2D => Some(0x0C), 0x2E => Some(0x0D),                     // SPACE - =
        0x2F => Some(0x1A), 0x30 => Some(0x1B),                                         // [ ]
        0x31 => Some(0x2B),                                                             // \
        0x33 => Some(0x27), 0x34 => Some(0x28), 0x35 => Some(0x29),                     // ; ' `
        0x36 => Some(0x33), 0x37 => Some(0x34), 0x38 => Some(0x35),                     // , . /
        0x4C => Some(0x53),                                                             // DELETE
        _ => None,
    }
}

/// Global xHCI driver state — inicializado uma vez no boot
pub static XHCI_STATE: spin::Mutex<Option<XhciState>> = spin::Mutex::new(None);

pub struct XhciState {
    op: u64, capl: u64, base: u64, pmoff: u64,
    dcbaa_va: u64, er_va: u64,
    slot: u8, db_off: u64,
    tr_va: u64, report_va: u64,
    last_report: [u8; 8],
}

pub unsafe fn init_xhci() {
    let devs = crate::pci::scan_pci();
    for d in &devs {
        if d.class != 0x0C || d.subclass != 0x03 { continue; }
        let pmoff = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        let mmio = (d.bar0 & !0xF) as u64;
        crate::apic::set_page_uc(mmio, pmoff);
        let base = mmio + pmoff;
        let capl = r32(base, 0) as u64 & 0xFF;
        let op = base + capl;
        let hcc1 = r32(base + capl, 8);
        let db_off = ((hcc1 >> 16) as u64 & !0x3);

        w32(op, 0, r32(op, 0) & !0x01);
        for _ in 0..1000 { if r32(op, 0) & 0x01 == 0 { break; } core::hint::spin_loop(); }

        let dcbaa = alloc_phys(1).unwrap(); core::ptr::write_bytes(dcbaa.1, 0, 4096);
        w32(op, 0x10, dcbaa.0 as u32); w32(op, 0x14, (dcbaa.0 >> 32) as u32);

        let er = alloc_phys(2).unwrap(); core::ptr::write_bytes(er.1, 0, 8192);
        w32(base + capl, 0x38, er.0 as u32); w32(base + capl, 0x3C, (er.0 >> 32) as u32);
        w32(base + capl, 0x30, 0); w32(base + capl, 0x34, er.0 as u32 | 0x01);

        let hcs1 = r32(op, 4); let slots = ((hcs1 >> 8) & 0xFF) as u8;
        w32(op, 0x38, slots as u32);
        w32(op, 0, r32(op, 0) | 0x01);
        for _ in 0..1000 { if r32(op, 0) & 0x01 != 0 { break; } core::hint::spin_loop(); }

        // Allocate transfer ring + report buffer
        let tr = alloc_phys(1).unwrap(); core::ptr::write_bytes(tr.1, 0, 4096);
        let report = alloc_phys(1).unwrap(); core::ptr::write_bytes(report.1, 0, 4096);

        *XHCI_STATE.lock() = Some(XhciState {
            op, capl, base, pmoff,
            dcbaa_va: dcbaa.0 + pmoff, er_va: er.0 + pmoff,
            slot: 1, db_off, tr_va: tr.0 + pmoff, report_va: report.0 + pmoff,
            last_report: [0; 8],
        });
        serial_println!("[XHCI] Inicializado. db_off={:#x}", db_off);
        return;
    }
}

/// Poll do teclado USB — chamado pelo InputAgent a cada 5 ticks.
/// Retorna scancode PS/2 (make) ou None.
pub unsafe fn poll_keyboard() -> Option<u8> {
    let mut state_lock = XHCI_STATE.lock();
    let state = match &mut *state_lock { Some(s) => s, None => return None };

    // Se primeiro poll, configura HID boot
    if state.last_report[0] == 0 && state.slot > 0 {
        // Setup device context slot
        let ctx_phys = alloc_phys(2).unwrap();
        core::ptr::write_bytes(ctx_phys.1, 0, 8192);
        let dcbaa = state.dcbaa_va as *mut u64;
        dcbaa.add(state.slot as usize).write_volatile(ctx_phys.0);

        // Input Control Context + Slot Context
        let icc = ctx_phys.1 as *mut u32;
        icc.add(0).write_volatile(0x03);
        icc.add(2).write_volatile(0x10); // slot.context_entries=1
        icc.add(4).write_volatile((state.slot as u32) << 24); // route string
        icc.add(5).write_volatile(0x0000_0000);
        // EP0 context (control endpoint)
        let ep0 = ctx_phys.1.add(32 + 32) as *mut u32;
        ep0.add(0).write_volatile(0x0000_0808);
        ep0.add(1).write_volatile(0x0000_0000);
        ep0.add(2).write_volatile(0x0000_0000);
        ep0.add(3).write_volatile(0x0000_0000);

        // Set device context pointer in DCBAA + ring doorbell 0
        // (simplified: assumes xHC accepts default slot context)

        state.last_report[0] = 0xFF; // mark as configured
        serial_println!("[USB] HID boot configurado.");
    }

    // Ler Event Ring para completions
    let evt = state.er_va as *const u64;
    let ctrl = state.er_va as *const u32;
    let cycle = ctrl.add(3).read_volatile() & 0x01;

    for i in 0..8u16 {
        let trb = evt.add(i as usize * 4);
        let flags = (trb.add(2).read_volatile() >> 24) as u8;
        if flags & 0x01 == 0 { continue; } // not completed
        if flags & 0x20 != 0 {
            // Transfer event
            let _len = (trb.add(2).read_volatile() >> 24) & 0xFFFFFF;
            // Ler HID report do buffer
            let report = state.report_va as *const u8;
            let mods = report.read_volatile();    // byte 0: modifiers
            let usage = report.add(2).read_volatile(); // byte 2: first key

            // Detect CAD: LCtrl(bit0) + LAlt(bit2) + Delete(0x4C)
            if mods & 0x05 == 0x05 && usage == 0x4C {
                return Some(0x53); // scancode DEL (make)
            }

            // Converter HID usage para scancode
            if let Some(sc) = hid_to_scancode(usage) {
                if usage != state.last_report[2] {
                    state.last_report[2] = usage;
                    return Some(sc);
                }
            }
        }
        break;
    }
    None
}

unsafe fn alloc_phys(n: usize) -> Option<(u64, *mut u8)> {
    use x86_64::structures::paging::FrameAllocator;
    let mut g = GLOBAL_ALLOCATOR.lock();
    let a = (*g).as_mut()?;
    let f = a.allocate_contiguous(n)?;
    let pa = f.start_address().as_u64();
    Some((pa, (pa + PHYS_MEM_OFFSET.load(Ordering::Relaxed)) as *mut u8))
}
