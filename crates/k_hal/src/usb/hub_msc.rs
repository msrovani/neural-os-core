//! MSC bring-up com hub interno (route string + TT).
//!
//! Referências: Redox xhcid/usbhubd; Chitti `enumerate_hub` (Mac mini USB-A
//! atrás de hub). Sem isto, Alienware chega ao desktop (Limine leu ESP) mas
//! `BOOT.LOG`/`NSGDB` nunca gravam — stick não está em root CCS.
//!
//! SESSION_345 F3 attack (2026-09-14): metal ccs=4 + MSC fail + UI bloqueava
//! retry → placeholder no stick. Aqui: budget metal maior, **hub-first**,
//! breadcrumbs no FB/ramlog, não skip permanente em abort por budget.

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

fn is_metal() -> bool {
    k_nano::platform_probe::probe_done()
        && matches!(
            k_nano::platform_probe::hypervisor(),
            k_nano::platform_probe::HypervisorKind::None
        )
}

fn fb_usb(msg: &str) {
    // Metal sem COM1: FB + ramlog (slog sozinho some no dump das 1ªs linhas).
    k_nano::boot_ramlog::append(msg);
    k_nano::display::fb::boot_ckpt_noflush(191, msg);
}

/// Entry R1 registrada em `k_nano::xhci::register_msc_bringup`.
pub unsafe fn bringup_boot_msc() -> Option<MscDevice> {
    // SESSION_345: metal pós-Limine — stick USB3 atrasa CCS; 8s no deferred.
    let secs: u64 = if is_metal() {
        if k_nano::boot_logger::ui_is_live() {
            8
        } else {
            12
        }
    } else {
        3
    };
    let hz = k_nano::tsc::tsc_hz();
    let t0 = k_nano::tsc::rdtsc();
    if hz > 1_000_000 {
        MSC_TSC_DEADLINE.store(t0.wrapping_add(hz.saturating_mul(secs)), Ordering::Relaxed);
    } else {
        MSC_TSC_DEADLINE.store(0, Ordering::Relaxed);
    }

    let max_ports = match xhci::host_max_ports() {
        Some(m) => m,
        None => {
            xhci::mark_msc_xhci_down();
            fb_usb("USB: MSC xhci-down (sem controller)");
            return None;
        }
    };
    let mut ccs: alloc::vec::Vec<(u8, u8)> = alloc::vec::Vec::new();
    for port in 1..=max_ports {
        if xhci::msc_port_skipped(port) {
            continue;
        }
        if let Some((speed, portsc)) = xhci::host_port_ccs(port) {
            k_nano::slog_hal_home!(
                "USB",
                "ok",
                "k_hal::usb::hub_msc",
                "porta {} CCS speed={} PORTSC={:#x}",
                port,
                speed,
                portsc
            );
            ccs.push((port, speed));
        }
    }
    if ccs.is_empty() {
        k_nano::slog_hal_home!(
            "USB",
            "warn",
            "k_hal::usb::hub_msc",
            "ccs=0, tentativa hub-first (stick atrás de hub interno?)"
        );
        fb_usb("USB: ccs=0, tentativa hub-first");
        if !msc_budget_ok() {
            fb_usb("USB: MSC budget abort (ccs=0)");
            MSC_TSC_DEADLINE.store(0, Ordering::Relaxed);
            return None;
        }
        for port in 1..=max_ports {
            if !msc_budget_ok() {
                fb_usb("USB: MSC budget abort (hub-first)");
                MSC_TSC_DEADLINE.store(0, Ordering::Relaxed);
                return None;
            }
            if xhci::msc_port_skipped(port) {
                continue;
            }
            // M1 higiene anti-destruição: sem CCS e sem sticky de change (CSC)
            // a porta nunca teve história elétrica neste boot — reset aqui é
            // destruição (evidência HW: PORTSC 0x2a0 presa em RxDetect).
            // CCS=1 ou CSC=1: comportamento idêntico ao anterior.
            if unsafe { xhci::host_port_ccs(port) }.is_none()
                && !unsafe { xhci::port_csc_sticky(port) }
            {
                k_nano::slog_hal_home!(
                    "USB",
                    "warn",
                    "k_hal::usb::hub_msc",
                    "skip P{} sem história (sem CCS/CSC) — sem reset",
                    port
                );
                fb_usb(&alloc::format!("USB: skip P{} sem história", port));
                continue;
            }
            match classify_root_port(port, 0) {
                RootClass::Msc(dev) => {
                    MSC_TSC_DEADLINE.store(0, Ordering::Relaxed);
                    fb_usb(&alloc::format!(
                        "USB: MSC OK hub-first root P{} slot={}",
                        dev.port,
                        dev.slot
                    ));
                    return Some(dev);
                }
                RootClass::Hub { slot, mps } => {
                    if !msc_budget_ok() {
                        let _ = xhci::host_disable_slot(slot);
                        MSC_TSC_DEADLINE.store(0, Ordering::Relaxed);
                        return None;
                    }
                    let hub_loc = xhci::DevLoc::root(port, 0);
                    match try_msc_behind_hub(slot, hub_loc, mps) {
                        Some(dev) => {
                            MSC_TSC_DEADLINE.store(0, Ordering::Relaxed);
                            fb_usb(&alloc::format!(
                                "USB: MSC OK hub-first P{} child slot={}",
                                port,
                                dev.slot
                            ));
                            return Some(dev);
                        }
                        None => {
                            let _ = xhci::host_disable_slot(slot);
                        }
                    }
                }
                RootClass::Other | RootClass::Fail => {}
            }
        }
        k_nano::slog_hal_home!(
            "USB",
            "warn",
            "k_hal::usb::hub_msc",
            "hub-first sem MSC — retry deferred"
        );
        fb_usb("USB: hub-first sem MSC (retry)");
        return None;
    }
    // SuperSpeed primeiro nas roots; hubs (class 9) tratados em pass 1 abaixo.
    ccs.sort_by(|a, b| b.1.cmp(&a.1));

    fb_usb(&alloc::format!(
        "USB: MSC scan ccs={} budget={}s metal={}",
        ccs.len(),
        secs,
        is_metal() as u8
    ));

    // Pass 1: classificar — MSC root OK imediato; hubs enfileirados; resto skip.
    let mut hubs: alloc::vec::Vec<(u8, u8, u8, u16)> = alloc::vec::Vec::new(); // port,speed,slot,mps
    for (port, speed) in ccs.iter().copied() {
        if !msc_budget_ok() {
            k_nano::slog_hal!(
                "USB",
                "warn",
                "MSC bringup budget — abort classify (UI first; retry)"
            );
            fb_usb("USB: MSC budget abort (classify)");
            // NÃO mark_failed — retry DriverInit/deferred deve rever estas portas.
            break;
        }
        fb_usb(&alloc::format!("USB: try root P{} speed={}", port, speed));
        match classify_root_port(port, speed) {
            RootClass::Msc(dev) => {
                MSC_TSC_DEADLINE.store(0, Ordering::Relaxed);
                fb_usb(&alloc::format!(
                    "USB: MSC OK root P{} slot={}",
                    dev.port,
                    dev.slot
                ));
                return Some(dev);
            }
            RootClass::Hub {
                slot,
                mps,
            } => {
                hubs.push((port, speed, slot, mps));
            }
            // M1: Other ja marcou done dentro do classify (definitivo);
            // Fail = transiente, sem mark (retry no proximo ciclo).
            RootClass::Other | RootClass::Fail => {}
        }
    }

    // Pass 2: MSC atrás dos hubs (Alienware USB-A típico).
    for (port, speed, hub_slot, hub_mps) in hubs {
        if !msc_budget_ok() {
            fb_usb("USB: MSC budget abort (hub pass)");
            // Libera slots de hub sem marcar porta failed (budget).
            let _ = xhci::host_disable_slot(hub_slot);
            break;
        }
        fb_usb(&alloc::format!("USB: hub enum root P{} slot={}", port, hub_slot));
        let hub_loc = xhci::DevLoc::root(port, speed);
        match try_msc_behind_hub(hub_slot, hub_loc, hub_mps) {
            Some(dev) => {
                MSC_TSC_DEADLINE.store(0, Ordering::Relaxed);
                fb_usb(&alloc::format!(
                    "USB: MSC OK hub P{} child slot={}",
                    port,
                    dev.slot
                ));
                return Some(dev);
            }
            None => {
                let _ = xhci::host_disable_slot(hub_slot);
                // M1 definitivo: hub totalmente enumerado, sem filho MSC.
                xhci::mark_msc_port_done(port, "hub-no-msc");
                fb_usb(&alloc::format!("USB: done P{} hub-no-msc", port));
                k_nano::slog_hal!(
                    "USB",
                    "warn",
                    "MSC bringup FAIL hub root={} — tenta proxima",
                    port
                );
            }
        }
    }

    k_nano::slog_hal!("USB", "warn", "MSC bringup FAIL em todas as portas CCS");
    fb_usb("USB: MSC FAIL all CCS/hubs");
    MSC_TSC_DEADLINE.store(0, Ordering::Relaxed);
    None
}

