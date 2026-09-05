//! MSC bring-up com hub interno (route string + TT).
//!
//! Referências: Redox xhcid/usbhubd; Chitti `enumerate_hub` (Mac mini USB-A
//! atrás de hub). Sem isto, Alienware chega ao desktop (Limine leu ESP) mas
//! `BOOT.LOG`/`NSGDB` nunca gravam — stick não está em root CCS.

use k_nano::xhci::{self, MscDevice};
use core::sync::atomic::{AtomicU64, Ordering};

/// Deadline TSC do bring-up MSC (0 = sem teto). Evita tela preta pós-Limine.
static MSC_TSC_DEADLINE: AtomicU64 = AtomicU64::new(0);

#[inline]
fn msc_budget_ok() -> bool {
    let d = MSC_TSC_DEADLINE.load(Ordering::Relaxed);
    if d == 0 {
        return true;
    }
    k_nano::tsc::rdtsc() < d
}

/// Entry R1 registrada em `k_nano::xhci::register_msc_bringup`.
pub unsafe fn bringup_boot_msc() -> Option<MscDevice> {
    // Budget 3s parede: hub×EP0 sem teto = Limine → tela preta interminável.
    let hz = k_nano::tsc::tsc_hz();
    let t0 = k_nano::tsc::rdtsc();
    if hz > 1_000_000 {
        MSC_TSC_DEADLINE.store(t0.wrapping_add(hz.saturating_mul(3)), Ordering::Relaxed);
    } else {
        MSC_TSC_DEADLINE.store(0, Ordering::Relaxed);
    }

    let max_ports = xhci::host_max_ports()?;
    let mut ccs: alloc::vec::Vec<(u8, u8)> = alloc::vec::Vec::new();
    for port in 1..=max_ports {
        if xhci::msc_port_skipped(port) {
            continue;
        }
        if let Some((speed, portsc)) = xhci::host_port_ccs(port) {
            k_nano::slog_hal!(
                "USB",
                "msc",
                "porta {} CCS speed={} PORTSC={:#x}",
                port,
                speed,
                portsc
            );
            ccs.push((port, speed));
        }
    }
    if ccs.is_empty() {
        k_nano::slog_hal!("USB", "msc", "nenhuma porta CCS — stick ausente?");
        MSC_TSC_DEADLINE.store(0, Ordering::Relaxed);
        return None;
    }
    ccs.sort_by(|a, b| b.1.cmp(&a.1));

    for (port, speed) in ccs {
        if !msc_budget_ok() {
            k_nano::slog_hal!(
                "USB",
                "warn",
                "MSC bringup budget 3s — abort (UI first; retry DriverInit)"
            );
            break;
        }
        match try_msc_on_port(port, speed) {
            Some(dev) => {
                k_nano::slog_hal!(
                    "USB",
                    "ok",
                    "MSC bringup OK port={} slot={} speed={}",
                    dev.port,
                    dev.slot,
                    speed
                );
                MSC_TSC_DEADLINE.store(0, Ordering::Relaxed);
                return Some(dev);
            }
            None => {
                xhci::mark_msc_port_failed(port);
                k_nano::slog_hal!(
                    "USB",
                    "warn",
                    "MSC bringup FAIL port={} — tenta proxima",
                    port
                );
            }
        }
    }
    k_nano::slog_hal!("USB", "warn", "MSC bringup FAIL em todas as portas CCS");
    MSC_TSC_DEADLINE.store(0, Ordering::Relaxed);
    None
}

unsafe fn try_msc_on_port(port: u8, speed: u8) -> Option<MscDevice> {
    if !xhci::host_reset_port(port, speed) {
        k_nano::slog_hal!("USB", "msc", "port {} reset FAIL", port);
        return None;
    }
    let loc = xhci::DevLoc::root(port, speed);
    let slot = match xhci::host_enable_slot(port) {
        Some(s) if s > 0 => s,
        _ => {
            k_nano::slog_hal!("USB", "msc", "Enable Slot FAIL port={}", port);
            return None;
        }
    };
    let mps = xhci::ep0_mps_for_speed(speed);
    if !xhci::host_address_device(slot, loc, mps) {
        k_nano::slog_hal!(
            "USB",
            "msc",
            "Address Device FAIL slot={} port={}",
            slot,
            port
        );
        let _ = xhci::host_disable_slot(slot);
        return None;
    }
    crate::unlock_dag::grant(crate::unlock_dag::CapToken::UsbEp0);

    if xhci::host_device_class(slot, mps) == Some(9) {
        k_nano::slog_hal!(
            "USB",
            "ok",
            "hub class @ root port={} — enumerando filhos p/ MSC",
            port
        );
        crate::unlock_dag::grant(crate::unlock_dag::CapToken::UsbHubOk);
        return try_msc_behind_hub(slot, loc, mps);
    }
    finish_msc(slot, loc, mps)
}

