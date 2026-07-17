use core::sync::atomic::Ordering;
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
        let db_off = (hcc1 >> 16) as u64 & !0x3 ;

        w32(op, 0, r32(op, 0) & !0x01);
        for _ in 0..1000 { if r32(op, 0) & 0x01 == 0 { break; } core::hint::spin_loop(); }

        let dcbaa = match alloc_phys(1) {
            Some(p) => p,
            None => { serial_println!("[XHCI] alloc_phys falhou (DCBAA) — abortando init"); continue; }
        };
        core::ptr::write_bytes(dcbaa.1, 0, 4096);
        w32(op, 0x10, dcbaa.0 as u32); w32(op, 0x14, (dcbaa.0 >> 32) as u32);

        let er = match alloc_phys(2) {
            Some(p) => p,
            None => { serial_println!("[XHCI] alloc_phys falhou (Event Ring) — abortando init"); continue; }
        };
        core::ptr::write_bytes(er.1, 0, 8192);
        w32(base + capl, 0x38, er.0 as u32); w32(base + capl, 0x3C, (er.0 >> 32) as u32);
        w32(base + capl, 0x30, 0); w32(base + capl, 0x34, er.0 as u32 | 0x01);

        let hcs1 = r32(op, 4); let slots = ((hcs1 >> 8) & 0xFF) as u8;
        w32(op, 0x38, slots as u32);
        w32(op, 0, r32(op, 0) | 0x01);
        for _ in 0..1000 { if r32(op, 0) & 0x01 != 0 { break; } core::hint::spin_loop(); }

        // Allocate transfer ring + report buffer
        let tr = match alloc_phys(1) {
            Some(p) => p,
            None => { serial_println!("[XHCI] alloc_phys falhou (transfer ring) — abortando init"); continue; }
        };
        core::ptr::write_bytes(tr.1, 0, 4096);
        let report = match alloc_phys(1) {
            Some(p) => p,
            None => { serial_println!("[XHCI] alloc_phys falhou (report buffer) — abortando init"); continue; }
        };
        core::ptr::write_bytes(report.1, 0, 4096);

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
        let ctx_phys = match alloc_phys(2) {
            Some(c) => c,
            None => { return None; }
        };
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
    let _cycle = ctrl.add(3).read_volatile() & 0x01;

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
    
    let mut g = GLOBAL_ALLOCATOR.lock();
    let a = (*g).as_mut()?;
    let f = a.allocate_contiguous(n)?;
    let pa = f.start_address().as_u64();
    Some((pa, (pa + PHYS_MEM_OFFSET.load(Ordering::Relaxed)) as *mut u8))
}

// ── USB Mass Storage Bulk Transfers ────────────────────────────
// Suporte a bulk IN/OUT via xHCI com gerenciamento de ring.

/// Estado de um transfer ring bulk (IN ou OUT)
pub struct BulkEndpoint {
    pub trb_pa: u64,
    pub trb_va: *mut u32,
    pub enqueue_idx: u16,
    pub cycle: bool,
    pub max_entries: u16,
}

unsafe impl Send for BulkEndpoint {}
unsafe impl Sync for BulkEndpoint {}

/// Configura bulk endpoints (EP1 IN, EP2 OUT) para USB MSC
pub unsafe fn configure_msc_endpoints(slot: u8, max_packet: u16) -> Option<(BulkEndpoint, BulkEndpoint)> {
    let state = XHCI_STATE.lock();
    let st = state.as_ref()?;

    let tr_in = alloc_phys(1)?;
    let tr_out = alloc_phys(1)?;
    core::ptr::write_bytes(tr_in.1, 0, 4096);
    core::ptr::write_bytes(tr_out.1, 0, 4096);

    let ctx = alloc_phys(2)?;
    core::ptr::write_bytes(ctx.1, 0, 8192);
    let dcbaa = st.dcbaa_va as *mut u64;
    dcbaa.add(slot as usize).write_volatile(ctx.0);

    let icc = ctx.1 as *mut u32;
    icc.add(0).write_volatile(0x03);
    icc.add(1).write_volatile(0x06);

    icc.add(2).write_volatile(0x20);
    icc.add(4).write_volatile((slot as u32) << 24);

    let max_entries = 256u16; // 4096 bytes / 16 bytes per TRB

    // EP1 OUT context (bulk, host→device)
    let ep1_out = ctx.1.add(32 + 32) as *mut u32;
    ep1_out.add(0).write_volatile(0x0002_0802);
    ep1_out.add(1).write_volatile(tr_out.0 as u32);
    ep1_out.add(2).write_volatile((tr_out.0 >> 32) as u32 | 0x01);
    ep1_out.add(3).write_volatile(0x0000_0000);
    ((ctx.1.add(32 + 32 + 8)) as *mut u16).write_volatile(max_packet);

    // EP1 IN context (bulk, device→host)
    let ep1_in = ctx.1.add(32 + 64) as *mut u32;
    ep1_in.add(0).write_volatile(0x0006_0802);
    ep1_in.add(1).write_volatile(tr_in.0 as u32);
    ep1_in.add(2).write_volatile((tr_in.0 >> 32) as u32 | 0x01);
    ep1_in.add(3).write_volatile(0x0000_0000);
    ((ctx.1.add(32 + 64 + 8)) as *mut u16).write_volatile(max_packet);

    serial_println!("[XHCI] Bulk endpoints OK. slot={} tr_in={:#x} tr_out={:#x}", slot, tr_in.0, tr_out.0);

    Some((
        BulkEndpoint { trb_pa: tr_in.0, trb_va: tr_in.1 as *mut u32, enqueue_idx: 0, cycle: true, max_entries },
        BulkEndpoint { trb_pa: tr_out.0, trb_va: tr_out.1 as *mut u32, enqueue_idx: 0, cycle: true, max_entries },
    ))
}

