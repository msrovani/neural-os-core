//! xHCI host — init + bulk + MSC + HID kb/mouse (ADR-0062 P11/P24).
//! Registradores: Cap @ BAR0; Op = Cap+CAPLENGTH; DBOFF/RTSOFF no Cap space.

use core::sync::atomic::Ordering;
use crate::memory::{PHYS_MEM_OFFSET, GLOBAL_ALLOCATOR};

mod bringup;
mod hub;
pub use bringup::{bringup_boot_msc, bringup_hid_keyboard, bringup_hid_mouse};
pub use hub::{
    hub_address_boot_smoke, hub_address_ok, hub_child_ok, hub_ok, hub_ports, mark_hub_address_device,
};

pub struct XhciDev {
    pub port: u8,
    pub slot: u8,
    pub speed: u8,
    pub is_keyboard: bool,
    pub last_report: [u8; 8],
}

fn mmio32(base: u64, off: u64) -> *mut u32 { (base as *mut u32).wrapping_add(off as usize / 4) }
pub(crate) unsafe fn r32(base: u64, off: u64) -> u32 { mmio32(base, off).read_volatile() }
pub(crate) unsafe fn w32(base: u64, off: u64, v: u32) { mmio32(base, off).write_volatile(v) }

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
    pub(crate) op: u64,
    pub(crate) capl: u64,
    pub(crate) base: u64,
    pub(crate) pmoff: u64,
    pub(crate) dcbaa_va: u64,
    pub(crate) er_va: u64,
    pub(crate) slot: u8,
    /// Byte offset from Cap base to Doorbell Array (DBOFF).
    pub(crate) db_off: u64,
    pub(crate) tr_va: u64,
    pub(crate) report_va: u64,
    pub(crate) last_report: [u8; 8],
    pub(crate) cmd_ring_pa: u64,
    pub(crate) cmd_ring_va: u64,
    pub(crate) cmd_enqueue: u16,
    pub(crate) cmd_cycle: bool,
    pub(crate) max_slots: u8,
    pub(crate) max_ports: u8,
    pub(crate) er_dequeue: u16,
    /// Porta reservada pelo MSC (0 = nenhuma) — HID não rouba o stick.
    pub(crate) msc_port: u8,
    /// HID boot keyboard ready
    pub(crate) hid_ready: bool,
    pub(crate) hid_slot: u8,
    pub(crate) hid_port: u8,
    pub(crate) hid_tr_va: u64,
    pub(crate) hid_report_va: u64,
    pub(crate) hid_last_usage: u8,
    /// ADR-0062 P24b: HID boot mouse
    pub(crate) mouse_ready: bool,
    pub(crate) mouse_slot: u8,
    pub(crate) mouse_port: u8,
    pub(crate) mouse_tr_va: u64,
    pub(crate) mouse_report_va: u64,
    pub(crate) mouse_last: [u8; 4],
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

        // Cap space (NÃO Operational): HCSPARAMS1, DBOFF, RTSOFF
        let hcs1 = r32(base, 0x04);
        let max_slots = (hcs1 & 0xFF) as u8;
        let max_ports = ((hcs1 >> 24) & 0xFF) as u8;
        let db_off = (r32(base, 0x14) & !0x3) as u64;
        let rtsoff = (r32(base, 0x18) & !0x1F) as u64;

        // Halt controller
        w32(op, 0, r32(op, 0) & !0x01);
        for _ in 0..100_000 {
            if r32(op, 0x04) & 0x01 != 0 { break; } // HCH
            core::hint::spin_loop();
        }

        // HCRST
        w32(op, 0, r32(op, 0) | 0x02);
        for _ in 0..100_000 {
            if r32(op, 0) & 0x02 == 0 { break; }
            core::hint::spin_loop();
        }

        let dcbaa = match alloc_phys(1) {
            Some(p) => p,
            None => { crate::slog_nano!("USB", "xhci", "alloc_phys falhou (DCBAA)"); continue; }
        };
        core::ptr::write_bytes(dcbaa.1, 0, 4096);
        // DCBAAP @ Op+0x30
        w32(op, 0x30, dcbaa.0 as u32);
        w32(op, 0x34, (dcbaa.0 >> 32) as u32);

        // Command ring @ Op+0x18 (CRCR)
        let cmd = match alloc_phys(1) {
            Some(p) => p,
            None => { crate::slog_nano!("USB", "xhci", "alloc_phys falhou (CRCR)"); continue; }
        };
        core::ptr::write_bytes(cmd.1, 0, 4096);
        w32(op, 0x18, cmd.0 as u32 | 0x1); // RCS=1
        w32(op, 0x1C, (cmd.0 >> 32) as u32);

        // Event ring + ERST @ Runtime (Cap+RTSOFF)
        let erst_mem = match alloc_phys(1) {
            Some(p) => p,
            None => { crate::slog_nano!("USB", "xhci", "alloc_phys falhou (ERST)"); continue; }
        };
        let er = match alloc_phys(1) {
            Some(p) => p,
            None => { crate::slog_nano!("USB", "xhci", "alloc_phys falhou (Event Ring)"); continue; }
        };
        core::ptr::write_bytes(erst_mem.1, 0, 4096);
        core::ptr::write_bytes(er.1, 0, 4096);
        // ERST entry: addr + size (TRBs)
        let erst = erst_mem.1 as *mut u64;
        erst.write_volatile(er.0);
        erst.add(1).write_volatile(256u64);
        let rt = base + rtsoff;
        w32(rt, 0x08, 1); // ERSTSZ (Interrupter 0)
        w32(rt, 0x10, erst_mem.0 as u32); // ERSTBA (Interrupter 0)
        w32(rt, 0x14, (erst_mem.0 >> 32) as u32);
        w32(rt, 0x18, er.0 as u32); // ERDP (Interrupter 0)
        w32(rt, 0x1C, (er.0 >> 32) as u32);

        // CONFIG MaxSlotsEn
        let slots = if max_slots == 0 { 8 } else { max_slots.min(64) };
        w32(op, 0x38, slots as u32);

        // Run
        w32(op, 0, r32(op, 0) | 0x01);
        for _ in 0..100_000 {
            if r32(op, 0x04) & 0x01 == 0 { break; }
            core::hint::spin_loop();
        }

        let tr = match alloc_phys(1) {
            Some(p) => p,
            None => { crate::slog_nano!("USB", "xhci", "alloc_phys falhou (TR)"); continue; }
        };
        core::ptr::write_bytes(tr.1, 0, 4096);
        let report = match alloc_phys(1) {
            Some(p) => p,
            None => { crate::slog_nano!("USB", "xhci", "alloc_phys falhou (report)"); continue; }
        };
        core::ptr::write_bytes(report.1, 0, 4096);

        *XHCI_STATE.lock() = Some(XhciState {
            op, capl, base, pmoff,
            dcbaa_va: dcbaa.0 + pmoff,
            er_va: er.0 + pmoff,
            slot: 1,
            db_off,
            tr_va: tr.0 + pmoff,
            report_va: report.0 + pmoff,
            last_report: [0; 8],
            cmd_ring_pa: cmd.0,
            cmd_ring_va: cmd.0 + pmoff,
            cmd_enqueue: 0,
            cmd_cycle: true,
            max_slots: slots,
            max_ports: if max_ports == 0 { 8 } else { max_ports },
            er_dequeue: 0,
            msc_port: 0,
            hid_ready: false,
            hid_slot: 0,
            hid_port: 0,
            hid_tr_va: 0,
            hid_report_va: 0,
            hid_last_usage: 0,
            mouse_ready: false,
            mouse_slot: 0,
            mouse_port: 0,
            mouse_tr_va: 0,
            mouse_report_va: 0,
            mouse_last: [0; 4],
        });
        crate::slog_nano!(
            "USB",
            "xhci",
            "Inicializado. slots={} ports={} db_off={:#x} rtsoff={:#x}",
            slots,
            max_ports,
            db_off,
            rtsoff
        );
        return;
    }
}