unsafe fn finish_msc(slot: u8, loc: xhci::DevLoc, ep0_mps: u16) -> Option<MscDevice> {
    let mut cfg = [0u8; 512];
    let msc_eps = if xhci::host_ep0_control_in(slot, ep0_mps, 0x80, 0x06, 0x0200, 0, &mut cfg)
    {
        xhci::parse_msc_config(&cfg)
    } else {
        None
    };
    let (cfg_val, ep_in, ep_out, bulk_mps) = match msc_eps {
        Some(info) => {
            k_nano::slog_hal!(
                "USB",
                "ok",
                "MSC desc cfg={} ep_in={:#x} ep_out={:#x} mps={} route={:#x}",
                info.config_value,
                info.ep_in,
                info.ep_out,
                info.max_packet,
                loc.route
            );
            (
                info.config_value.max(1),
                info.ep_in,
                info.ep_out,
                if info.max_packet >= 64 {
                    info.max_packet
                } else if loc.speed >= 3 {
                    512
                } else {
                    64
                },
            )
        }
        None => {
            k_nano::slog_hal!(
                "USB",
                "warn",
                "sem interface MSC BOT slot={} route={:#x}",
                slot,
                loc.route
            );
            return None;
        }
    };
    let _ = xhci::host_set_configuration(slot, ep0_mps, cfg_val);
    let Some((ep_in_be, ep_out_be)) =
        xhci::host_configure_msc(slot, loc, bulk_mps, ep_in, ep_out)
    else {
        let _ = xhci::host_disable_slot(slot);
        return None;
    };
    xhci::host_set_msc_port(loc.root_port);
    Some(MscDevice {
        slot,
        port: loc.root_port,
        speed: loc.speed,
        ep_in: ep_in_be,
        ep_out: ep_out_be,
        max_packet: bulk_mps,
    })
}