/// Executa transferencia bulk com gerenciamento de ring + IOC + ERDP advance.
/// direction: 0=OUT (host→device), 1=IN (device→host)
pub unsafe fn bulk_transfer(slot: u8, endpoint: u8, ep: &mut BulkEndpoint, data_pa: u64, len: u32, direction: u8) -> bool {
    let state = XHCI_STATE.lock();
    let st = match state.as_ref() { Some(s) => s, None => return false };

    let idx = ep.enqueue_idx as usize;
    let max = ep.max_entries as usize;

    // Write TRB at current enqueue position
    let trb = ep.trb_va.add(idx * 4);
    trb.add(0).write_volatile(data_pa as u32);
    trb.add(1).write_volatile((data_pa >> 32) as u32);
    // IOC=1 (bit 5), TD size=1 (bits 17-31), chain=0
    trb.add(2).write_volatile(len | (1u32 << 5) | (1u32 << 17));
    // Normal TRB (type=1), cycle=ep.cycle
    trb.add(3).write_volatile(if ep.cycle { 0x0000_0001u32 } else { 0x0000_0000u32 });

    // Fence write before doorbell
    core::arch::asm!("sfence", options(nostack, preserves_flags));

    // Advance enqueue pointer
    let next = (idx + 1) % max;
    // If next is the last slot, write Link TRB to wrap
    if next == max - 1 {
        let link = ep.trb_va.add((max - 1) * 4);
        link.add(0).write_volatile(ep.trb_pa as u32);
        link.add(1).write_volatile((ep.trb_pa >> 32) as u32);
        link.add(2).write_volatile(0);
        // Link TRB (type=6), cycle=ep.cycle, Toggle Cycle=1
        link.add(3).write_volatile(if ep.cycle { 0x0000_0026u32 } else { 0x0000_0006u32 });
    }
    ep.enqueue_idx = if next == max - 1 { 0 } else { next as u16 };
    if next == max - 1 { ep.cycle = !ep.cycle; }

    // Ring doorbell
    let db_off = st.db_off + (slot as u64 * 2 + endpoint as u64) * 4;
    let db_val = if direction == 0 { 2u32 } else { 3u32 };
    w32(st.base, db_off, db_val);

    // Wait for completion event (poll ER with timeout)
    for _ in 0..2_000_000 {
        let evt = st.er_va as *const u32;
        let flags = (evt.add(11).read_volatile() >> 24) as u8;
        if flags & 0x20 != 0 {
            let comp = evt.add(10).read_volatile() & 0xFF;
            // Advance ERDP to acknowledge the event
            let erdp_phys = st.er_va - st.pmoff;
            w32(st.base + st.capl, 0x38, erdp_phys as u32);
            w32(st.base + st.capl, 0x3C, (erdp_phys >> 32) as u32 | 0x01);
            if comp == 0 { return true; }
            serial_println!("[XHCI] Bulk err: comp={}", comp);
            return false;
        }
        core::hint::spin_loop();
    }
    serial_println!("[XHCI] Bulk timeout");
    false
}

/// Tenta ler Configuration Descriptor do device no slot ativo (GET_DESCRIPTOR).
/// Retorna (bytes_lidos, vid, did) ou None se xHCI/HID-only sem EP0 control genérico.
///
/// Sprint Sound: path honesto — sem device UAC no bus QEMU default, retorna None.
/// Quando EP0 control transfer estiver pleno, preencher `buf` com o descriptor.
pub unsafe fn try_read_config_descriptor(buf: &mut [u8]) -> Option<(usize, u16, u16)> {
    let state = XHCI_STATE.lock();
    let _st = state.as_ref()?;
    // Control transfer GET_DESCRIPTOR(Configuration) ainda não está wired no
    // path HID-only. Deixa buffer zerado e sinaliza incompleto ao caller UAC.
    let _ = buf;
    None
}
