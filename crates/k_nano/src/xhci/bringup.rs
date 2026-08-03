//! MSC bring-up no stick de boot (ADR-0062 P11): Port Reset → Enable Slot →
//! Address Device → Configure Endpoint (bulk) → devolve slot+EPs para BOT/SCSI.
//! Sem isto, `usb_msc::probe` usava slot=2 fantasma e BOOT.LOG nunca gravava.

use super::{alloc_phys, portsc_addr, r32, w32, BulkEndpoint, XHCI_STATE};

/// Espera um Transfer Event (type 32) no anel de eventos, varrendo a partir de
/// `er_dequeue` (padrão wait_cmd_completion) e avançando o ERDP. O poll antigo
/// lia o índice 0 fixo — nunca via eventos depois do 1º comando consumido.
/// Retorna true se CC == Success (1) ou Short Packet (13).
unsafe fn wait_transfer_event(timeout: u32) -> bool {
    for _ in 0..timeout {
        let mut g = XHCI_STATE.lock();
        let Some(st) = g.as_mut() else { return false };
        let evt = st.er_va as *const u32;
        for look in 0..16u16 {
            let i = ((st.er_dequeue as u32 + look as u32) % 256) as usize;
            let dw2 = evt.add(i * 4 + 2).read_volatile();
            let dw3 = evt.add(i * 4 + 3).read_volatile();
            let trb_type = (dw3 >> 10) & 0x3F;
            if trb_type != 32 {
                continue;
            }
            let cc = ((dw2 >> 24) & 0xFF) as u8;
            st.er_dequeue = ((i as u32 + 1) % 256) as u16;
            let er_pa = st.er_va - st.pmoff;
            let rtsoff = (r32(st.base, 0x18) & !0x1F) as u64;
            let rt = st.base + rtsoff;
            let erdp = er_pa + (st.er_dequeue as u64) * 16;
            w32(rt, 0x18, erdp as u32);
            w32(rt, 0x1C, (erdp >> 32) as u32);
            return cc == 1 || cc == 13;
        }
        drop(g);
        core::hint::spin_loop();
    }
    false
}

/// Resultado do bring-up do pendrive bootável (dados FAT / BOOT.LOG).
pub struct MscDevice {
    pub slot: u8,
    pub port: u8,
    pub speed: u8,
    pub ep_in: BulkEndpoint,
    pub ep_out: BulkEndpoint,
    pub max_packet: u16,
}

/// Enumera a 1ª porta com device conectado e configura bulk MSC.
pub unsafe fn bringup_boot_msc() -> Option<MscDevice> {
    let max_ports = {
        let g = XHCI_STATE.lock();
        g.as_ref()?.max_ports
    };

    let mut found: Option<(u8, u8)> = None; // (port, speed)
    for port in 1..=max_ports {
        let g = XHCI_STATE.lock();
        let Some(st) = g.as_ref() else { return None };
        let Some(addr) = portsc_addr(st, port) else { continue };
        let v = r32(st.base, addr - st.base);
        let ccs = v & 1 != 0;
        if !ccs {
            continue;
        }
        let speed = ((v >> 10) & 0xF) as u8;
        crate::slog_nano!(
            "USB",
            "msc",
            "porta {} CCS speed={} PORTSC={:#x}",
            port,
            speed,
            v
        );
        found = Some((port, if speed == 0 { 3 } else { speed }));
        break;
    }
    let Some((port, speed)) = found else {
        crate::slog_nano!("USB", "msc", "nenhuma porta CCS — stick ausente?");
        return None;
    };

    if !reset_port(port) {
        crate::slog_nano!("USB", "msc", "port {} reset FAIL", port);
        return None;
    }
    crate::slog_nano!("USB", "msc", "port {} reset+PED OK", port);

    let slot = match cmd_enable_slot() {
        Some(s) if s > 0 => s,
        _ => {
            crate::slog_nano!("USB", "msc", "Enable Slot FAIL");
            return None;
        }
    };
    crate::slog_nano!("USB", "msc", "Enable Slot → slot={}", slot);

    let max_packet: u16 = match speed {
        1 => 64,  // Full
        2 => 8,   // Low
        3 => 64,  // High (EP0)
        4 => 512, // Super EP0
        _ => 64,
    };

    if !address_device(slot, port, speed, max_packet) {
        crate::slog_nano!("USB", "msc", "Address Device FAIL slot={}", slot);
        return None;
    }
    crate::slog_nano!("USB", "msc", "Address Device OK slot={} port={}", slot, port);

    // SET_CONFIGURATION(1) via EP0 — necessário antes do BOT na maioria dos sticks.
    if !ep0_set_configuration(slot, max_packet, 1) {
        crate::slog_nano!("USB", "msc", "SET_CONFIGURATION warn — tenta Configure EP mesmo assim");
    }

    let bulk_mps: u16 = if speed >= 3 { 512 } else { 64 };
    let Some((ep_in, ep_out)) = configure_msc_endpoints_cmd(slot, port, speed, bulk_mps) else {
        crate::slog_nano!("USB", "msc", "Configure Endpoint MSC FAIL");
        return None;
    };

    {
        let mut g = XHCI_STATE.lock();
        if let Some(ref mut st) = *g {
            st.msc_port = port;
        }
    }

    Some(MscDevice {
        slot,
        port,
        speed,
        ep_in,
        ep_out,
        max_packet: bulk_mps,
    })
}

/// ADR-0062 P24a: HID boot keyboard em porta CCS distinta do MSC.
pub unsafe fn bringup_hid_keyboard() -> bool {
    bringup_hid_boot(HidBootKind::Keyboard)
}