enum RootClass {
    Msc(MscDevice),
    Hub { slot: u8, mps: u16 },
    Other,
    Fail,
}

/// M1+M2: wrapper twin-aware. A gemea entra UMA vez, SO se o reset da face
/// tentada FALHAR e a gemea tiver historia eletrica (CCS=1 ou CSC sticky),
/// nunca done-stick, sempre com budget. Heuristica 1:1 por ordinal.
unsafe fn classify_root_port(port: u8, speed: u8) -> RootClass {
    match classify_root_port_once(port, speed) {
        RootClass::Fail => {
            if let Some((twin, ord)) = xhci::companion_twin(port) {
                if !xhci::msc_port_skipped(twin) && xhci::port_has_history(twin) {
                    if !msc_budget_ok() {
                        k_nano::slog_hal!(
                            "USB",
                            "warn",
                            "twin P{}/{} ord={} budget-abort",
                            port,
                            twin,
                            ord
                        );
                        fb_usb(&alloc::format!("USB: twin P{}/{} no-budget", port, twin));
                        return RootClass::Fail;
                    }
                    fb_usb(&alloc::format!("USB: twin P{}/{} ord={}", port, twin, ord));
                    k_nano::slog_hal!(
                        "USB",
                        "warn",
                        "twin P{}/{} ord={} (heuristic) - 1 try",
                        port,
                        twin,
                        ord
                    );
                    let ts = xhci::host_port_ccs(twin).map(|(s, _)| s).unwrap_or(0);
                    return classify_root_port_once(twin, ts);
                }
                k_nano::slog_hal!(
                    "USB",
                    "warn",
                    "twin P{}: gemea P{} sem-historia/done - sem try",
                    port,
                    twin
                );
            }
            RootClass::Fail
        }
        other => other,
    }
}

