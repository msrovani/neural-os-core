//! MSC bring-up no stick de boot (ADR-0062 P11): Port Reset → Enable Slot →
//! Address Device → Configure Endpoint (bulk) → devolve slot+EPs para BOT/SCSI.
//! Sem isto, `usb_msc::probe` usava slot=2 fantasma e BOOT.LOG nunca gravava.

use super::{alloc_phys, portsc_addr, r32, w32, BulkEndpoint, XHCI_STATE};

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
            let st = g.as_ref().unwrap();
            let addr = portsc_addr(st, port).unwrap();
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
        g.as_ref().unwrap().tr_va
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
        let st = g.as_ref().unwrap();
        w32(st.base, st.db_off + (slot as u64) * 4, 1);
    }
    for _ in 0..100_000 {
        let g = XHCI_STATE.lock();
        let Some(st) = g.as_ref() else { return false };
        let evt = st.er_va as *const u32;
        let trb_type = (evt.add(3).read_volatile() >> 10) & 0x3F;
        if trb_type == 32 {
            return true;
        }
        drop(g);
        core::hint::spin_loop();
    }
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
        g.as_ref().unwrap().tr_va
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
        let st = g.as_ref().unwrap();
        w32(st.base, st.db_off + (slot as u64) * 4, 1);
    }
    let mut ok = false;
    for _ in 0..200_000 {
        let g = XHCI_STATE.lock();
        let Some(st) = g.as_ref() else { return false };
        let evt = st.er_va as *const u32;
        let trb_type = (evt.add(3).read_volatile() >> 10) & 0x3F;
        if trb_type == 32 {
            let cc = (evt.add(2).read_volatile() >> 24) & 0xFF;
            ok = cc == 1 || cc == 13;
            break;
        }
        drop(g);
        core::hint::spin_loop();
    }
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
        g.as_ref().unwrap().pmoff
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
                w32(rt, 0x38, erdp as u32);
                w32(rt, 0x3C, (erdp >> 32) as u32);
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
        let st = g.as_mut().unwrap();
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
        let st = g.as_ref().unwrap();
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

    let (tr_va, tr_pa) = {
        let g = XHCI_STATE.lock();
        let st = g.as_ref().unwrap();
        (st.tr_va, st.tr_va - st.pmoff)
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

    let _ = (tr_pa, ep0_mps, setup_pa);
    core::arch::asm!("sfence", options(nostack, preserves_flags));
    {
        let g = XHCI_STATE.lock();
        let st = g.as_ref().unwrap();
        w32(st.base, st.db_off + (slot as u64) * 4, 1); // EP0 DCI=1
    }

    // Poll transfer event (reuse bulk timeout style)
    for _ in 0..200_000 {
        let g = XHCI_STATE.lock();
        let Some(st) = g.as_ref() else { return false };
        let evt = st.er_va as *const u32;
        let dw3 = evt.add(3).read_volatile();
        let trb_type = (dw3 >> 10) & 0x3F;
        if trb_type == 32 {
            let cc = (evt.add(2).read_volatile() >> 24) & 0xFF;
            return cc == 1 || cc == 13; // Success or Short Packet
        }
        drop(g);
        core::hint::spin_loop();
    }
    false
}