/// ADR-0062 P24b: HID boot mouse em porta CCS ≠ MSC e ≠ kb.
pub unsafe fn bringup_hid_mouse() -> bool {
    bringup_hid_boot(HidBootKind::Mouse)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HidBootKind {
    Keyboard,
    Mouse,
}

unsafe fn bringup_hid_boot(kind: HidBootKind) -> bool {
    let (max_ports, msc_port, hid_port, mouse_port) = {
        let g = XHCI_STATE.lock();
        let Some(st) = g.as_ref() else { return false };
        (st.max_ports, st.msc_port, st.hid_port, st.mouse_port)
    };
    let want_proto: u8 = match kind {
        HidBootKind::Keyboard => 1,
        HidBootKind::Mouse => 2,
    };
    let tag = match kind {
        HidBootKind::Keyboard => "P24a",
        HidBootKind::Mouse => "P24b",
    };

    for port in 1..=max_ports {
        if port == msc_port || port == hid_port || port == mouse_port {
            continue;
        }
        let ccs = {
            let g = XHCI_STATE.lock();
            let Some(st) = g.as_ref() else { return false };
            let Some(addr) = portsc_addr(st, port) else { continue };
            let v = r32(st.base, addr - st.base);
            v & 1 != 0
        };
        if !ccs {
            continue;
        }
        let speed = {
            let g = XHCI_STATE.lock();
            let st = match g.as_ref() { Some(s) => s, None => return false };
            let addr = match portsc_addr(st, port) { Some(a) => a, None => continue };
            let v = r32(st.base, addr - st.base);
            let s = ((v >> 10) & 0xF) as u8;
            if s == 0 { 3 } else { s }
        };
        crate::slog_nano!("USB", "hid", "{} tentando porta {} speed={}", tag, port, speed);

        if !reset_port(port) {
            continue;
        }
        let slot = match cmd_enable_slot() {
            Some(s) if s > 0 => s,
            _ => continue,
        };
        let max_packet: u16 = match speed {
            2 => 8, // Low-speed
            1 => 64,
            _ => 64,
        };
        if !address_device(slot, port, speed, max_packet) {
            crate::slog_nano!("USB", "hid", "{} Address FAIL slot={}", tag, slot);
            continue;
        }

        // Hub class (0x09) — Labor 15: hub descriptor + port power (ADR-0073)
        if let Some(dev_class) = ep0_get_device_class(slot, max_packet) {
            if dev_class == 0x09 {
                let mut hdesc = [0u8; 16];
                if ep0_control_in(slot, max_packet, 0xA0, 0x06, 0x2900, 0, &mut hdesc) {
                    let nports = hdesc[2];
                    super::hub::mark_hub_ok(nports);
                    // PORT_POWER (feature 8) ports 1..min(nports,8)
                    for p in 1..=nports.min(8) {
                        let _ = ep0_class_no_data(slot, max_packet, 0x23, 3, 8, p as u16);
                    }
                    // Labor 21: GetPortStatus (class IN) — CCS bit0
                    let mut child_n = 0u8;
                    for p in 1..=nports.min(8) {
                        let mut st = [0u8; 4];
                        if ep0_control_in(slot, max_packet, 0xA3, 0, 0, p as u16, &mut st) {
                            let status = u16::from_le_bytes([st[0], st[1]]);
                            if status & 0x1 != 0 {
                                child_n = child_n.saturating_add(1);
                                if !super::hub::hub_child_ok() {
                                    super::hub::mark_hub_child(p);
                                    crate::slog_nano!(
                                        "USB",
                                        "hub",
                                        "hub=CHILD port={} ccs=1 (TT enum MVP; Address Device residual)",
                                        p
                                    );
                                }
                            }
                        }
                    }
                    crate::slog_nano!(
                        "USB",
                        "hub",
                        "hub=OK port={} slot={} ports={} children={} (P24c L21)",
                        port,
                        slot,
                        nports,
                        child_n
                    );
                } else {
                    crate::slog_nano!(
                        "USB",
                        "hub",
                        "hub=AWAITING port={} slot={} (hub desc fail)",
                        port,
                        slot
                    );
                }
                continue;
            }
        }
        let proto = ep0_peek_hid_boot_protocol(slot, max_packet).unwrap_or(0);
        if proto != 0 && proto != want_proto {
            crate::slog_nano!(
                "USB",
                "hid",
                "{} skip port={} proto={} (want {})",
                tag,
                port,
                proto,
                want_proto
            );
            continue;
        }
        // proto==0: best-effort (descriptor curto) — tenta mesmo assim

        let _ = ep0_set_configuration(slot, max_packet, 1);
        let _ = ep0_hid_set_protocol(slot, max_packet, 0);
        let _ = ep0_hid_set_idle(slot, max_packet);

        let report_mps: u16 = match kind {
            HidBootKind::Keyboard => 8,
            HidBootKind::Mouse => 4,
        };
        if !configure_hid_interrupt_ep(slot, port, speed, report_mps, kind) {
            crate::slog_nano!("USB", "hid", "{} Configure EP FAIL slot={}", tag, slot);
            continue;
        }

        {
            let mut g = XHCI_STATE.lock();
            if let Some(ref mut st) = *g {
                match kind {
                    HidBootKind::Keyboard => {
                        st.hid_ready = true;
                        st.hid_slot = slot;
                        st.hid_port = port;
                        st.hid_last_usage = 0;
                        crate::slog_nano!(
                            "USB",
                            "hid",
                            "P24a HID boot keyboard OK slot={} port={}",
                            slot,
                            port
                        );
                    }
                    HidBootKind::Mouse => {
                        st.mouse_ready = true;
                        st.mouse_slot = slot;
                        st.mouse_port = port;
                        st.mouse_last = [0; 4];
                        crate::slog_nano!(
                            "USB",
                            "hid",
                            "P24b HID boot mouse OK slot={} port={}",
                            slot,
                            port
                        );
                    }
                }
            }
        }
        {
            let mut g = XHCI_STATE.lock();
            if let Some(ref mut st) = *g {
                match kind {
                    HidBootKind::Keyboard => super::queue_hid_interrupt_read(st),
                    HidBootKind::Mouse => super::queue_mouse_interrupt_read(st),
                }
            }
        }
        return true;
    }
    crate::slog_nano!(
        "USB",
        "hid",
        "{}: nenhum HID boot em root ports (skip MSC={} kb={} mouse={})",
        tag,
        msc_port,
        hid_port,
        mouse_port
    );
    false
}

unsafe fn ep0_hid_set_protocol(slot: u8, ep0_mps: u16, protocol: u8) -> bool {
    // bmRequestType=0x21 class|interface|host→dev, bRequest=0x0B SET_PROTOCOL
    ep0_class_no_data(slot, ep0_mps, 0x21, 0x0B, protocol as u16, 0)
}

unsafe fn ep0_hid_set_idle(slot: u8, ep0_mps: u16) -> bool {
    // SET_IDLE: bRequest=0x0A, wValue=0 (indefinite)
    ep0_class_no_data(slot, ep0_mps, 0x21, 0x0A, 0, 0)
}

unsafe fn ep0_class_no_data(
    slot: u8,
    _ep0_mps: u16,
    bm_req: u8,
    b_req: u8,
    w_value: u16,
    w_index: u16,
) -> bool {
    let setup_pa = match alloc_phys(1) {
        Some(p) => p,
        None => return false,
    };
    let pkt: [u8; 8] = [
        bm_req,
        b_req,
        (w_value & 0xFF) as u8,
        (w_value >> 8) as u8,
        (w_index & 0xFF) as u8,
        (w_index >> 8) as u8,
        0,
        0,
    ];
    core::ptr::copy_nonoverlapping(pkt.as_ptr(), setup_pa.1, 8);
    let tr_va = {
        let g = XHCI_STATE.lock();
        match g.as_ref() { Some(s) => s.tr_va, None => return false }
    };
    let trb0 = tr_va as *mut u32;
    trb0.add(0).write_volatile(u32::from_le_bytes([pkt[0], pkt[1], pkt[2], pkt[3]]));
    trb0.add(1).write_volatile(u32::from_le_bytes([pkt[4], pkt[5], pkt[6], pkt[7]]));
    trb0.add(2).write_volatile(8);
    trb0.add(3).write_volatile((2u32 << 10) | (1 << 6) | 1);
    let trb1 = (tr_va as *mut u32).add(4);
    trb1.add(0).write_volatile(0);
    trb1.add(1).write_volatile(0);
    trb1.add(2).write_volatile(0);
    trb1.add(3).write_volatile((4u32 << 10) | (1 << 16) | (1 << 5) | 1);
    core::arch::asm!("sfence", options(nostack, preserves_flags));
    {
        let g = XHCI_STATE.lock();
        let st = match g.as_ref() { Some(s) => s, None => return false };
        w32(st.base, st.db_off + (slot as u64) * 4, 1);
    }
    let _ = wait_transfer_event(100_000);
    true // best-effort — alguns devices ignoram SET_PROTOCOL
}

unsafe fn ep0_get_device_class(slot: u8, ep0_mps: u16) -> Option<u8> {
    let mut buf = [0u8; 18];
    if !ep0_control_in(slot, ep0_mps, 0x80, 0x06, 0x0100, 0, &mut buf) {
        return None;
    }
    // bDescriptorType == 1 (DEVICE), bDeviceClass @ offset 4
    if buf[1] != 0x01 {
        return None;
    }
    Some(buf[4])
}

/// Procura bInterfaceProtocol em config truncada (HID boot: 1=kbd, 2=mouse).
unsafe fn ep0_peek_hid_boot_protocol(slot: u8, ep0_mps: u16) -> Option<u8> {
    let mut buf = [0u8; 64];
    if !ep0_control_in(slot, ep0_mps, 0x80, 0x06, 0x0200, 0, &mut buf) {
        return None;
    }
    let mut i = 0usize;
    while i + 9 <= buf.len() {
        let blen = buf[i] as usize;
        if blen < 2 {
            break;
        }
        let dtype = buf[i + 1];
        if dtype == 0x04 && blen >= 9 {
            // Interface: class@5 subclass@6 protocol@7
            let class = buf[i + 5];
            let sub = buf[i + 6];
            let proto = buf[i + 7];
            if class == 0x03 && sub == 0x01 && (proto == 1 || proto == 2) {
                return Some(proto);
            }
        }
        i += blen;
    }
    None
}

/// Control IN: Setup + Data IN + Status OUT.
unsafe fn ep0_control_in(
    slot: u8,
    _ep0_mps: u16,
    bm_req: u8,
    b_req: u8,
    w_value: u16,
    w_index: u16,
    data: &mut [u8],
) -> bool {
    let data_pa = match alloc_phys(1) {
        Some(p) => p,
        None => return false,
    };
    core::ptr::write_bytes(data_pa.1, 0, 4096);
    let len = data.len().min(512) as u16;
    let pkt: [u8; 8] = [
        bm_req,
        b_req,
        (w_value & 0xFF) as u8,
        (w_value >> 8) as u8,
        (w_index & 0xFF) as u8,
        (w_index >> 8) as u8,
        (len & 0xFF) as u8,
        (len >> 8) as u8,
    ];

    let tr_va = {
        let g = XHCI_STATE.lock();
        match g.as_ref() { Some(s) => s.tr_va, None => return false }
    };
    let trb0 = tr_va as *mut u32;
    // Setup Stage, TRT=IN data (3), IDT=1
    trb0.add(0).write_volatile(u32::from_le_bytes([pkt[0], pkt[1], pkt[2], pkt[3]]));
    trb0.add(1).write_volatile(u32::from_le_bytes([pkt[4], pkt[5], pkt[6], pkt[7]]));
    trb0.add(2).write_volatile(8);
    trb0.add(3).write_volatile((2u32 << 10) | (3u32 << 16) | (1 << 6) | 1);

    // Data Stage IN (type 3), DIR=IN
    let trb1 = (tr_va as *mut u32).add(4);
    trb1.add(0).write_volatile(data_pa.0 as u32);
    trb1.add(1).write_volatile((data_pa.0 >> 32) as u32);
    trb1.add(2).write_volatile(len as u32);
    trb1.add(3).write_volatile((3u32 << 10) | (1 << 16) | 1);

    // Status Stage OUT (type 4), DIR=OUT (0), IOC
    let trb2 = (tr_va as *mut u32).add(8);
    trb2.add(0).write_volatile(0);
    trb2.add(1).write_volatile(0);
    trb2.add(2).write_volatile(0);
    trb2.add(3).write_volatile((4u32 << 10) | (1 << 5) | 1);

    core::arch::asm!("sfence", options(nostack, preserves_flags));
    {
        let g = XHCI_STATE.lock();
        let st = match g.as_ref() { Some(s) => s, None => return false };
        w32(st.base, st.db_off + (slot as u64) * 4, 1);
    }
    let ok = wait_transfer_event(200_000);
    if ok {
        core::ptr::copy_nonoverlapping(data_pa.1, data.as_mut_ptr(), data.len());
    }
    ok
}

unsafe fn configure_hid_interrupt_ep(
    slot: u8,
    port: u8,
    speed: u8,
    max_packet: u16,
    kind: HidBootKind,
) -> bool {
    let tr = match alloc_phys(1) {
        Some(t) => t,
        None => return false,
    };
    let report = match alloc_phys(1) {
        Some(t) => t,
        None => return false,
    };
    core::ptr::write_bytes(tr.1, 0, 4096);
    core::ptr::write_bytes(report.1, 0, 4096);

    let ctx = match alloc_phys(2) {
        Some(c) => c,
        None => return false,
    };
    core::ptr::write_bytes(ctx.1, 0, 8192);
    let icc = ctx.1 as *mut u32;
    // Add EP1 IN (DCI3) + slot (A0)
    icc.add(1).write_volatile(0x9); // bits 0 and 3

    let slot_ctx = ctx.1.add(0x20) as *mut u32;
    // contextEntries=3 (EP0 + EP1 OUT unused + EP1 IN)
    slot_ctx.add(0).write_volatile((3u32 << 27) | ((speed as u32) << 20));
    slot_ctx.add(1).write_volatile((port as u32) << 16);

    // EP1 IN interrupt @ DCI3 offset 0x80
    let ep_in = ctx.1.add(0x80) as *mut u32;
    ep_in.add(0).write_volatile(0);
    // EP Type Interrupt IN = 7, CErr=3, MaxPacketSize
    ep_in.add(1).write_volatile((3u32 << 1) | (7u32 << 3) | ((max_packet as u32) << 16));
    ep_in.add(2).write_volatile(tr.0 as u32 | 1);
    ep_in.add(3).write_volatile((tr.0 >> 32) as u32);
    // Average TRB length / Interval
    let avg = max_packet as u32;
    ep_in.add(4).write_volatile(avg | ((4u32) << 16)); // interval ~8ms for FS/LS

    if !issue_address_or_config_cmd(ctx.0, slot, 12) {
        return false;
    }

    let pmoff = {
        let g = XHCI_STATE.lock();
        match g.as_ref() { Some(s) => s.pmoff, None => return false }
    };
    {
        let mut g = XHCI_STATE.lock();
        if let Some(ref mut st) = *g {
            match kind {
                HidBootKind::Keyboard => {
                    st.hid_tr_va = tr.0 + pmoff;
                    st.hid_report_va = report.0 + pmoff;
                }
                HidBootKind::Mouse => {
                    st.mouse_tr_va = tr.0 + pmoff;
                    st.mouse_report_va = report.0 + pmoff;
                }
            }
        }
    }
    true
}

unsafe fn reset_port(port: u8) -> bool {
    let g = XHCI_STATE.lock();
    let Some(st) = g.as_ref() else { return false };
    let Some(addr) = portsc_addr(st, port) else { return false };
    let off = addr - st.base;
    let mut v = r32(st.base, off);
    // W1C change bits + set PR (bit 4)
    v |= (1 << 17) | (1 << 18) | (1 << 21) | (1 << 4);
    w32(st.base, off, v);
    drop(g);

    for _ in 0..2_000_000 {
        let g = XHCI_STATE.lock();
        let Some(st) = g.as_ref() else { return false };
        let Some(addr) = portsc_addr(st, port) else { return false };
        let v = r32(st.base, addr - st.base);
        let prc = v & (1 << 21) != 0;
        let ped = v & 2 != 0;
        let pr = v & (1 << 4) != 0;
        if prc || (ped && !pr) {
            // Clear PRC
            let mut clr = v;
            clr |= 1 << 21;
            w32(st.base, addr - st.base, clr);
            return ped || prc;
        }
        core::hint::spin_loop();
    }
    false
}

unsafe fn ring_cmd_doorbell() {
    let g = XHCI_STATE.lock();
    let Some(st) = g.as_ref() else { return };
    w32(st.base, st.db_off, 0); // Doorbell 0 = Command Ring
}

unsafe fn cmd_enable_slot() -> Option<u8> {
    let (trb_va, idx, cycle) = {
        let mut g = XHCI_STATE.lock();
        let st = g.as_mut()?;
        let idx = st.cmd_enqueue as usize;
        let cycle = st.cmd_cycle;
        let trb = (st.cmd_ring_va as *mut u32).add(idx * 4);
        trb.add(0).write_volatile(0);
        trb.add(1).write_volatile(0);
        trb.add(2).write_volatile(0);
        // Type=9 Enable Slot, C=cycle
        trb.add(3).write_volatile((9u32 << 10) | if cycle { 1 } else { 0 });
        st.cmd_enqueue = st.cmd_enqueue.wrapping_add(1);
        if st.cmd_enqueue >= 255 {
            // Link TRB wrap
            let link = (st.cmd_ring_va as *mut u32).add(255 * 4);
            link.add(0).write_volatile(st.cmd_ring_pa as u32);
            link.add(1).write_volatile((st.cmd_ring_pa >> 32) as u32);
            link.add(2).write_volatile(0);
            link.add(3).write_volatile((6u32 << 10) | (1 << 1) | if cycle { 1 } else { 0 });
            st.cmd_enqueue = 0;
            st.cmd_cycle = !st.cmd_cycle;
        }
        (st.cmd_ring_va, idx, cycle)
    };
    let _ = (trb_va, idx, cycle);
    core::arch::asm!("sfence", options(nostack, preserves_flags));
    ring_cmd_doorbell();
    wait_cmd_completion().map(|(slot, _cc)| slot)
}

unsafe fn wait_cmd_completion() -> Option<(u8, u8)> {
    for _ in 0..500_000 {
        let mut g = XHCI_STATE.lock();
        let Some(st) = g.as_mut() else { return None };
        let evt = st.er_va as *const u32;
        for look in 0..16u16 {
            let i = ((st.er_dequeue + look) % 256) as usize;
            let dw2 = evt.add(i * 4 + 2).read_volatile();
            let dw3 = evt.add(i * 4 + 3).read_volatile();
            let trb_type = (dw3 >> 10) & 0x3F;
            if trb_type == 33 {
                let cc = ((dw2 >> 24) & 0xFF) as u8;
                let slot = ((dw3 >> 24) & 0xFF) as u8;
                st.er_dequeue = ((i as u16) + 1) % 256;
                let er_pa = st.er_va - st.pmoff;
                let rtsoff = (r32(st.base, 0x18) & !0x1F) as u64;
                let rt = st.base + rtsoff;
                let erdp = er_pa + (st.er_dequeue as u64) * 16;
                w32(rt, 0x18, erdp as u32);
                w32(rt, 0x1C, (erdp >> 32) as u32);
                if cc == 1 {
                    return Some((slot, cc));
                }
                crate::slog_nano!("USB", "xhci", "cmd CC={} slot={}", cc, slot);
                return None;
            }
        }
        drop(g);
        core::hint::spin_loop();
    }
    crate::slog_nano!("USB", "xhci", "cmd completion timeout");
    None
}

unsafe fn issue_address_or_config_cmd(input_ctx_pa: u64, slot: u8, trb_type: u32) -> bool {
    {
        let mut g = XHCI_STATE.lock();
        let st = match g.as_mut() { Some(s) => s, None => return false };
        let idx = st.cmd_enqueue as usize;
        let cycle = st.cmd_cycle;
        let trb = (st.cmd_ring_va as *mut u32).add(idx * 4);
        trb.add(0).write_volatile(input_ctx_pa as u32);
        trb.add(1).write_volatile((input_ctx_pa >> 32) as u32);
        trb.add(2).write_volatile(0);
        trb.add(3).write_volatile((trb_type << 10) | ((slot as u32) << 24) | if cycle { 1 } else { 0 });
        st.cmd_enqueue = st.cmd_enqueue.wrapping_add(1);
        if st.cmd_enqueue >= 255 {
            st.cmd_enqueue = 0;
            st.cmd_cycle = !st.cmd_cycle;
        }
    }
    core::arch::asm!("sfence", options(nostack, preserves_flags));
    ring_cmd_doorbell();
    wait_cmd_completion().is_some()
}

unsafe fn address_device(slot: u8, port: u8, speed: u8, ep0_mps: u16) -> bool {
    let ctx = match alloc_phys(2) {
        Some(c) => c,
        None => return false,
    };
    core::ptr::write_bytes(ctx.1, 0, 8192);

    // Output device context in DCBAA[slot]
    let out_ctx = match alloc_phys(2) {
        Some(c) => c,
        None => return false,
    };
    core::ptr::write_bytes(out_ctx.1, 0, 8192);
    {
        let g = XHCI_STATE.lock();
        let st = match g.as_ref() { Some(s) => s, None => return false };
        let dcbaa = st.dcbaa_va as *mut u64;
        dcbaa.add(slot as usize).write_volatile(out_ctx.0);
    }

    // EP0 transfer ring
    let ep0_tr = match alloc_phys(1) {
        Some(c) => c,
        None => return false,
    };
    core::ptr::write_bytes(ep0_tr.1, 0, 4096);

    let icc = ctx.1 as *mut u32;
    // Input Control: A0|A1 (slot + EP0)
    icc.add(1).write_volatile(0x3);

    // Slot Context @ +0x20 (32-byte contexts)
    let slot_ctx = ctx.1.add(0x20) as *mut u32;
    // DW0: Context Entries=1, Speed
    slot_ctx.add(0).write_volatile(((1u32) << 27) | ((speed as u32) << 20));
    // DW1: Root Hub Port Number
    slot_ctx.add(1).write_volatile((port as u32) << 16);

    // EP0 Context @ +0x40
    let ep0 = ctx.1.add(0x40) as *mut u32;
    // DW0: EP State=0
    ep0.add(0).write_volatile(0);
    // DW1: CErr=3, EP Type=Control(4), Max Packet Size
    ep0.add(1).write_volatile((3u32 << 1) | (4u32 << 3) | ((ep0_mps as u32) << 16));
    // DW2-3: TR Dequeue + DCS=1
    ep0.add(2).write_volatile(ep0_tr.0 as u32 | 1);
    ep0.add(3).write_volatile((ep0_tr.0 >> 32) as u32);

    // Guardar EP0 ring VA no state.tr_va para SET_CONFIGURATION
    {
        let mut g = XHCI_STATE.lock();
        if let Some(st) = g.as_mut() {
            st.tr_va = ep0_tr.0 + st.pmoff;
            st.slot = slot;
        }
    }

    issue_address_or_config_cmd(ctx.0, slot, 11) // Address Device
}

unsafe fn configure_msc_endpoints_cmd(
    slot: u8,
    port: u8,
    speed: u8,
    max_packet: u16,
) -> Option<(BulkEndpoint, BulkEndpoint)> {
    let tr_in = alloc_phys(1)?;
    let tr_out = alloc_phys(1)?;
    core::ptr::write_bytes(tr_in.1, 0, 4096);
    core::ptr::write_bytes(tr_out.1, 0, 4096);

    let ctx = alloc_phys(2)?;
    core::ptr::write_bytes(ctx.1, 0, 8192);

    let icc = ctx.1 as *mut u32;
    // Add EP1 OUT (DCI2) + EP1 IN (DCI3); also update slot (A0)
    icc.add(1).write_volatile(0xD); // bits 0,2,3

    let slot_ctx = ctx.1.add(0x20) as *mut u32;
    slot_ctx.add(0).write_volatile(((3u32) << 27) | ((speed as u32) << 20));
    slot_ctx.add(1).write_volatile((port as u32) << 16);

    // EP1 OUT @ DCI2 → offset 0x20 + 2*0x20 = 0x60
    let ep_out = ctx.1.add(0x60) as *mut u32;
    ep_out.add(0).write_volatile(0);
    // Bulk OUT type=2
    ep_out.add(1).write_volatile((3u32 << 1) | (2u32 << 3) | ((max_packet as u32) << 16));
    ep_out.add(2).write_volatile(tr_out.0 as u32 | 1);
    ep_out.add(3).write_volatile((tr_out.0 >> 32) as u32);

    // EP1 IN @ DCI3 → offset 0x80
    let ep_in = ctx.1.add(0x80) as *mut u32;
    ep_in.add(0).write_volatile(0);
    // Bulk IN type=6
    ep_in.add(1).write_volatile((3u32 << 1) | (6u32 << 3) | ((max_packet as u32) << 16));
    ep_in.add(2).write_volatile(tr_in.0 as u32 | 1);
    ep_in.add(3).write_volatile((tr_in.0 >> 32) as u32);

    if !issue_address_or_config_cmd(ctx.0, slot, 12) {
        return None;
    }

    Some((
        BulkEndpoint {
            trb_pa: tr_in.0,
            trb_va: tr_in.1 as *mut u32,
            enqueue_idx: 0,
            cycle: true,
            max_entries: 256,
        },
        BulkEndpoint {
            trb_pa: tr_out.0,
            trb_va: tr_out.1 as *mut u32,
            enqueue_idx: 0,
            cycle: true,
            max_entries: 256,
        },
    ))
}

/// SET_CONFIGURATION via 3-stage control transfer no EP0.
unsafe fn ep0_set_configuration(slot: u8, ep0_mps: u16, config: u8) -> bool {
    let setup_pa = match alloc_phys(1) {
        Some(p) => p,
        None => return false,
    };
    // bmRequestType=0x00, bRequest=9 SET_CONFIGURATION, wValue=config
    let pkt: [u8; 8] = [0x00, 0x09, config, 0, 0, 0, 0, 0];
    core::ptr::copy_nonoverlapping(pkt.as_ptr(), setup_pa.1, 8);

    let tr_va = {
        let g = XHCI_STATE.lock();
        match g.as_ref() { Some(s) => s.tr_va, None => return false }
    };

    // Setup Stage TRB (type 2), IDT=1, TRT=No Data (0)
    let trb0 = tr_va as *mut u32;
    trb0.add(0).write_volatile(u32::from_le_bytes([pkt[0], pkt[1], pkt[2], pkt[3]]));
    trb0.add(1).write_volatile(u32::from_le_bytes([pkt[4], pkt[5], pkt[6], pkt[7]]));
    trb0.add(2).write_volatile(8); // Transfer Length
    trb0.add(3).write_volatile((2u32 << 10) | (1 << 6) | 1); // Setup, IDT, C=1

    // Status Stage TRB (type 4), DIR=IN (1)
    let trb1 = (tr_va as *mut u32).add(4);
    trb1.add(0).write_volatile(0);
    trb1.add(1).write_volatile(0);
    trb1.add(2).write_volatile(0);
    trb1.add(3).write_volatile((4u32 << 10) | (1 << 16) | (1 << 5) | 1); // Status, DIR, IOC, C

    let _ = (ep0_mps, setup_pa);
    core::arch::asm!("sfence", options(nostack, preserves_flags));
    {
        let g = XHCI_STATE.lock();
        let st = match g.as_ref() { Some(s) => s, None => return false };
        w32(st.base, st.db_off + (slot as u64) * 4, 1); // EP0 DCI=1
    }

    wait_transfer_event(200_000)
}

// ── UAC bring-up (ADR-0045) — isócrono ─────────────────────────────────────
// Enumera a 1ª porta CCS não usada (MSC/HID), lê o Configuration Descriptor,
// detecta interface Audio Streaming (class 0x01, subclass 0x02), faz
// SET_CONFIGURATION + SET_INTERFACE(alt real) e configura os EPs isócronos
// (Configure Endpoint). Rings ficam em ISOC_IN/ISOC_OUT; o arm inicial é feito
// por schedule_isoc_in()/schedule_isoc_out() após trust OK (jarbas).

struct UacEpInfo {
    iface: u8,
    alt: u8,
    capture_ep: u8,
    playback_ep: u8,
    max_packet: u16,
    b_interval: u8,
    sample_rate: u16,
}

/// Parseia o Configuration Descriptor procurando interface UAC streaming com
/// EPs isócronos. Prefere o alt setting mais alto que tenha EPs (alt 0 = zero
/// bandwidth, sem EPs). Extrai taxa de amostragem do descriptor de formato
/// (CS_INTERFACE 0x24, FORMAT_TYPE PCM) quando presente.
fn parse_uac_config(cfg: &[u8]) -> Option<UacEpInfo> {
    if cfg.len() < 9 || cfg[1] != 0x02 {
        return None;
    }
    let total = u16::from_le_bytes([cfg[2], cfg[3]]) as usize;
    let end = total.min(cfg.len());
    let mut cur = UacEpInfo {
        iface: 0,
        alt: 0,
        capture_ep: 0,
        playback_ep: 0,
        max_packet: 0,
        b_interval: 1,
        sample_rate: 48000,
    };
    let mut chosen: Option<UacEpInfo> = None;
    let mut in_streaming = false;
    let mut i = 0usize;
    while i + 2 <= end {
        let blen = cfg[i] as usize;
        if blen < 2 || i + blen > end {
            break;
        }
        let dtype = cfg[i + 1];
        match dtype {
            0x04 if blen >= 9 => {
                // INTERFACE: fecha o candidato anterior com EPs e abre novo.
                if cur.capture_ep != 0 || cur.playback_ep != 0 {
                    chosen = Some(cur);
                }
                let if_class = cfg[i + 5];
                let if_sub = cfg[i + 6];
                cur = UacEpInfo {
                    iface: cfg[i + 2],
                    alt: cfg[i + 3],
                    capture_ep: 0,
                    playback_ep: 0,
                    max_packet: 0,
                    b_interval: 1,
                    sample_rate: 48000,
                };
                in_streaming = if_class == 0x01 && if_sub == 0x02;
            }
            0x05 if blen >= 7 && in_streaming => {
                // ENDPOINT: isócrono (attr & 0x03 == 0x01).
                let attr = cfg[i + 3];
                if attr & 0x03 == 0x01 {
                    let addr = cfg[i + 2];
                    let maxp = u16::from_le_bytes([cfg[i + 4], cfg[i + 5]]);
                    cur.b_interval = cfg[i + 6].max(1);
                    cur.max_packet = cur.max_packet.max(maxp);
                    if addr & 0x80 != 0 {
                        cur.capture_ep = addr;
                    } else {
                        cur.playback_ep = addr;
                    }
                }
            }
            0x24 if blen >= 11 && in_streaming => {
                // CS_INTERFACE FORMAT_TYPE (subtype 1): tSamFreq[0] em +8..+10.
                if cfg[i + 2] == 0x01 && cfg[i + 7] > 0 {
                    let rate = u32::from_le_bytes([cfg[i + 8], cfg[i + 9], cfg[i + 10], 0]);
                    if (8000..=96000).contains(&rate) {
                        cur.sample_rate = rate as u16;
                    }
                }
            }
            _ => {}
        }
        i += blen;
    }
    if cur.capture_ep != 0 || cur.playback_ep != 0 {
        chosen = Some(cur);
    }
    chosen.filter(|c| c.capture_ep != 0 || c.playback_ep != 0)
}

/// Escreve o Endpoint Context isócrono (32B) no input context, DCI dado.
unsafe fn write_ep_ctx(ctx: *mut u8, dci: u32, ep_type: u32, max_packet: u16, interval: u8, ring_pa: u64) {
    let ep = ctx.add(0x20 + (dci as usize) * 0x20) as *mut u32;
    // DW0: Interval (log2 µframes, bits 23:16) + EP state 0
    ep.add(0).write_volatile((interval as u32) << 16);
    // DW1: CErr=3 | EP type (1=isoc OUT, 5=isoc IN) | MaxPacketSize
    ep.add(1).write_volatile((3u32 << 1) | (ep_type << 3) | ((max_packet as u32) << 16));
    // DW2/3: TR Dequeue Pointer + DCS=1
    ep.add(2).write_volatile(ring_pa as u32 | 1);
    ep.add(3).write_volatile((ring_pa >> 32) as u32);
    // DW4: Average TRB Length + Max ESIT Payload
    ep.add(4).write_volatile((max_packet as u32) | ((max_packet as u32) << 16));
}

/// Configura os EPs isócronos (IN e/ou OUT) via Configure Endpoint e cria os
/// rings. `cap`/`play` = (endpoint_address, max_packet) do descriptor.
unsafe fn configure_isoc_endpoints(
    slot: u8,
    port: u8,
    speed: u8,
    cap: Option<(u8, u16)>,
    play: Option<(u8, u16)>,
    interval_field: u8,
) -> bool {
    let ctx = match alloc_phys(2) {
        Some(c) => c,
        None => return false,
    };
    core::ptr::write_bytes(ctx.1, 0, 8192);

    let dci_in = cap.map(|(a, _)| ((a & 0x0F) as u32) * 2 + 1); // IN: 2n+1
    let dci_out = play.map(|(a, _)| ((a & 0x0F) as u32) * 2); // OUT: 2n
    let mut add: u32 = 1; // slot context
    if let Some(d) = dci_in {
        add |= 1 << d;
    }
    if let Some(d) = dci_out {
        add |= 1 << d;
    }

    let icc = ctx.1 as *mut u32;
    icc.add(1).write_volatile(add); // Add flags (DW1 do input control ctx)

    let max_dci = dci_in.unwrap_or(0).max(dci_out.unwrap_or(0));
    let slot_ctx = ctx.1.add(0x20) as *mut u32;
    slot_ctx.add(0).write_volatile((max_dci << 27) | ((speed as u32) << 20));
    slot_ctx.add(1).write_volatile((port as u32) << 16);

    let mut ring_in = None;
    let mut ring_out = None;
    if let Some((_, mps)) = cap {
        let d = dci_in.unwrap();
        let mps = mps.min(super::ISOC_BUF_SIZE as u16);
        let ring = match super::new_isoc_ring(slot, d as u8, mps) {
            Some(r) => r,
            None => {
                crate::slog_nano!("USB", "uac", "alloc ring IN falhou");
                return false;
            }
        };
        write_ep_ctx(ctx.1, d, 5, mps, interval_field, ring.trb.phys);
        ring_in = Some(ring);
    }
    if let Some((_, mps)) = play {
        let d = dci_out.unwrap();
        let mps = mps.min(super::ISOC_BUF_SIZE as u16);
        let ring = match super::new_isoc_ring(slot, d as u8, mps) {
            Some(r) => r,
            None => {
                crate::slog_nano!("USB", "uac", "alloc ring OUT falhou");
                return false;
            }
        };
        write_ep_ctx(ctx.1, d, 1, mps, interval_field, ring.trb.phys);
        ring_out = Some(ring);
    }

    if !issue_address_or_config_cmd(ctx.0, slot, 12) {
        crate::slog_nano!("USB", "uac", "Configure Endpoint FAIL slot={} add={:#x}", slot, add);
        return false;
    }
    *super::ISOC_IN.lock() = ring_in;
    *super::ISOC_OUT.lock() = ring_out;
    crate::slog_nano!(
        "USB",
        "uac",
        "Configure Endpoint isoc OK slot={} dci_in={:?} dci_out={:?} interval={}",
        slot,
        dci_in,
        dci_out,
        interval_field
    );
    true
}

/// Field Interval do endpoint context: log2 do intervalo em µframes (125µs).
/// FS/LS: bInterval em ms → bInterval*8 µframes. HS/SS: 2^(bInterval-1).
fn interval_field_for(speed: u8, b_interval: u8) -> u8 {
    let b = (b_interval.max(1)) as u32;
    match speed {
        1 | 2 => {
            let uf = b * 8;
            uf.next_power_of_two().trailing_zeros().min(15) as u8
        }
        _ => b.saturating_sub(1).min(15) as u8,
    }
}

/// Registra o device que o UAC tentou (para o UVC reusar o slot — webcam com
/// mic tem interfaces Audio E Video no MESMO device/slot; re-enumerar mataria o
/// stream de áudio). Captura o EP0 ring (st.tr_va) antes de outro address.
unsafe fn mark_uac_tried(slot: u8, port: u8, speed: u8, cfg: Option<&[u8]>) {
    let mut g = XHCI_STATE.lock();
    if let Some(st) = g.as_mut() {
        st.uac_port = port;
        st.uac_slot = slot;
        st.uac_speed = speed;
        st.uac_ep0_tr_va = st.tr_va;
        if let Some(c) = cfg {
            let n = c.len().min(512);
            st.uac_cfg[..n].copy_from_slice(&c[..n]);
            st.uac_cfg_len = n;
        }
    }
}

/// Enumera e configura um device USB Audio Class (isócrono). Retorna info para
/// jarbas preencher os atomics UAC_*. Sem device UAC → None (comportamento
/// existente inalterado). Idempotente via `uac_port` marcado no 1º try.
pub unsafe fn bringup_uac() -> Option<super::UacDevice> {
    let (max_ports, msc_port, hid_port, mouse_port, uac_port) = {
        let g = XHCI_STATE.lock();
        let st = g.as_ref()?;
        (st.max_ports, st.msc_port, st.hid_port, st.mouse_port, st.uac_port)
    };
    if uac_port != 0 {
        return None; // já tentado
    }

    for port in 1..=max_ports {
        if port == msc_port || port == hid_port || port == mouse_port {
            continue;
        }
        let ccs = {
            let g = XHCI_STATE.lock();
            let Some(st) = g.as_ref() else { return None };
            let Some(addr) = portsc_addr(st, port) else { continue };
            r32(st.base, addr - st.base) & 1 != 0
        };
        if !ccs {
            continue;
        }
        let speed = {
            let g = XHCI_STATE.lock();
            let st = match g.as_ref() { Some(s) => s, None => return None };
            let addr = match portsc_addr(st, port) { Some(a) => a, None => continue };
            let s = ((r32(st.base, addr - st.base) >> 10) & 0xF) as u8;
            if s == 0 { 3 } else { s }
        };
        crate::slog_nano!("USB", "uac", "tentando porta {} speed={}", port, speed);

        if !reset_port(port) {
            continue;
        }
        let slot = match cmd_enable_slot() {
            Some(s) if s > 0 => s,
            _ => continue,
        };
        let ep0_mps: u16 = match speed {
            2 => 8,
            1 => 64,
            3 => 64,
            4 => 512,
            _ => 64,
        };
        if !address_device(slot, port, speed, ep0_mps) {
            continue;
        }

        let mut ddesc = [0u8; 18];
        if !ep0_control_in(slot, ep0_mps, 0x80, 0x06, 0x0100, 0, &mut ddesc) {
            mark_uac_tried(slot, port, speed, None);
            continue;
        }
        let vid = u16::from_le_bytes([ddesc[8], ddesc[9]]);
        let did = u16::from_le_bytes([ddesc[10], ddesc[11]]);

        let mut cfg = [0u8; 512];
        if !ep0_control_in(slot, ep0_mps, 0x80, 0x06, 0x0200, 0, &mut cfg) {
            mark_uac_tried(slot, port, speed, None);
            continue;
        }
        let Some(info) = parse_uac_config(&cfg) else {
            crate::slog_nano!(
                "USB",
                "uac",
                "port {} slot {} vid={:04x} did={:04x} — sem interface Audio Streaming",
                port,
                slot,
                vid,
                did
            );
            mark_uac_tried(slot, port, speed, Some(&cfg));
            continue;
        };

        let _ = ep0_set_configuration(slot, ep0_mps, 1);
        if info.alt > 0
            && !ep0_class_no_data(slot, ep0_mps, 0x01, 0x0B, info.alt as u16, info.iface as u16)
        {
            crate::slog_nano!(
                "USB",
                "uac",
                "SET_INTERFACE(iface={} alt={}) FAIL — alt setting ignorado",
                info.iface,
                info.alt
            );
            mark_uac_tried(slot, port, speed, Some(&cfg));
            continue;
        }

        let interval_field = interval_field_for(speed, info.b_interval);
        let cap = if info.capture_ep != 0 {
            Some((info.capture_ep, info.max_packet))
        } else {
            None
        };
        let play = if info.playback_ep != 0 {
            Some((info.playback_ep, info.max_packet))
        } else {
            None
        };
        if !configure_isoc_endpoints(slot, port, speed, cap, play, interval_field) {
            mark_uac_tried(slot, port, speed, Some(&cfg));
            continue;
        }

        let cfg_len = (u16::from_le_bytes([cfg[2], cfg[3]]) as usize).min(512);
        {
            let mut g = XHCI_STATE.lock();
            if let Some(st) = g.as_mut() {
                st.uac_ready = true;
                st.uac_slot = slot;
                st.uac_port = port;
                st.uac_speed = speed;
                st.uac_vid = vid;
                st.uac_did = did;
                st.uac_capture_ep = info.capture_ep;
                st.uac_playback_ep = info.playback_ep;
                st.uac_sample_rate = info.sample_rate;
                st.uac_cfg = cfg;
                st.uac_cfg_len = cfg_len;
                st.uac_ep0_tr_va = st.tr_va;
            }
        }
        crate::slog_nano!(
            "USB",
            "uac",
            "UAC OK slot={} port={} vid={:04x} did={:04x} cap_ep={:#04x} play_ep={:#04x} rate={} max_pkt={}",
            slot,
            port,
            vid,
            did,
            info.capture_ep,
            info.playback_ep,
            info.sample_rate,
            info.max_packet
        );
        return Some(super::UacDevice {
            slot,
            port,
            speed,
            vid,
            did,
            capture_ep: info.capture_ep,
            playback_ep: info.playback_ep,
            sample_rate: info.sample_rate,
            max_packet: info.max_packet,
        });
    }
    None
}

// ── UVC bring-up (Phase 4) — câmera isócrona ───────────────────────────────
// Device class 0x0E (Video), subclass 0x01 (VideoControl) / 0x02
// (VideoStreaming). Reusa o mecanismo isócrono do Phase 3.

struct UvcEpInfo {
    iface: u8,
    alt: u8,
    ep: u8,
    max_packet: u16,
    b_interval: u8,
    width: u16,
    height: u16,
    fps: u16,
    /// 1 = MJPEG, 0 = YUY2/raw.
    format: u8,
}

/// Parseia o Configuration Descriptor procurando interface UVC VideoStreaming
/// (class 0x0E, subclass 0x02) com endpoint isócrono IN. Extrai width/height/
/// fps dos VS_FRAME descriptors (dwDefaultFrameInterval em 100ns → fps =
/// 10^7/interval) e formato (VS_FORMAT_MJPEG 0x06 / UNCOMPRESSED 0x04).
/// Prefere o alt setting mais alto com EP; MJPEG se disponível.
fn parse_uvc_config(cfg: &[u8]) -> Option<UvcEpInfo> {
    if cfg.len() < 9 || cfg[1] != 0x02 {
        return None;
    }
    let total = u16::from_le_bytes([cfg[2], cfg[3]]) as usize;
    let end = total.min(cfg.len());
    let mut cur = UvcEpInfo {
        iface: 0,
        alt: 0,
        ep: 0,
        max_packet: 0,
        b_interval: 1,
        width: 0,
        height: 0,
        fps: 0,
        format: 1,
    };
    let mut chosen: Option<UvcEpInfo> = None;
    let mut width = 0u16;
    let mut height = 0u16;
    let mut fps = 0u16;
    let mut is_mjpeg = false;
    let mut in_vs = false;
    let mut i = 0usize;
    while i + 2 <= end {
        let blen = cfg[i] as usize;
        if blen < 2 || i + blen > end {
            break;
        }
        let dtype = cfg[i + 1];
        match dtype {
            0x04 if blen >= 9 => {
                // INTERFACE: fecha candidato anterior e abre novo.
                if cur.ep != 0 {
                    chosen = Some(cur);
                }
                let class = cfg[i + 5];
                let sub = cfg[i + 6];
                cur = UvcEpInfo {
                    iface: cfg[i + 2],
                    alt: cfg[i + 3],
                    ep: 0,
                    max_packet: 0,
                    b_interval: 1,
                    width: 0,
                    height: 0,
                    fps: 0,
                    format: 1,
                };
                in_vs = class == 0x0E && sub == 0x02;
            }
            0x24 if blen >= 3 && in_vs => {
                // CS_INTERFACE (dentro do VideoStreaming).
                let sub = cfg[i + 2];
                match sub {
                    0x06 => is_mjpeg = true,          // VS_FORMAT_MJPEG
                    0x07 if blen >= 24 => {           // VS_FRAME_MJPEG
                        is_mjpeg = true;
                        if width == 0 || height == 0 {
                            width = u16::from_le_bytes([cfg[i + 4], cfg[i + 5]]);
                            height = u16::from_le_bytes([cfg[i + 6], cfg[i + 7]]);
                        }
                        if fps == 0 {
                            let iv = u32::from_le_bytes([
                                cfg[i + 20],
                                cfg[i + 21],
                                cfg[i + 22],
                                cfg[i + 23],
                            ]);
                            if iv != 0 {
                                fps = (10_000_000u32 / iv).clamp(1, 240) as u16;
                            }
                        }
                    }
                    0x05 if blen >= 24 => {           // VS_FRAME_UNCOMPRESSED
                        if width == 0 || height == 0 {
                            width = u16::from_le_bytes([cfg[i + 4], cfg[i + 5]]);
                            height = u16::from_le_bytes([cfg[i + 6], cfg[i + 7]]);
                        }
                        if fps == 0 {
                            let iv = u32::from_le_bytes([
                                cfg[i + 20],
                                cfg[i + 21],
                                cfg[i + 22],
                                cfg[i + 23],
                            ]);
                            if iv != 0 {
                                fps = (10_000_000u32 / iv).clamp(1, 240) as u16;
                            }
                        }
                    }
                    _ => {}
                }
            }
            0x05 if blen >= 7 && in_vs => {
                // ENDPOINT isócrono IN (alt com bandwidth).
                let attr = cfg[i + 3];
                if attr & 0x03 == 0x01 {
                    let addr = cfg[i + 2];
                    if addr & 0x80 != 0 {
                        let maxp = u16::from_le_bytes([cfg[i + 4], cfg[i + 5]]);
                        if maxp > cur.max_packet {
                            cur.ep = addr;
                            cur.max_packet = maxp;
                            cur.b_interval = cfg[i + 6].max(1);
                        }
                    }
                }
            }
            _ => {}
        }
        i += blen;
    }
    if cur.ep != 0 {
        chosen = Some(cur);
    }
    let mut info = chosen?;
    info.width = width.max(1);
    info.height = height.max(1);
    info.fps = fps.max(1);
    info.format = if is_mjpeg { 1 } else { 0 };
    Some(info)
}

/// Configura o EP isócrono IN do UVC via Configure Endpoint (aditivo: para
/// webcam com mic, o slot já tem EPs de áudio — `last_ctx` cobre o maior DCI).
/// Cria o ring e o guarda em ISOC_UVC (o ring vive só no static; o poll usa
/// `ISOC_UVC.lock()` — retornar o ring causava use-after-move).
unsafe fn configure_uvc_endpoint(
    slot: u8,
    port: u8,
    speed: u8,
    ep_addr: u8,
    max_packet: u16,
    interval_field: u8,
    last_ctx: u32,
) -> Option<()> {
    let ctx = match alloc_phys(2) {
        Some(c) => c,
        None => return None,
    };
    core::ptr::write_bytes(ctx.1, 0, 8192);

    let dci = ((ep_addr & 0x0F) as u32) * 2 + 1; // isoc IN: 2n+1
    let icc = ctx.1 as *mut u32;
    icc.add(1).write_volatile(1 | (1 << dci)); // slot + EP do vídeo

    let slot_ctx = ctx.1.add(0x20) as *mut u32;
    slot_ctx.add(0).write_volatile((last_ctx << 27) | ((speed as u32) << 20));
    slot_ctx.add(1).write_volatile((port as u32) << 16);

    let mps = max_packet.min(super::ISOC_BUF_SIZE as u16);
    if mps < max_packet {
        crate::slog_nano!(
            "USB",
            "uvc",
            "wMaxPacketSize {} > {} — truncado (high-bandwidth parcial)",
            max_packet,
            super::ISOC_BUF_SIZE
        );
    }
    let ring = super::new_isoc_ring(slot, dci as u8, mps)?;
    write_ep_ctx(ctx.1, dci, 5, mps, interval_field, ring.trb.phys);

    if !issue_address_or_config_cmd(ctx.0, slot, 12) {
        crate::slog_nano!(
            "USB",
            "uvc",
            "Configure Endpoint FAIL slot={} dci={} add={:#x}",
            slot,
            dci,
            1 | (1 << dci)
        );
        return None;
    }
    *super::ISOC_UVC.lock() = Some(ring);
    crate::slog_nano!(
        "USB",
        "uvc",
        "Configure Endpoint isoc IN OK slot={} dci={} mps={} interval={}",
        slot,
        dci,
        mps,
        interval_field
    );
    Some(())
}

/// Enumera e configura um device USB Video Class (isócrono IN). Reusa o slot
/// já endereçado pelo UAC quando o device tem UVC (webcam com mic), senão faz
/// enumeração completa em portas CCS. Idempotente via `uvc_port`.
pub unsafe fn bringup_uvc() -> Option<super::UvcDevice> {
    let (max_ports, msc_port, hid_port, mouse_port, uvc_port) = {
        let g = XHCI_STATE.lock();
        let st = g.as_ref()?;
        (st.max_ports, st.msc_port, st.hid_port, st.mouse_port, st.uvc_port)
    };
    if uvc_port != 0 {
        return None; // já tentado
    }

    // Caso 1: device endereçado pelo UAC (mesma porta) pode ter UVC no MESMO
    // slot (webcam com mic). Reusa slot/EP0 — re-enumerar mataria o áudio.
    let (uac_ready, uac_slot, uac_port, uac_speed, uac_cfg, uac_cfg_len, uac_ep0_tr_va) = {
        let g = XHCI_STATE.lock();
        match g.as_ref() {
            Some(s) => (
                s.uac_ready,
                s.uac_slot,
                s.uac_port,
                s.uac_speed,
                s.uac_cfg,
                s.uac_cfg_len,
                s.uac_ep0_tr_va,
            ),
            None => (false, 0, 0, 0, [0u8; 512], 0, 0),
        }
    };
    if uac_slot != 0 && uac_cfg_len > 0 {
        if let Some(info) = parse_uvc_config(&uac_cfg[..uac_cfg_len]) {
            crate::slog_nano!(
                "USB",
                "uvc",
                "reuso do slot {} (UAC) — UVC {:.0}x{:.0}@{} format={} ep={:#04x}",
                uac_slot,
                info.width,
                info.height,
                info.fps,
                if info.format == 1 { "MJPEG" } else { "YUY2" },
                info.ep
            );
            // Garante o EP0 ring correto p/ SET_CONFIGURATION/SET_INTERFACE.
            {
                let mut g = XHCI_STATE.lock();
                if let Some(st) = g.as_mut() {
                    st.tr_va = uac_ep0_tr_va;
                }
            }
            if let Some(dev) = configure_uvc_on_slot(uac_slot, uac_port, uac_speed, uac_ready, info) {
                return Some(dev);
            }
            // É UVC mas a configuração falhou — desiste (re-enumerar o mesmo
            // device mataria o áudio num combo e não resolveria o SET_INTERFACE).
            crate::slog_nano!(
                "USB",
                "uvc",
                "reuso do slot {} falhou — UVC_READY=false (sem re-enumeração)",
                uac_slot
            );
            {
                let mut g = XHCI_STATE.lock();
                if let Some(st) = g.as_mut() {
                    st.uvc_port = uac_port;
                }
            }
            return None;
        }
    }

    // Caso 2: portas CCS novas (skip MSC/HID/mouse; a porta que o UAC tentou
    // já foi avaliada acima — não re-enumerar).
    for port in 1..=max_ports {
        if port == msc_port || port == hid_port || port == mouse_port || port == uac_port {
            continue;
        }
        let ccs = {
            let g = XHCI_STATE.lock();
            let Some(st) = g.as_ref() else { return None };
            let Some(addr) = portsc_addr(st, port) else { continue };
            r32(st.base, addr - st.base) & 1 != 0
        };
        if !ccs {
            continue;
        }
        let speed = {
            let g = XHCI_STATE.lock();
            let st = match g.as_ref() { Some(s) => s, None => return None };
            let addr = match portsc_addr(st, port) { Some(a) => a, None => continue };
            let s = ((r32(st.base, addr - st.base) >> 10) & 0xF) as u8;
            if s == 0 { 3 } else { s }
        };
        crate::slog_nano!("USB", "uvc", "tentando porta {} speed={}", port, speed);

        if !reset_port(port) {
            continue;
        }
        let slot = match cmd_enable_slot() {
            Some(s) if s > 0 => s,
            _ => continue,
        };
        let ep0_mps: u16 = match speed {
            2 => 8,
            1 => 64,
            3 => 64,
            4 => 512,
            _ => 64,
        };
        if !address_device(slot, port, speed, ep0_mps) {
            continue;
        }
        let mut cfg = [0u8; 512];
        if !ep0_control_in(slot, ep0_mps, 0x80, 0x06, 0x0200, 0, &mut cfg) {
            continue;
        }
        let Some(info) = parse_uvc_config(&cfg) else {
            crate::slog_nano!("USB", "uvc", "port {} slot {} — sem UVC", port, slot);
            {
                let mut g = XHCI_STATE.lock();
                if let Some(st) = g.as_mut() {
                    st.uvc_port = port;
                }
            }
            continue;
        };
        if let Some(dev) = configure_uvc_on_slot(slot, port, speed, false, info) {
            return Some(dev);
        }
    }
    None
}

/// Configura o UVC num slot dado (reuso ou novo): SET_CONFIGURATION se ainda
/// não configurado, SET_INTERFACE(alt), Configure Endpoint isócrono IN.
unsafe fn configure_uvc_on_slot(
    slot: u8,
    port: u8,
    speed: u8,
    device_configured: bool,
    info: UvcEpInfo,
) -> Option<super::UvcDevice> {
    let ep0_mps: u16 = match speed {
        2 => 8,
        1 => 64,
        3 => 64,
        4 => 512,
        _ => 64,
    };
    if !device_configured {
        let _ = ep0_set_configuration(slot, ep0_mps, 1);
    }
    if info.alt > 0
        && !ep0_class_no_data(slot, ep0_mps, 0x01, 0x0B, info.alt as u16, info.iface as u16)
    {
        crate::slog_nano!(
            "USB",
            "uvc",
            "SET_INTERFACE(iface={} alt={}) FAIL — streaming não ativado",
            info.iface,
            info.alt
        );
        return None;
    }

    // last_ctx cobre o maior DCI: áudio (se UAC no mesmo slot) + vídeo.
    let video_dci = ((info.ep & 0x0F) as u32) * 2 + 1;
    let audio_max_dci = {
        let g = XHCI_STATE.lock();
        match g.as_ref() {
            Some(st) if st.uac_slot == slot => {
                let ci = ((st.uac_capture_ep & 0x0F) as u32) * 2 + 1;
                let co = ((st.uac_playback_ep & 0x0F) as u32) * 2;
                ci.max(co).max(1)
            }
            _ => 1,
        }
    };
    let interval_field = interval_field_for(speed, info.b_interval);
    configure_uvc_endpoint(
        slot,
        port,
        speed,
        info.ep,
        info.max_packet,
        interval_field,
        audio_max_dci.max(video_dci),
    )?;

    let vid = 0;
    let did = 0;
    {
        let mut g = XHCI_STATE.lock();
        if let Some(st) = g.as_mut() {
            st.uvc_ready = true;
            st.uvc_slot = slot;
            st.uvc_port = port;
            st.uvc_vid = vid;
            st.uvc_did = did;
            st.uvc_ep = info.ep;
            st.uvc_width = info.width;
            st.uvc_height = info.height;
            st.uvc_fps = info.fps;
            st.uvc_format = info.format;
            st.uvc_max_packet = info.max_packet;
        }
    }
    crate::slog_nano!(
        "USB",
        "uvc",
        "UVC OK slot={} port={} ep={:#04x} {}x{}@{} format={} max_pkt={}",
        slot,
        port,
        info.ep,
        info.width,
        info.height,
        info.fps,
        if info.format == 1 { "MJPEG" } else { "YUY2" },
        info.max_packet
    );
    Some(super::UvcDevice {
        slot,
        port,
        vid,
        did,
        ep: info.ep,
        width: info.width,
        height: info.height,
        fps: info.fps,
        format: info.format,
        max_packet: info.max_packet,
    })
}