unsafe fn classify_root_port_once(port: u8, speed: u8) -> RootClass {
    if !xhci::host_reset_port(port, speed) {
        k_nano::slog_hal!("USB", "warn", "port {} reset FAIL", port);
        k_nano::slog_hal!("USB", "warn", "retry P{} reset", port);
        fb_usb(&alloc::format!("USB: retry P{} reset", port));
        return RootClass::Fail;
    }
    let loc = xhci::DevLoc::root(port, speed);
    let slot = match xhci::host_enable_slot(port) {
        Some(s) if s > 0 => s,
        _ => {
            k_nano::slog_hal!("USB", "warn", "Enable Slot FAIL port={}", port);
            k_nano::slog_hal!("USB", "warn", "retry P{} slot", port);
            fb_usb(&alloc::format!("USB: retry P{} slot", port));
            return RootClass::Fail;
        }
    };
    let mps = xhci::ep0_mps_for_speed(speed);
    if !xhci::host_address_device(slot, loc, mps) {
        k_nano::slog_hal!(
            "USB",
            "warn",
            "Address Device FAIL slot={} port={}",
            slot,
            port
        );
        let _ = xhci::host_disable_slot(slot);
        k_nano::slog_hal!("USB", "warn", "retry P{} addr", port);
        fb_usb(&alloc::format!("USB: retry P{} addr", port));
        return RootClass::Fail;
    }
    crate::unlock_dag::grant(crate::unlock_dag::CapToken::UsbEp0);

    match xhci::host_device_class(slot, mps) {
        Some(9) => {
            k_nano::slog_hal!(
                "USB",
                "ok",
                "hub class @ root port={} — defer hub enum",
                port
            );
            crate::unlock_dag::grant(crate::unlock_dag::CapToken::UsbHubOk);
            RootClass::Hub { slot, mps }
        }
        _ => match finish_msc(slot, loc, mps) {
            MscFinish::Dev(dev) => RootClass::Msc(dev),
            MscFinish::NonMsc => {
                let _ = xhci::host_disable_slot(slot);
                // M1 definitivo: classe lida OK e != MSC (ex: camera/BT).
                xhci::mark_msc_port_done(port, "non-msc");
                fb_usb(&alloc::format!("USB: done P{} non-msc", port));
                RootClass::Other
            }
            MscFinish::Fail(why) => {
                let _ = xhci::host_disable_slot(slot);
                k_nano::slog_hal!("USB", "warn", "retry P{} {}", port, why);
                fb_usb(&alloc::format!("USB: retry P{} {}", port, why));
                RootClass::Fail
            }
        },
    }
}

