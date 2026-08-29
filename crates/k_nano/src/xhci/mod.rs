//! xHCI host — init + bulk + MSC + HID kb/mouse (ADR-0062 P11/P24).
//! Registradores: Cap @ BAR0; Op = Cap+CAPLENGTH; DBOFF/RTSOFF no Cap space.

use core::sync::atomic::Ordering;
use crate::memory::{PHYS_MEM_OFFSET, GLOBAL_ALLOCATOR};
use crate::dma::DmaBuf;

mod bringup;
mod hub;
pub use bringup::{
    bringup_boot_msc, bringup_hid_keyboard, bringup_hid_mouse, bringup_uac, bringup_uvc,
    try_deferred_hid_bringup,
};
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

// ── Isochronous (USB Audio) ────────────────────────────────────────────────
// ADR-0045 UAC: TRBs isócronos (Type 5, NÃO 8 — o work order dizia 8; o layout
// correto foi validado contra Linux xhci.h/xhci-ring.c):
//   DW2: [16:0] TRB length (OUT) | [21:17] TD size (0) | [31:22] interrupter (0)
//   DW3: [0] cycle | [4] chain (0) | [5] IOC | [9:7] TBC (0) | [15:10] type (5)
//        | [19:16] TLBPC (0) | [30:20] frame ID (0) | [31] SIA (1 = ASAP)
// Um TRB por intervalo de serviço; ring mantido cheio (re-arm por evento).

pub const ISOC_SLOTS: usize = 64;      // data TRBs por ring (Link no índice 64)
pub const ISOC_BUF_SIZE: usize = 1024; // buffer por slot (cobre 48k stereo/ms e mais)

/// Ring de TRBs isócronos (IN ou OUT) + pool de buffers por slot.
pub struct IsochRing {
    /// Página do ring (256 TRBs; usamos 0..ISOC_SLOTS + Link em ISOC_SLOTS).
    pub trb: DmaBuf,
    /// ISOC_SLOTS × ISOC_BUF_SIZE contíguos, UC (DMA).
    pub bufs: DmaBuf,
    /// Próxima posição do anel a escrever (0..=ISOC_SLOTS; ==ISOC_SLOTS = no Link).
    pub enqueue: u16,
    /// Cycle bit do produtor (software).
    pub cycle: bool,
    pub max_packet: u16,
    /// xHCI slot id.
    pub slot: u8,
    /// Doorbell DCI (IN = 2n+1, OUT = 2n).
    pub dci: u8,
    /// TRBs armados (não consumidos pelo controller).
    pub armed: u16,
    /// FIFO de pacotes pendentes: (buffer_idx, bytes_recebidos) — usado pelo
    /// poll_isoc_frame (UVC) para devolver UM pacote por chamada sem re-drenar.
    pub freed: [(u16, u16); ISOC_SLOTS],
    pub freed_head: u16,
    pub freed_tail: u16,
    /// Sobra de PCM parcial de chamadas anteriores de schedule_isoc_out.
    pub pending: [i16; 1024],
    pub pending_len: u16,
}

unsafe impl Send for IsochRing {}
unsafe impl Sync for IsochRing {}

/// Informação de device UAC enumerado (para jarbas preencher seus atomics).
pub struct UacDevice {
    pub slot: u8,
    pub port: u8,
    pub speed: u8,
    pub vid: u16,
    pub did: u16,
    pub capture_ep: u8,
    pub playback_ep: u8,
    pub sample_rate: u16,
    pub max_packet: u16,
}

/// Rings isócronos (separados de XhciState p/ evitar borrow conflitante com o
/// estado global — mesmo padrão de hub.rs / BulkEndpoint).
pub static ISOC_IN: spin::Mutex<Option<IsochRing>> = spin::Mutex::new(None);
pub static ISOC_OUT: spin::Mutex<Option<IsochRing>> = spin::Mutex::new(None);
/// Ring isócrono IN do UVC (câmera) — device class 0x0E, mesmo mecanismo.
pub static ISOC_UVC: spin::Mutex<Option<IsochRing>> = spin::Mutex::new(None);