unsafe fn try_msc_behind_hub(
    hub_slot: u8,
    hub_loc: xhci::DevLoc,
    hub_mps: u16,
) -> Option<MscDevice> {
    let hub_ep0 = xhci::host_ep0_tr_va();
    if hub_ep0 == 0 {
        return None;
    }
    let _ = xhci::host_set_configuration(hub_slot, hub_mps, 1);
    let mut hdesc = [0u8; 15];
    if !xhci::host_ep0_control_in(hub_slot, hub_mps, 0xA0, 0x06, 0x2900, 0, &mut hdesc) {
        k_nano::slog_hal!("USB", "warn", "hub GET_DESCRIPTOR FAIL slot={}", hub_slot);
        let _ = xhci::host_disable_slot(hub_slot);
        return None;
    }
    let nbr_ports = hdesc[2].max(1).min(15);
    let characteristics = u16::from_le_bytes([hdesc[3], hdesc[4]]);
    let pwr_on_2_good = hdesc[5] as u64;
    let ttt = ((characteristics >> 5) & 0x3) as u32;
    let mtt = (characteristics & 1) != 0;
    k_nano::xhci::mark_hub_ok(nbr_ports);
    k_nano::slog_hal!(
        "USB",
        "ok",
        "hub slot={} ports={} ttt={} mtt={} — buscando MSC atrás",
        hub_slot,
        nbr_ports,
        ttt,
        mtt as u8
    );
    let _ = xhci::host_mark_hub(hub_slot, hub_loc, nbr_ports, ttt, mtt);

    for p in 1..=nbr_ports {
        let _ = xhci::host_ep0_class_nodata(hub_slot, hub_mps, 0x23, 3, 8, p as u16);
    }
    k_nano::tsc::sleep_us(2_000 + pwr_on_2_good.saturating_mul(2_000));

    for p in 1..=nbr_ports {
        if !msc_budget_ok() {
            k_nano::slog_hal!("USB", "warn", "hub MSC budget — abort mid-hub");
            break;
        }
        xhci::host_restore_ep0(hub_ep0, hub_slot);
        let mut stbuf = [0u8; 4];
        if !xhci::host_ep0_control_in(hub_slot, hub_mps, 0xA3, 0, 0, p as u16, &mut stbuf) {
            continue;
        }
        let status = u16::from_le_bytes([stbuf[0], stbuf[1]]);
        let change = u16::from_le_bytes([stbuf[2], stbuf[3]]);
        if status & 1 == 0 {
            continue;
        }
        // Linux hub: clear C_PORT_CONNECTION (feature 16) antes do reset.
        if change & 1 != 0 {
            let _ = xhci::host_ep0_class_nodata(hub_slot, hub_mps, 0x23, 1, 16, p as u16);
        }
        k_nano::xhci::mark_hub_child(p);
        k_nano::slog_hal!(
            "USB",
            "ok",
            "hub port {} status={:#x} change={:#x} — reset",
            p,
            status,
            change
        );

        let _ = xhci::host_ep0_class_nodata(hub_slot, hub_mps, 0x23, 3, 4, p as u16);
        let mut reset_ok = false;
        for _ in 0..50 {
            k_nano::tsc::sleep_us(1_000);
            xhci::host_restore_ep0(hub_ep0, hub_slot);
            if !xhci::host_ep0_control_in(hub_slot, hub_mps, 0xA3, 0, 0, p as u16, &mut stbuf) {
                continue;
            }
            let st = u16::from_le_bytes([stbuf[0], stbuf[1]]);
            let ch = u16::from_le_bytes([stbuf[2], stbuf[3]]);
            // C_PORT_RESET set e PORT_RESET clear
            if ch & (1 << 4) != 0 || (st & (1 << 4) == 0 && st & 1 != 0) {
                if st & (1 << 4) == 0 {
                    reset_ok = true;
                    break;
                }
            }
            if st & (1 << 1) != 0 && st & (1 << 4) == 0 {
                reset_ok = true;
                break;
            }
        }
        if !reset_ok {
            k_nano::slog_hal!("USB", "warn", "hub port {} reset TIMEOUT", p);
            continue;
        }
        let _ = xhci::host_ep0_class_nodata(hub_slot, hub_mps, 0x23, 1, 20, p as u16);
        k_nano::tsc::sleep_us(10_000);
        // CRÍTICO: re-ler status após clear C_RESET — buffer antigo mentia a speed (SESSION_314).
        xhci::host_restore_ep0(hub_ep0, hub_slot);
        if !xhci::host_ep0_control_in(hub_slot, hub_mps, 0xA3, 0, 0, p as u16, &mut stbuf) {
            continue;
        }
        let status = u16::from_le_bytes([stbuf[0], stbuf[1]]);
        if status & 1 == 0 {
            k_nano::slog_hal!("USB", "warn", "hub port {} lost CCS pós-reset", p);
            continue;
        }
        // Hub port status: bit9=LS, bit10=HS; senão Full. (USB2.0 11.24.2.7)
        let speed = if status & (1 << 9) != 0 {
            2
        } else if status & (1 << 10) != 0 {
            3
        } else {
            1
        };
        let Some(route) = xhci::push_route(hub_loc.route, p) else {
            continue;
        };
        // U-Boot: TT só LS/FS atrás de hub HS+ (Slot TT_HUB/TT_PORT).
        let need_tt = (speed == 1 || speed == 2) && hub_loc.speed >= 3;
        let child_loc = xhci::DevLoc {
            root_port: hub_loc.root_port,
            route,
            speed,
            parent_slot: hub_slot,
            parent_port: p,
            tt: need_tt,
            // Linux xhci-mem: DEV_MTT no filho LS/FS se hub->tt.multi
            mtt: need_tt && mtt,
        };
        k_nano::slog_hal!(
            "USB",
            "ok",
            "hub port {} pós-reset status={:#x} speed={} tt={} route={:#x}",
            p,
            status,
            speed,
            need_tt as u8,
            route
        );
        let child_slot = match xhci::host_enable_slot(hub_loc.root_port) {
            Some(s) if s > 0 => s,
            _ => continue,
        };
        let child_mps = xhci::ep0_mps_for_speed(speed);
        if !xhci::host_address_device(child_slot, child_loc, child_mps) {
            let _ = xhci::host_disable_slot(child_slot);
            continue;
        }
        k_nano::xhci::mark_hub_address_device(p);
        k_nano::slog_hal!(
            "USB",
            "ok",
            "hub child addressed slot={} hub_port={} route={:#x} tt={}",
            child_slot,
            p,
            route,
            child_loc.tt as u8
        );
        if xhci::host_device_class(child_slot, child_mps) == Some(9) {
            k_nano::slog_hal!("USB", "warn", "nested hub port={} — skip", p);
            let _ = xhci::host_disable_slot(child_slot);
            continue;
        }
        if let Some(msc) = finish_msc(child_slot, child_loc, child_mps) {
            k_nano::slog_hal!(
                "USB",
                "ok",
                "MSC atrás do hub root={} hub_port={} slot={}",
                hub_loc.root_port,
                p,
                msc.slot
            );
            return Some(msc);
        }
        let _ = xhci::host_disable_slot(child_slot);
    }
    k_nano::slog_hal!("USB", "warn", "hub slot={} sem filho MSC", hub_slot);
    None
}