/// M1: veredito do finish MSC. NonMsc (desc lido OK, sem interface BOT) e
/// DEFINITIVO (done); Fail (transporte: EP0/config) e retryable.
enum MscFinish {
    Dev(MscDevice),
    NonMsc,
    Fail(&'static str),
}

unsafe fn finish_msc(slot: u8, loc: xhci::DevLoc, ep0_mps: u16) -> MscFinish {
    let mut cfg = [0u8; 512];
    if !xhci::host_ep0_control_in(slot, ep0_mps, 0x80, 0x06, 0x0200, 0, &mut cfg) {
        return MscFinish::Fail("ep0");
    }
    let msc_eps = xhci::parse_msc_config(&cfg);
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
            return MscFinish::NonMsc;
        }
    };
    let _ = xhci::host_set_configuration(slot, ep0_mps, cfg_val);
    let Some((ep_in_be, ep_out_be)) =
        xhci::host_configure_msc(slot, loc, bulk_mps, ep_in, ep_out)
    else {
        let _ = xhci::host_disable_slot(slot);
        return MscFinish::Fail("cfg");
    };
    xhci::host_set_msc_port(loc.root_port);
    MscFinish::Dev(MscDevice {
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
            fb_usb(&alloc::format!("USB: hub mid-budget abort @{p}"));
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
        xhci::host_restore_ep0(hub_ep0, hub_slot);
        if !xhci::host_ep0_control_in(hub_slot, hub_mps, 0xA3, 0, 0, p as u16, &mut stbuf) {
            continue;
        }
        let status = u16::from_le_bytes([stbuf[0], stbuf[1]]);
        if status & 1 == 0 {
            k_nano::slog_hal!("USB", "warn", "hub port {} lost CCS pós-reset", p);
            continue;
        }
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
        let need_tt = (speed == 1 || speed == 2) && hub_loc.speed >= 3;
        let child_loc = xhci::DevLoc {
            root_port: hub_loc.root_port,
            route,
            speed,
            parent_slot: hub_slot,
            parent_port: p,
            tt: need_tt,
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
        // M1: filho hub nao marca done (o done e da root, apos enum exaurido);
        // NonMsc aqui = so skip deste filho; Fail = retry no proximo ciclo.
        match finish_msc(child_slot, child_loc, child_mps) {
            MscFinish::Dev(msc) => {
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
            MscFinish::NonMsc => {
                k_nano::slog_hal!("USB", "warn", "hub filho P{} non-msc — skip filho", p);
                let _ = xhci::host_disable_slot(child_slot);
            }
            MscFinish::Fail(why) => {
                k_nano::slog_hal!("USB", "warn", "hub filho P{} retry {}", p, why);
                let _ = xhci::host_disable_slot(child_slot);
            }
        }
    }
    k_nano::slog_hal!("USB", "warn", "hub slot={} sem filho MSC", hub_slot);
    None
}