/// Informação de device UVC enumerado (para jarbas preencher UVC_*).
pub struct UvcDevice {
    pub slot: u8,
    pub port: u8,
    pub vid: u16,
    pub did: u16,
    pub ep: u8,
    pub width: u16,
    pub height: u16,
    pub fps: u16,
    /// 1 = MJPEG, 0 = YUY2/raw.
    pub format: u8,
    pub max_packet: u16,
}

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
    /// UAC (USB Audio Class) — device isócrono enumerado (ADR-0045).
    pub(crate) uac_ready: bool,
    pub(crate) uac_slot: u8,
    pub(crate) uac_port: u8,
    pub(crate) uac_speed: u8,
    pub(crate) uac_vid: u16,
    pub(crate) uac_did: u16,
    pub(crate) uac_capture_ep: u8,
    pub(crate) uac_playback_ep: u8,
    pub(crate) uac_sample_rate: u16,
    /// Blob do Configuration Descriptor do device UAC (p/ try_read_config_descriptor).
    pub(crate) uac_cfg: [u8; 512],
    pub(crate) uac_cfg_len: usize,
    /// UVC (USB Video Class) — câmera isócrona (Phase 4).
    pub(crate) uvc_ready: bool,
    pub(crate) uvc_slot: u8,
    pub(crate) uvc_port: u8,
    pub(crate) uvc_vid: u16,
    pub(crate) uvc_did: u16,
    pub(crate) uvc_ep: u8,
    pub(crate) uvc_width: u16,
    pub(crate) uvc_height: u16,
    pub(crate) uvc_fps: u16,
    /// 1 = MJPEG, 0 = YUY2/raw.
    pub(crate) uvc_format: u8,
    pub(crate) uvc_max_packet: u16,
    /// EP0 ring do device UAC tentado (p/ UVC reusar o slot — webcam c/ mic).
    pub(crate) uac_ep0_tr_va: u64,
}