/// Poll do teclado USB HID boot — InputAgent. Requer `bringup_hid_keyboard` (P24a).
pub unsafe fn poll_keyboard() -> Option<u8> {
    let mut state_lock = XHCI_STATE.lock();
    let state = match &mut *state_lock {
        Some(s) if s.hid_ready => s,
        _ => return None,
    };

    let report = state.hid_report_va as *const u8;
    let usage = report.add(2).read_volatile();
    let mods = report.read_volatile();

    // CAD: LCtrl+LAlt+Delete
    if mods & 0x05 == 0x05 && usage == 0x4C {
        return Some(0x53);
    }

    if usage == 0 || usage == state.hid_last_usage {
        // Re-armar transfer interrupt (Normal TRB 8 bytes) se idle
        queue_hid_interrupt_read(state);
        return None;
    }
    state.hid_last_usage = usage;
    let sc = hid_to_scancode(usage)?;
    queue_hid_interrupt_read(state);
    Some(sc)
}

/// Poll HID boot mouse — injeta no path PS/2 canônico (`mouse_inject_hid_boot`).
pub unsafe fn poll_mouse() -> bool {
    let mut state_lock = XHCI_STATE.lock();
    let state = match &mut *state_lock {
        Some(s) if s.mouse_ready => s,
        _ => return false,
    };
    if state.mouse_report_va == 0 {
        return false;
    }
    let report = state.mouse_report_va as *const u8;
    let mut cur = [0u8; 4];
    for i in 0..4 {
        cur[i] = report.add(i).read_volatile();
    }
    if cur == state.mouse_last {
        queue_mouse_interrupt_read(state);
        return false;
    }
    state.mouse_last = cur;
    let buttons = cur[0];
    let dx = cur[1] as i8;
    let dy = cur[2] as i8;
    queue_mouse_interrupt_read(state);
    drop(state_lock);
    crate::interrupts::mouse_inject_hid_boot(buttons, dx, dy);
    true
}