pub unsafe fn init_xhci() {
    if XHCI_STATE.lock().is_some() {
        return; // already up (early BOOT.LOG path)
    }
    let devs = crate::pci::scan_pci();
    for d in &devs {
        if d.class != 0x0C || d.subclass != 0x03 { continue; }
        let pmoff = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        let mmio = (d.bar0 & !0xF) as u64;
        // xHCI BAR cobre Cap+Op+Runtime+Doorbell (~64KB). Mapeia TODAS as páginas
        // UC com map_page_uc (cria o mapeamento). set_page_uc só seta flags em
        // mapeamento EXISTENTE — sem map, o 1º r32() dá #PF (exposto sob TCG;
        // WHPX mascarava. Ver SESSION_237).
        for page in 0..16 {
            crate::apic::map_page_uc(mmio + page * 0x1000, pmoff);
        }
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
            uac_ready: false,
            uac_slot: 0,
            uac_port: 0,
            uac_speed: 0,
            uac_vid: 0,
            uac_did: 0,
            uac_capture_ep: 0,
            uac_playback_ep: 0,
            uac_sample_rate: 0,
            uac_cfg: [0; 512],
            uac_cfg_len: 0,
            uvc_ready: false,
            uvc_slot: 0,
            uvc_port: 0,
            uvc_vid: 0,
            uvc_did: 0,
            uvc_ep: 0,
            uvc_width: 0,
            uvc_height: 0,
            uvc_fps: 0,
            uvc_format: 1,
            uvc_max_packet: 0,
            uac_ep0_tr_va: 0,
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

// ── Isochronous TRB scheduling (USB Audio) ─────────────────────────────
// xHCI 1.2 §4.14-4.15, §6.4.6. Ring sempre cheio: o controller consome 1 TRB
// por intervalo de serviço e gera 1 Transfer Event (IOC). O poll drena eventos,
// copia dados e re-arma os slots — os TRBs ficam na memória ≥1 frame antes do
// serviço (SIA=1 = ASAP + ring cheio satisfaz o requisito de "queue ahead").

/// Cria um ring isócrono: página de TRBs (Link no índice ISOC_SLOTS) + buffers UC.
pub(crate) unsafe fn new_isoc_ring(slot: u8, dci: u8, max_packet: u16) -> Option<IsochRing> {
    let trb = crate::dma::dma_alloc(4096)?;
    let bufs = crate::dma::dma_alloc(ISOC_SLOTS * ISOC_BUF_SIZE)?;
    let ring = IsochRing {
        trb,
        bufs,
        enqueue: 0,
        cycle: true,
        max_packet,
        slot,
        dci,
        armed: 0,
        freed: [(0, 0); ISOC_SLOTS],
        freed_head: 0,
        freed_tail: 0,
        pending: [0; 1024],
        pending_len: 0,
    };
    // Link TRB no fim dos data TRBs: aponta p/ base, Toggle Cycle, C=1 (1ª passada).
    let l = (ring.trb.virt as *mut u32).add(ISOC_SLOTS * 4);
    l.add(0).write_volatile(ring.trb.phys as u32);
    l.add(1).write_volatile((ring.trb.phys >> 32) as u32);
    l.add(2).write_volatile(0);
    l.add(3).write_volatile((6u32 << 10) | (1 << 1) | 1);
    Some(ring)
}

/// Escreve um TRB isócrono no ring (avança enqueue, atualiza Link no wrap).
/// `buf_idx` = slot do pool de buffers; `len` = bytes (OUT) / max_packet (IN).
pub(crate) unsafe fn isoc_arm_trb(ring: &mut IsochRing, buf_idx: usize, len: u16) {
    if ring.enqueue as usize >= ISOC_SLOTS {
        // Wrap do produtor: entrega o Link ao HW com o novo cycle.
        let new_cycle = !ring.cycle;
        let link = (ring.trb.virt as *mut u32).add(ISOC_SLOTS * 4);
        link.add(0).write_volatile(ring.trb.phys as u32);
        link.add(1).write_volatile((ring.trb.phys >> 32) as u32);
        link.add(2).write_volatile(0);
        link.add(3).write_volatile((6u32 << 10) | (1 << 1) | if new_cycle { 1 } else { 0 });
        ring.enqueue = 0;
        ring.cycle = new_cycle;
    }
    let e = ring.enqueue as usize;
    let buf_pa = ring.bufs.phys + (buf_idx as u64) * ISOC_BUF_SIZE as u64;
    let trb = (ring.trb.virt as *mut u32).add(e * 4);
    trb.add(0).write_volatile(buf_pa as u32);
    trb.add(1).write_volatile((buf_pa >> 32) as u32);
    trb.add(2).write_volatile((len as u32) & 0x1FFFF); // length (17b) | TD size 0 | intr 0
    trb.add(3).write_volatile(
        (5u32 << 10) | (1 << 5) | (1u32 << 31) | if ring.cycle { 1 } else { 0 }, // Type5|IOC|SIA|C
    );
    ring.enqueue = (e as u16).wrapping_add(1);
    ring.armed = ring.armed.saturating_add(1);
}

/// Sincroniza ERDP (Runtime Interrupter 0) com `er_dequeue`.
pub(crate) unsafe fn erdp_sync(st: &XhciState) {
    let er_pa = st.er_va - st.pmoff;
    let rtsoff = (r32(st.base, 0x18) & !0x1F) as u64;
    let rt = st.base + rtsoff;
    let erdp = er_pa + (st.er_dequeue as u64) * 16;
    w32(rt, 0x18, erdp as u32);
    w32(rt, 0x1C, (erdp >> 32) as u32);
}

/// Roteia UM Transfer Event (type 32) para o ring isócrono dono (match por
/// slot + DCI + faixa do TRB pointer). Eventos sem dono (Missed Service,
/// Ring Underrun, bulk antigo) são consumidos e descartados — loga 1x.
/// Retorna true sempre (o evento deve ser avançado no anel de eventos).
unsafe fn route_isoc_event(eslot: u8, epid: u8, trb_ptr: u64, len: u16) -> bool {
    let rings: [&spin::Mutex<Option<IsochRing>>; 3] = [&ISOC_IN, &ISOC_OUT, &ISOC_UVC];
    let mut routed = false;
    for r in rings {
        let mut g = r.lock();
        let Some(ring) = g.as_mut() else { continue };
        if ring.slot != eslot || ring.dci != epid {
            continue;
        }
        let rel = trb_ptr.wrapping_sub(ring.trb.phys);
        if rel >= (ISOC_SLOTS as u64) * 16 {
            continue;
        }
        let idx = (rel / 16) as usize;
        // push (idx, len) no FIFO — bounded; overflow = poll atrasado.
        let head = ring.freed_head as usize;
        let tail = ring.freed_tail as usize;
        if (tail + 1) % ISOC_SLOTS == head {
            static OVERFLOW_LOGGED: core::sync::atomic::AtomicBool =
                core::sync::atomic::AtomicBool::new(false);
            if !OVERFLOW_LOGGED.swap(true, Ordering::Relaxed) {
                crate::slog_nano!(
                    "USB",
                    "isoc",
                    "FIFO cheio — eventos descartados (poll atrasado, ring drenando)"
                );
            }
        } else {
            ring.freed[tail] = (idx as u16, len);
            ring.freed_tail = ((tail + 1) % ISOC_SLOTS) as u16;
        }
        ring.armed = ring.armed.saturating_sub(1);
        routed = true;
        break;
    }
    if !routed {
        static UNROUTED_LOGGED: core::sync::atomic::AtomicBool =
            core::sync::atomic::AtomicBool::new(false);
        if !UNROUTED_LOGGED.swap(true, Ordering::Relaxed) {
            crate::slog_nano!(
                "USB",
                "isoc",
                "Transfer Event sem dono (slot={} ep={}) — descartado",
                eslot,
                epid
            );
        }
    }
    true
}

/// Drena Transfer Events do anel e roteia para o ring dono (UAC IN/OUT, UVC).
/// Único consumidor do anel durante streaming — polls por-ring avançariam
/// `er_dequeue` por cima de eventos de outros rings (perda de re-arm). Bounded
/// (ISOC_SLOTS*3 eventos por chamada). Retorna nº de eventos processados.
pub(crate) unsafe fn drain_isoc_events(st: &mut XhciState) -> u16 {
    let evt = st.er_va as *const u32;
    let mut consumed = 0u16;
    for _ in 0..(ISOC_SLOTS * 3) {
        let mut found = false;
        for look in 0..16u16 {
            let i = ((st.er_dequeue as u32 + look as u32) % 256) as usize;
            let dw0 = evt.add(i * 4).read_volatile();
            let dw1 = evt.add(i * 4 + 1).read_volatile();
            let dw2 = evt.add(i * 4 + 2).read_volatile();
            let dw3 = evt.add(i * 4 + 3).read_volatile();
            let ty = (dw3 >> 10) & 0x3F;
            if ty != 32 {
                continue; // Command/Port Status events ficam para outros consumidores
            }
            let eslot = ((dw3 >> 24) & 0xFF) as u8;
            let epid = ((dw3 >> 16) & 0x1F) as u8;
            let trb_ptr = (dw0 as u64) | ((dw1 as u64) << 32);
            let len = (dw2 & 0xFFFFFF) as u16;
            let _ = route_isoc_event(eslot, epid, trb_ptr, len);
            st.er_dequeue = ((i as u32 + 1) % 256) as u16;
            consumed += 1;
            found = true;
            break;
        }
        if !found {
            break;
        }
    }
    if consumed > 0 {
        erdp_sync(st);
    }
    consumed
}

/// MFINDEX (Runtime + 0x00, 14 bits, 125µs/unidade). O work order citava
/// offset 0x2C do OP — incorreto; a spec/Linux põem MFINDEX em Runtime+0.
pub(crate) unsafe fn mfindex(st: &XhciState) -> u32 {
    let rtsoff = (r32(st.base, 0x18) & !0x1F) as u64;
    r32(st.base, rtsoff) & 0x3FFF
}

/// Arma o ring inteiro (ISOC_SLOTS TRBs) + doorbell + sync MFINDEX.
/// Retorna nº de TRBs armados (0 se já armado ou sem controller).
unsafe fn arm_isoc_ring_full(ring: &mut IsochRing) -> usize {
    if ring.armed != 0 {
        return 0; // já armado
    }
    for idx in 0..ISOC_SLOTS {
        isoc_arm_trb(ring, idx, ring.max_packet);
    }
    let st_lock = XHCI_STATE.lock();
    let st = match st_lock.as_ref() {
        Some(s) => s,
        None => return 0,
    };
    core::arch::asm!("sfence", options(nostack, preserves_flags));
    let mf0 = mfindex(st);
    // Doorbell[slot] = DCI — controller passa a servir o EP a cada intervalo.
    w32(st.base, st.db_off + (ring.slot as u64) * 4, ring.dci as u32);
    // Espera ≥1 frame de MFINDEX: garante TRBs na memória antes do 1º serviço.
    for _ in 0..20_000 {
        if mfindex(st) != mf0 {
            break;
        }
        core::hint::spin_loop();
    }
    let armed = ring.armed as usize;
    drop(st_lock);
    armed
}

/// Arma o ring isócrono IN (captura) com ISOC_SLOTS TRBs de `max_packet`.
/// Deve ser chamado após bringup_uac + trust OK. Retorna nº de TRBs armados.
pub unsafe fn schedule_isoc_in() -> usize {
    let mut g = ISOC_IN.lock();
    let ring = match g.as_mut() {
        Some(r) => r,
        None => return 0,
    };
    let armed = arm_isoc_ring_full(ring);
    if armed > 0 {
        crate::slog_nano!(
            "USB",
            "uac",
            "isoc IN armado: {} TRBs slot={} dci={}",
            armed,
            ring.slot,
            ring.dci
        );
    }
    armed
}

/// Poll de captura isócrona: drena eventos (roteia p/ todos os rings) e copia
/// PCM dos slots completos do ring IN, re-armando. Copia pacotes inteiros
/// enquanto couberem em `out` (pacotes sem espaço ficam na fila do ring).
/// Retorna amostras i16 escritas.
pub unsafe fn poll_isoc_in(out: &mut [i16]) -> usize {
    {
        let mut st = XHCI_STATE.lock();
        let Some(s) = st.as_mut() else { return 0 };
        drain_isoc_events(s);
    }
    let mut rg = ISOC_IN.lock();
    let Some(ring) = rg.as_mut() else { return 0 };
    let mut samples = 0usize;
    loop {
        if ring.freed_head == ring.freed_tail {
            break;
        }
        let (idx, len) = ring.freed[ring.freed_head as usize];
        let idx = idx as usize;
        let max_smps = (ring.max_packet as usize) / 2;
        let n_smps = ((len as usize).min(ISOC_BUF_SIZE)) / 2;
        if n_smps == 0 {
            // pacote vazio: pop + re-arm sem copiar.
            ring.freed_head = ((ring.freed_head as usize + 1) % ISOC_SLOTS) as u16;
            isoc_arm_trb(ring, idx, ring.max_packet);
            continue;
        }
        let n_smps = n_smps.min(max_smps);
        if samples + n_smps > out.len() {
            break; // sem espaço — pacote PERMANECE na fila (peek-before-pop)
        }
        ring.freed_head = ((ring.freed_head as usize + 1) % ISOC_SLOTS) as u16;
        let src = (ring.bufs.virt as *const u8).add(idx * ISOC_BUF_SIZE) as *const i16;
        for smp in 0..n_smps {
            out[samples + smp] = src.add(smp).read_volatile();
        }
        samples += n_smps;
        isoc_arm_trb(ring, idx, ring.max_packet);
    }
    samples
}

/// Poll de playback: drena eventos e re-arma slots liberados do ring OUT com
/// silêncio (mantém o stream vivo sem PCM — evita Ring Underrun).
/// Retorna nº de slots re-armados.
pub unsafe fn poll_isoc_out() -> usize {
    {
        let mut st = XHCI_STATE.lock();
        let Some(s) = st.as_mut() else { return 0 };
        drain_isoc_events(s);
    }
    let mut rg = ISOC_OUT.lock();
    let Some(ring) = rg.as_mut() else { return 0 };
    let mut n = 0usize;
    while ring.freed_head != ring.freed_tail {
        let (idx, _len) = ring.freed[ring.freed_head as usize];
        ring.freed_head = ((ring.freed_head as usize + 1) % ISOC_SLOTS) as u16;
        let idx = idx as usize;
        core::ptr::write_bytes(
            (ring.bufs.virt as *mut u8).add(idx * ISOC_BUF_SIZE),
            0,
            ring.max_packet as usize,
        );
        isoc_arm_trb(ring, idx, ring.max_packet);
        n += 1;
    }
    n
}

// ── UVC (USB Video Class) — captura isócrona de frames ────────────────
// Reusa IsochRing/drain_isoc_events/erdp_sync/mfindex do Phase 3. O poll
// devolve UM pacote cru por chamada (header UVC de 2+ bytes incluído); o
// caller (jarbas) monta frames MJPEG/YUY2.

/// Arma o ring isócrono IN do UVC (câmera). Deve ser chamado após bringup_uvc.
/// Retorna nº de TRBs armados.
pub unsafe fn schedule_isoc_in_frame() -> usize {
    let mut g = ISOC_UVC.lock();
    let ring = match g.as_mut() {
        Some(r) => r,
        None => return 0,
    };
    let armed = arm_isoc_ring_full(ring);
    if armed > 0 {
        crate::slog_nano!(
            "USB",
            "uvc",
            "isoc frame armado: {} TRBs slot={} dci={}",
            armed,
            ring.slot,
            ring.dci
        );
    }
    armed
}

/// Poll de captura UVC: drena eventos (roteia p/ todos os rings) e devolve UM
/// pacote cru por chamada em `out` (header UVC incluído, ≤ max_packet).
/// Re-arma o slot após copiar. Retorna tamanho do pacote (0 = nada pendente —
/// o caller deve parar de chamar até o próximo poll).
pub unsafe fn poll_isoc_frame(out: &mut [u8]) -> usize {
    {
        let mut st = XHCI_STATE.lock();
        let Some(s) = st.as_mut() else { return 0 };
        drain_isoc_events(s);
    }
    let mut rg = ISOC_UVC.lock();
    let Some(ring) = rg.as_mut() else { return 0 };
    if ring.freed_head == ring.freed_tail {
        return 0;
    }
    let (idx, len) = ring.freed[ring.freed_head as usize];
    let idx = idx as usize;
    let len = (len as usize).min(ring.max_packet as usize).min(ISOC_BUF_SIZE);
    if len == 0 {
        // pacote vazio: pop + re-arm sem copiar.
        ring.freed_head = ((ring.freed_head as usize + 1) % ISOC_SLOTS) as u16;
        isoc_arm_trb(ring, idx, ring.max_packet);
        return 0;
    }
    if len > out.len() {
        return 0; // buffer pequeno demais — pacote PERMANECE na fila (sem truncar)
    }
    ring.freed_head = ((ring.freed_head as usize + 1) % ISOC_SLOTS) as u16;
    let src = (ring.bufs.virt as *const u8).add(idx * ISOC_BUF_SIZE);
    core::ptr::copy_nonoverlapping(src, out.as_mut_ptr(), len);
    isoc_arm_trb(ring, idx, ring.max_packet);
    len
}

/// Playback isócrono: copia PCM (i16) para slots liberados (pacotes completos),
/// fluxo = sobra pendente ++ pcm; o resto dos slots ganha silêncio. A sobra
/// parcial fica em `ring.pending` p/ a próxima chamada. Retorna amostras i16
/// novas enfileiradas.
pub unsafe fn schedule_isoc_out(pcm: &[i16]) -> usize {
    {
        let mut st = XHCI_STATE.lock();
        let Some(s) = st.as_mut() else { return 0 };
        drain_isoc_events(s);
    }
    let mut rg = ISOC_OUT.lock();
    let Some(ring) = rg.as_mut() else { return 0 };
    let max_smps = (ring.max_packet as usize) / 2;
    let pl0 = ring.pending_len as usize;
    let total = pl0 + pcm.len();
    let mut stream_off = 0usize;
    while ring.freed_head != ring.freed_tail {
        let (idx, _len) = ring.freed[ring.freed_head as usize];
        ring.freed_head = ((ring.freed_head as usize + 1) % ISOC_SLOTS) as u16;
        let idx = idx as usize;
        let dst = (ring.bufs.virt as *mut u8).add(idx * ISOC_BUF_SIZE) as *mut i16;
        let remaining = total.saturating_sub(stream_off);
        let n_use = if remaining >= max_smps { max_smps } else { 0 };
        if n_use > 0 {
            for c in 0..n_use {
                let p = stream_off + c;
                let v = if p < pl0 { ring.pending[p] } else { pcm[p - pl0] };
                dst.add(c).write_volatile(v);
            }
            stream_off += n_use;
        } else {
            core::ptr::write_bytes(dst as *mut u8, 0, ring.max_packet as usize);
        }
        isoc_arm_trb(ring, idx, ring.max_packet);
    }
    // Guarda a sobra (stream_off..total) de volta em pending (memmove-safe:
    // leitura sempre em índice >= escrita).
    let leftover = total.saturating_sub(stream_off);
    let stash = leftover.min(ring.pending.len());
    if stash < leftover {
        // Circuit breaker: anel OUT parado (device sumiu / erro) — dropa PCM.
        static DROPPED: core::sync::atomic::AtomicBool =
            core::sync::atomic::AtomicBool::new(false);
        if !DROPPED.swap(true, Ordering::Relaxed) {
            crate::slog_nano!(
                "USB",
                "uac",
                "isoc OUT: pending overflow ({} amostras) — ring parado?",
                leftover
            );
        }
    }
    for i in 0..stash {
        let p = stream_off + i;
        let v = if p < pl0 { ring.pending[p] } else { pcm[p - pl0] };
        ring.pending[i] = v;
    }
    ring.pending_len = stash as u16;
    // Amostras NOVAS enfileiradas = o que saiu do fluxo além do pending inicial.
    stream_off.saturating_sub(pl0).min(pcm.len())
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
    // Normal TRB: type=1 nos bits 10–15, Cycle no bit 0 (xHCI 6.4.1).
    // 0x1 só setava Cycle — Intel xHCI no metal ignora TRB sem tipo.
    let c = if ep.cycle { 1u32 } else { 0 };
    trb.add(3).write_volatile((1u32 << 10) | c);

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
        // Link TRB type=6 bits 10–15, Toggle Cycle bit 1, Cycle bit 0
        let c = if ep.cycle { 1u32 } else { 0 };
        link.add(3).write_volatile((6u32 << 10) | (1u32 << 1) | c);
    }
    ep.enqueue_idx = if next == max - 1 { 0 } else { next as u16 };
    if next == max - 1 { ep.cycle = !ep.cycle; }

    // Ring doorbell[Slot] = DCI (xHCI 4.2.1)
    let dci = if direction == 0 { 2u32 } else { 3u32 }; // EP1 OUT=2, EP1 IN=3
    w32(st.base, st.db_off + (slot as u64) * 4, dci);

    // Wait for completion event: varre de er_dequeue (padrão wait_cmd_completion)
    // e casa o TRB pointer do evento com o TRB postado (coexiste com eventos
    // isócronos). O poll antigo lia o índice 0 fixo + escrevia ERDP com bit 32
    // setado — corrompia o anel quando isoc estava ativo.
    let my_trb_pa = ep.trb_pa + (idx as u64) * 16;
    let mut comp = 0u8;
    let mut done = false;
    for _ in 0..80_000 {
        let mut g = XHCI_STATE.lock();
        let st = match g.as_mut() { Some(s) => s, None => return false };
        let evt = st.er_va as *const u32;
        let mut found = false;
        for look in 0..16u16 {
            let i = st.er_dequeue.wrapping_add(look) % 256;
            let dw0 = evt.add(i as usize * 4).read_volatile();
            let dw1 = evt.add(i as usize * 4 + 1).read_volatile();
            let dw3 = evt.add(i as usize * 4 + 3).read_volatile();
            let trb_type = (dw3 >> 10) & 0x3F;
            if trb_type != 32 {
                continue;
            }
            let ev_ptr = (dw0 as u64) | ((dw1 as u64) << 32);
            if ev_ptr != my_trb_pa {
                continue;
            }
            comp = ((evt.add(i as usize * 4 + 2).read_volatile() >> 24) & 0xFF) as u8;
            st.er_dequeue = i.wrapping_add(1) % 256;
            erdp_sync(st);
            found = true;
            break;
        }
        drop(g);
        if found {
            done = true;
            break;
        }
        core::hint::spin_loop();
    }
    if done {
        if comp == 0 {
            return true;
        }
        crate::slog_nano!("USB", "xhci", "Bulk err: comp={}", comp);
        return false;
    }
    // Uma vez só — flood de timeout ilegível no FB.
    static TIMEOUT_LOGGED: core::sync::atomic::AtomicBool =
        core::sync::atomic::AtomicBool::new(false);
    if !TIMEOUT_LOGGED.swap(true, Ordering::Relaxed) {
        crate::slog_nano!("USB", "xhci", "Bulk timeout (demais omitidos)");
    }
    false
}

/// Tenta ler Configuration Descriptor do device UAC ativo (gravado no bringup).
/// Retorna (bytes_lidos, vid, did) ou None se não há device UAC enumerado.
pub unsafe fn try_read_config_descriptor(buf: &mut [u8]) -> Option<(usize, u16, u16)> {
    let state = XHCI_STATE.lock();
    let st = state.as_ref()?;
    if !st.uac_ready || st.uac_cfg_len == 0 {
        return None;
    }
    let n = st.uac_cfg_len.min(buf.len());
    core::ptr::copy_nonoverlapping(st.uac_cfg.as_ptr(), buf.as_mut_ptr(), n);
    Some((n, st.uac_vid, st.uac_did))
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