pub(crate) unsafe fn queue_hid_interrupt_read(state: &mut XhciState) {
    if state.hid_tr_va == 0 || state.hid_report_va == 0 || state.hid_slot == 0 {
        return;
    }
    let trb = state.hid_tr_va as *mut u32;
    let report_pa = state.hid_report_va - state.pmoff;
    trb.add(0).write_volatile(report_pa as u32);
    trb.add(1).write_volatile((report_pa >> 32) as u32);
    trb.add(2).write_volatile(8); // 8-byte boot report
    trb.add(3).write_volatile((1u32 << 10) | (1 << 5) | 1); // Normal, IOC, C=1
    core::arch::asm!("sfence", options(nostack, preserves_flags));
    // DCI 3 = EP1 IN
    w32(state.base, state.db_off + (state.hid_slot as u64) * 4, 3);
}

pub(crate) unsafe fn queue_mouse_interrupt_read(state: &mut XhciState) {
    if state.mouse_tr_va == 0 || state.mouse_report_va == 0 || state.mouse_slot == 0 {
        return;
    }
    let trb = state.mouse_tr_va as *mut u32;
    let report_pa = state.mouse_report_va - state.pmoff;
    trb.add(0).write_volatile(report_pa as u32);
    trb.add(1).write_volatile((report_pa >> 32) as u32);
    trb.add(2).write_volatile(4); // 4-byte boot mouse
    trb.add(3).write_volatile((1u32 << 10) | (1 << 5) | 1);
    core::arch::asm!("sfence", options(nostack, preserves_flags));
    w32(state.base, state.db_off + (state.mouse_slot as u64) * 4, 3);
}

pub(crate) unsafe fn alloc_phys(n: usize) -> Option<(u64, *mut u8)> {
    
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

    crate::slog_nano!("USB", "xhci", "Bulk endpoints OK. slot={} tr_in={:#x} tr_out={:#x}", slot, tr_in.0, tr_out.0);

    Some((
        BulkEndpoint { trb_pa: tr_in.0, trb_va: tr_in.1 as *mut u32, enqueue_idx: 0, cycle: true, max_entries },
        BulkEndpoint { trb_pa: tr_out.0, trb_va: tr_out.1 as *mut u32, enqueue_idx: 0, cycle: true, max_entries },
    ))
}

/// Executa transferencia bulk com gerenciamento de ring + IOC + ERDP advance.
/// direction: 0=OUT (host→device), 1=IN (device→host)
pub unsafe fn bulk_transfer(
    slot: u8,
    _endpoint: u8,
    ep: &mut BulkEndpoint,
    data_pa: u64,
    len: u32,
    direction: u8,
) -> bool {
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

    // Ring doorbell[Slot] = DCI (xHCI 4.2.1)
    let dci = if direction == 0 { 2u32 } else { 3u32 }; // EP1 OUT=2, EP1 IN=3
    w32(st.base, st.db_off + (slot as u64) * 4, dci);

    // Wait for completion event (poll ER with timeout curto — HW real sem EP MSC).
    for _ in 0..80_000 {
        let evt = st.er_va as *const u32;
        let dw3 = evt.add(3).read_volatile();
        let trb_type = (dw3 >> 10) & 0x3F;
        if trb_type == 32 {
            let comp = ((evt.add(2).read_volatile() >> 24) & 0xFF) as u8;
            let erdp_phys = st.er_va - st.pmoff;
            // ERDP advance via Runtime space (Interrupter 0)
            let rtsoff = (r32(st.base, 0x18) & !0x1F) as u64;
            let rt = st.base + rtsoff;
            w32(rt, 0x18, erdp_phys as u32);
            w32(rt, 0x1C, (erdp_phys >> 32) as u32 | 0x01);
            if comp == 0 {
                return true;
            }
            crate::slog_nano!("USB", "xhci", "Bulk err: comp={}", comp);
            return false;
        }
        core::hint::spin_loop();
    }
    // Uma vez só — flood de timeout ilegível no FB.
    static TIMEOUT_LOGGED: core::sync::atomic::AtomicBool =
        core::sync::atomic::AtomicBool::new(false);
    if !TIMEOUT_LOGGED.swap(true, Ordering::Relaxed) {
        crate::slog_nano!("USB", "xhci", "Bulk timeout (demais omitidos)");
    }
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

/// PORTSC base = op + 0x400 + (port-1)*0x10 (xHCI 1.1).
pub(crate) unsafe fn portsc_addr(st: &XhciState, port: u8) -> Option<u64> {
    if port == 0 || port > st.max_ports {
        return None;
    }
    Some(st.op + 0x400 + ((port as u64 - 1) * 0x10))
}

/// Desabilita porta (limpa PED). Best-effort — W1C bits preservados.
pub unsafe fn disable_port(port: u8) -> bool {
    let state = XHCI_STATE.lock();
    let st = match state.as_ref() {
        Some(s) => s,
        None => return false,
    };
    let Some(addr) = portsc_addr(st, port) else {
        return false;
    };
    let off = addr - st.base;
    let mut v = r32(st.base, off);
    // Clear PED (bit 1); preserve CCS etc. Write-1-to-clear: mask carefully.
    v &= !0x2;
    // Clear change bits by writing 1s where needed (CSC=17, PEC=18, …)
    v |= (1 << 17) | (1 << 18);
    w32(st.base, off, v);
    true
}

/// IDEA #12 — desabilita portas com device conectado (CCS) em modo enforce Deny.
/// Nao distingue teclado vs MSC (EP0 limitado); conta portas tocadas.
pub unsafe fn disable_untrusted_ports() -> u8 {
    let state = XHCI_STATE.lock();
    let st = match state.as_ref() {
        Some(s) => s,
        None => return 0,
    };
    let hcs1 = r32(st.base, 0x04);
    let max_ports = ((hcs1 >> 24) & 0xFF) as u8;
    drop(state);
    let mut n = 0u8;
    for port in 1..=max_ports.max(1) {
        let state = XHCI_STATE.lock();
        let Some(st) = state.as_ref() else {
            break;
        };
        let Some(addr) = portsc_addr(st, port) else {
            continue;
        };
        let off = addr - st.base;
        let v = r32(st.base, off);
        let ccs = v & 1 != 0;
        let ped = v & 2 != 0;
        drop(state);
        if ccs && ped {
            if disable_port(port) {
                n = n.saturating_add(1);
                crate::slog_bin!("USB-TRUST", "info", "port {} PED cleared (CCS)", port);
            }
        }
    }
    n
}
