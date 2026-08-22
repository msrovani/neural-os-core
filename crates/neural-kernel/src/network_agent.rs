//! NetAgent tick — bootstrap de rede (QEMU user/slirp ou bridge TAP via e1000).
//! Sprint Net gate = **e1000** (smoltcp/NIC). SLIP = bypass serial FROZEN — nao e o gate.
//! Smoke labels: `[smoltcp/NIC]`. User: static 10.0.2.15; Bridge: DHCP.
//!
//! Escada (ver tambem `netdiag::run_network_test`):
//!   L0 link → L1 MAC → L2 netstack → L3 static/DHCP → L3.5 RX prove → L4 DNS → L5 HTTP
//! `bootstrap_early` sobe L2–L3.5 (+ smoke L4/L5 se RX ok) no boot, antes do scheduler.

extern crate alloc;
use crate::net::{NETSTACK, NET_CONFIG, TOPIC_NETWORK_CONFIGURED, TOPIC_NET_READY, QemuNetMode};
use crate::netstack::{HttpConn, HttpState};
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use event_bus::{CapabilityToken, Event};
use spin::Mutex;

/// Resultado honesto do smoke `bootstrap_early` (este boot).
/// 0=UNSET 1=L5_OK 2=L5_FAIL 3=L5_PENDING 4=L4_FAIL 5=SKIP 6=L3_5_FAIL
static EARLY_SMOKE: AtomicU8 = AtomicU8::new(0);
static CONTINUOUS_ANNOUNCED: AtomicBool = AtomicBool::new(false);

fn set_early_smoke(v: u8) {
    EARLY_SMOKE.store(v, Ordering::Relaxed);
}

/// Label do smoke deste boot — para gates Hermes/N4 (nao confundir com hist Sprint107 voz L5).
pub fn early_smoke_status() -> &'static str {
    match EARLY_SMOKE.load(Ordering::Relaxed) {
        1 => "L5_OK",
        2 => "L5_FAIL",
        3 => "L5_PENDING",
        4 => "L4_FAIL",
        5 => "SKIP",
        6 => "L3_5_FAIL",
        _ => "UNSET",
    }
}

fn log(tick: u64, msg: &str) {
    k_nano::slog_bin!("Net", "tick", "t={} {}", tick, msg);
}

fn init_netstack(mac: [u8; 6]) {
    let ns = crate::netstack::NetStack::new(mac);
    *NETSTACK.lock() = Some(ns);
}

fn has_ethernet_nic() -> bool {
    crate::net::E1000.lock().is_some() || crate::net::I225.lock().is_some()
        || crate::net::VIRTIO_DEV.lock().is_some()
        || crate::net::RTL8139.lock().is_some()
}

fn apply_static_qemu(ns: &mut crate::netstack::NetStack, tick: u64) {
    // Extrai IP customizado do netmode (se for Static) ou usa 10.0.2.15 padrao
    let custom_ip = match crate::net::detect_qemu_net_mode() {
        crate::net::QemuNetMode::Static(ip) => Some(ip),
        _ => None,
    };
    let ip = custom_ip.unwrap_or([10, 0, 2, 15]);
    ns.set_static_ip(custom_ip);
    log(tick, &alloc::format!("Static IP {}.{}.{}.{}/24 gw={}.{}.{}.1 dns={}.{}.{}.3 (QEMU user/slirp)",
        ip[0], ip[1], ip[2], ip[3], ip[0], ip[1], ip[2], ip[0], ip[1], ip[2]));
    publish_configured();
}

fn publish_configured() {
    let _ = crate::EVENT_BUS.publish(Event {
        id: 0,
        topic: alloc::string::String::from(TOPIC_NETWORK_CONFIGURED),
        payload: b"configured".to_vec(),
        token: CapabilityToken::Legacy(1),
    });
    let _ = crate::EVENT_BUS.publish(Event {
        id: 0,
        topic: alloc::string::String::from(TOPIC_NET_READY),
        payload: b"ready".to_vec(),
        token: CapabilityToken::Legacy(1),
    });
    crate::model_provisioner::maybe_on_net_ready();
}

struct NetState {
    tick: u64,
    phase: u8,
    http: Option<HttpConn>,
    target_ip: [u8; 4],
    dns_tries: u32,
    dev_env_detected: bool,
    static_applied: bool,
    bridge_mode: bool,
}

static NET_STATE: Mutex<NetState> = Mutex::new(NetState {
    tick: 0,
    phase: 0,
    http: None,
    target_ip: [0; 4],
    dns_tries: 0,
    dev_env_detected: false,
    static_applied: false,
    bridge_mode: false,
});

/// L3.5: ARP who-has gw + poll e1000 DD/recv before DNS. Honest FAIL if silent.
fn prove_rx_before_dns(tick: u64) -> bool {
    let (sip, tip) = {
        let cfg = NET_CONFIG.lock();
        let sip = if cfg.ip != [0; 4] { cfg.ip } else { [10, 0, 2, 15] };
        let tip = if cfg.gateway_ip != [0; 4] {
            cfg.gateway_ip
        } else {
            [10, 0, 2, 2]
        };
        (sip, tip)
    };
    let rx_before = crate::netstack::net_rx_count();
    let tx_before = crate::netstack::net_tx_count();
    log(
        tick,
        &alloc::format!(
            "bootstrap_early L3.5: prove RX (ARP {}.{}.{}.{} -> {}.{}.{}.{}) tx={} rx={}",
            sip[0], sip[1], sip[2], sip[3], tip[0], tip[1], tip[2], tip[3], tx_before, rx_before
        ),
    );
    let ok = unsafe { crate::net::prove_e1000_rx(sip, tip) };
    if let Some(ref mut ns) = *NETSTACK.lock() {
        for i in 0..40u64 {
            ns.poll((i * 5) as i64);
            crate::netstack::wall_pause_us(500);
        }
    }
    let rx_after = crate::netstack::net_rx_count();
    let tx_after = crate::netstack::net_tx_count();
    let dtx = tx_after.saturating_sub(tx_before);
    let drx = rx_after.saturating_sub(rx_before);
    if ok || drx > 0 {
        log(
            tick,
            &alloc::format!(
                "bootstrap_early L3.5 OK: RX alive dtx={} drx={} (e1000_ok={})",
                dtx, drx, ok
            ),
        );
        k_nano::slog_bin!(
            "NET-HW",
            "info",
            "VERDICT=PASS reason=rx_alive dtx={} drx={}",
            dtx,
            drx
        );
        true
    } else {
        log(
            tick,
            &alloc::format!(
                "bootstrap_early L3.5 FAIL: RX silent dtx={} drx={} — skip L4/L5 (honest)",
                dtx, drx
            ),
        );
        k_nano::slog_bin!(
            "NET-HW",
            "info",
            "step=lan_rx status=UNSUPPORTED detail=rx_silent_l3_5"
        );
        k_nano::slog_bin!(
            "NET-HW",
            "info",
            "VERDICT=AWAITING_REAL_HW reason=lan_rx_zero_onda7"
        );
        false
    }
}

/// Bootstrap L2–L3.5 no boot (DriverInit), antes do AgentScheduler.
/// Nao inventa DNS/HTTP OK — configura stack + static/DHCP, prova RX, so entao DNS.
pub fn bootstrap_early() {
    let mac = NET_CONFIG.lock().mac;
    if mac == [0; 6] {
        set_early_smoke(5);
        log(0, "bootstrap_early SKIP: MAC zero (L1 fail)");
        return;
    }

    let mut s = NET_STATE.lock();
    if s.static_applied && NETSTACK.lock().as_ref().map_or(false, |ns| ns.dhcp_done) {
        log(0, "bootstrap_early SKIP: already configured");
        return;
    }

    let net_mode = crate::net::detect_qemu_net_mode();
    s.bridge_mode = net_mode == QemuNetMode::Bridge;
    match net_mode {
        QemuNetMode::Bridge => {
            log(0, "bootstrap_early: netmode=BRIDGE (TAP) — DHCP, no static 10.0.2.15")
        }
        QemuNetMode::User => {
            log(0, "bootstrap_early: netmode=USER (slirp) — static 10.0.2.15")
        }
        QemuNetMode::Static(ip) => {
            log(0, &alloc::format!("bootstrap_early: netmode=STATIC {}.{}.{}.{} — mesh P2P", ip[0], ip[1], ip[2], ip[3]))
        }
    }

    if !s.dev_env_detected {
        let is_dev = crate::net::detect_dev_env() || crate::env::is_sandbox();
        NET_CONFIG.lock().is_dev_env = is_dev;
        s.dev_env_detected = true;
        if is_dev {
            log(0, "bootstrap_early: Dev env — L3 + L3.5 RX + DNS/HTTP smoke best-effort");
        } else {
            log(0, "bootstrap_early: HW — netstack only; DHCP fica no NetAgent");
        }
    }

    if NETSTACK.lock().is_none() {
        init_netstack(mac);
        log(
            0,
            &alloc::format!(
                "bootstrap_early L2: NETSTACK init MAC={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
            ),
        );
    }
    s.phase = 1;

    let nic = has_ethernet_nic();
    let is_dev = NET_CONFIG.lock().is_dev_env || crate::env::is_sandbox();

    if !is_dev && !s.bridge_mode {
        set_early_smoke(5);
        log(0, "bootstrap_early L3 deferred (HW DHCP via ticks)");
        return;
    }

    // L3: user/slirp → static; bridge/TAP → DHCP (never force 10.0.2.15 on bridge)
    if s.bridge_mode {
        // Tenta restaurar config DHCP anterior (pula warmup de 27s)
        if hermes_crate::net::restore_dhcp_config() {
            log(0, "bootstrap_early L3: cached DHCP config restored from SGDB, skipping DHCP poll");
            publish_configured();
            s.static_applied = true;
            s.phase = 7;
            return;
        }
        log(0, "bootstrap_early L3: DHCP poll (bridge/TAP)");
        let mut dhcp_ok = false;
        if let Some(ref mut ns) = *NETSTACK.lock() {
            for i in 0..600u64 {
                ns.poll((i * 5) as i64);
                crate::netstack::wall_pause_us(500);
                let (got, gw, dns) = ns.dhcp_poll((i * 5) as i64);
                if got {
                    let mut cfg = NET_CONFIG.lock();
                    cfg.configured = true;
                    cfg.online = true;
                    cfg.gateway_ip = gw;
                    if dns != [0, 0, 0, 0] {
                        cfg.dns_ip = dns;
                    }
                    let (m, i) = (cfg.mac, cfg.ip);
                    drop(cfg);
                    // SESSION_234: sincroniza MAC/IP para o transporte P2P do k_nano (R0).
                    k_nano::net::set_nic_config(m, i);
                    hermes_crate::net::persist_dhcp_config();
                    publish_configured();
                    log(
                        0,
                        &alloc::format!(
                            "bootstrap_early L3 OK DHCP gw={}.{}.{}.{} dns={}.{}.{}.{}",
                            gw[0], gw[1], gw[2], gw[3], dns[0], dns[1], dns[2], dns[3]
                        ),
                    );
                    dhcp_ok = true;
                    break;
                }
            }
        }
        if !dhcp_ok {
            set_early_smoke(6);
            log(0, "bootstrap_early L3 FAIL: DHCP timeout on bridge — skip L4/L5 (honest)");
            s.phase = 99;
            return;
        }
        s.static_applied = true;
    } else if is_dev && !s.static_applied {
        if let Some(ref mut ns) = *NETSTACK.lock() {
            if !ns.dhcp_done {
                apply_static_qemu(ns, 0);
            }
            s.static_applied = true;
            log(
                0,
                &alloc::format!(
                    "bootstrap_early L3 OK (nic={} configured={})",
                    nic,
                    NET_CONFIG.lock().configured
                ),
            );
        }
    }

    if s.http.is_some() || s.phase >= 2 {
        return;
    }
    unsafe {
        crate::net::dump_e1000_status();
    }

    // L3.5: prove RX before DNS
    if nic && !prove_rx_before_dns(0) {
        set_early_smoke(6);
        s.phase = 99;
        return;
    }

    let dns_srv = NET_CONFIG.lock().dns_ip;
    let tx_before = crate::netstack::net_tx_count();
    let rx_before = crate::netstack::net_rx_count();
    s.dns_tries = s.dns_tries.saturating_add(1);
    log(
        0,
        &alloc::format!(
            "bootstrap_early L4: DNS google.com via {}.{}.{}.{} [smoltcp/NIC] tx={} rx={}",
            dns_srv[0], dns_srv[1], dns_srv[2], dns_srv[3],
            tx_before, rx_before
        ),
    );
    let dns_ip = {
        let mut ns_guard = NETSTACK.lock();
        ns_guard.as_mut().and_then(|ns| {
            // QEMU slirp: 10.0.2.3 = DNS; fallback 10.0.2.2 (gateway) se 2.3 falhar
            ns.dns_resolve("google.com", dns_srv)
                .or_else(|| ns.dns_resolve("google.com", [10, 0, 2, 2]))
                .or_else(|| ns.dns_resolve("example.com", dns_srv))
        })
    };
    let tx_after = crate::netstack::net_tx_count();
    let rx_after = crate::netstack::net_rx_count();
    match dns_ip {
        Some(ip) => {
            s.target_ip = ip;
            log(
                0,
                &alloc::format!(
                    "bootstrap_early L4 OK: {}.{}.{}.{} (dtx={} drx={})",
                    ip[0], ip[1], ip[2], ip[3],
                    tx_after.saturating_sub(tx_before),
                    rx_after.saturating_sub(rx_before)
                ),
            );
            if let Some(ref mut ns) = *NETSTACK.lock() {
                let _ = ns.prime_neighbor_for_http();
                let mut conn = ns.http_new(ip, 80, "/");
                log(0, "bootstrap_early L5: HTTP GET / smoke (bounded)");
                let mut done = false;
                for i in 0..4_000u64 {
                    ns.poll((i * 5) as i64);
                    crate::netstack::wall_pause_us(500);
                    ns.http_poll(&mut conn, i * 5);
                    match &conn.state {
                        HttpState::Done(data) => {
                            let text = core::str::from_utf8(data).unwrap_or("<binary>");
                            // Single-line preview (HTTP headers have CR/LF — don't dump raw).
                            let preview: alloc::string::String = text
                                .chars()
                                .take(60)
                                .map(|c| if c == '\r' || c == '\n' { ' ' } else { c })
                                .collect();
                            set_early_smoke(1);
                            log(
                                0,
                                &alloc::format!(
                                    "bootstrap_early L5 OK ({} bytes) tx={} rx={}: {}",
                                    data.len(),
                                    crate::netstack::net_tx_count(),
                                    crate::netstack::net_rx_count(),
                                    preview
                                ),
                            );
                            ns.http_close(&mut conn);
                            s.phase = 99;
                            done = true;
                            // NetFs smoke AFTER drop NETSTACK lock (main.rs) — avoid deadlock
                            break;
                        }
                        HttpState::Failed => {
                            set_early_smoke(2);
                            log(
                                0,
                                &alloc::format!(
                                    "bootstrap_early L5 FAIL: HTTP failed tx={} rx={}",
                                    crate::netstack::net_tx_count(),
                                    crate::netstack::net_rx_count()
                                ),
                            );
                            ns.http_close(&mut conn);
                            s.phase = 99;
                            done = true;
                            break;
                        }
                        _ => {}
                    }
                }
                if !done {
                    // Deixa conexão para NetAgent continuar no scheduler (se sobreviver).
                    set_early_smoke(3);
                    log(
                        0,
                        &alloc::format!(
                            "bootstrap_early L5 PENDING — handoff NetAgent (tx={} rx={})",
                            crate::netstack::net_tx_count(),
                            crate::netstack::net_rx_count()
                        ),
                    );
                    s.http = Some(conn);
                    s.phase = 2;
                }
            }
        }
        None => {
            set_early_smoke(4);
            log(
                0,
                &alloc::format!(
                    "bootstrap_early L4 FAIL: DNS timeout (honest) dtx={} drx={} tx={} rx={}",
                    tx_after.saturating_sub(tx_before),
                    rx_after.saturating_sub(rx_before),
                    tx_after,
                    rx_after
                ),
            );
            // Fallback IP: tenta HTTP breve; se pendente, NetAgent retoma.
            s.target_ip = [142, 250, 190, 14];
            if let Some(ref mut ns) = *NETSTACK.lock() {
                let _ = ns.prime_neighbor_for_http();
                let mut conn = ns.http_new(s.target_ip, 80, "/");
                log(0, "bootstrap_early L5: HTTP via hardcoded IP (DNS failed)");
                let mut done = false;
                for i in 0..4_000u64 {
                    ns.poll((i * 5) as i64);
                    crate::netstack::wall_pause_us(500);
                    ns.http_poll(&mut conn, i * 5);
                    match &conn.state {
                        HttpState::Done(data) => {
                            set_early_smoke(1);
                            log(
                                0,
                                &alloc::format!(
                                    "bootstrap_early L5 OK hardcoded ({} bytes) tx={} rx={}",
                                    data.len(),
                                    crate::netstack::net_tx_count(),
                                    crate::netstack::net_rx_count()
                                ),
                            );
                            ns.http_close(&mut conn);
                            s.phase = 99;
                            done = true;
                            // NetFs smoke AFTER drop NETSTACK lock (main.rs)
                            break;
                        }
                        HttpState::Failed => {
                            set_early_smoke(2);
                            log(
                                0,
                                &alloc::format!(
                                    "bootstrap_early L5 FAIL: HTTP hardcoded failed tx={} rx={}",
                                    crate::netstack::net_tx_count(),
                                    crate::netstack::net_rx_count()
                                ),
                            );
                            ns.http_close(&mut conn);
                            s.phase = 99;
                            done = true;
                            break;
                        }
                        _ => {}
                    }
                }
                if !done {
                    set_early_smoke(3);
                    log(
                        0,
                        &alloc::format!(
                            "bootstrap_early L5 PENDING hardcoded — handoff NetAgent (tx={} rx={})",
                            crate::netstack::net_tx_count(),
                            crate::netstack::net_rx_count()
                        ),
                    );
                    s.http = Some(conn);
                    s.phase = 2;
                }
            }
        }
    }
}

pub fn network_agent_tick() {
    // Labor 11 / ADR-0070: I/O cooperativo sem stallar o tick LLM.
    let _ = hermes_crate::async_io::poll_budget(4);

    let mut s = NET_STATE.lock();
    let tick = s.tick;
    s.tick = tick.wrapping_add(1);
    let ms = tick * 55;

    // Pós SelfHeal/Disk: Continuous entrou no run() — log cedo mesmo se serial for cortado depois.
    // Não bloqueia se Disk ainda emitir; só anuncia que NetAgent está tickando.
    if !CONTINUOUS_ANNOUNCED.swap(true, Ordering::Relaxed) {
        k_nano::slog_hermes!("Net", "info", "Continuous active pós-init (SelfHeal/Disk Done) — gate=e1000 [smoltcp/NIC]");
        // Best-effort NetFs smoke if bootstrap already reached L5_OK.
        drop(s);
        crate::netfs::smoke_if_online();
        s = NET_STATE.lock();
    }
    // Periódico mínimo cedo (sobrevive flood serial de Disk se cortar depois).
    if tick <= 20 || tick % 50 == 0 {
        k_nano::slog_hermes!("Net", "info", "tick {}", tick);
    }
    if tick == 0 || tick == 1 || tick == 2 || tick == 5 || tick == 10 {
        log(tick, &alloc::format!("NetAgent tick started (tick={})", tick));
    }

    // Poll interface
    if let Some(ref mut ns) = *NETSTACK.lock() {
        ns.poll(ms as i64);
        if tick % 50 == 0 {
            log(
                tick,
                &alloc::format!(
                    "poll: tx={} rx={} slip_tx={} slip_rx={} configured={}",
                    crate::netstack::net_tx_count(),
                    crate::netstack::net_rx_count(),
                    crate::slip::slip_tx_count(),
                    crate::slip::slip_rx_count(),
                    NET_CONFIG.lock().configured
                ),
            );
        }
        if tick % 100 == 0 {
            unsafe {
                crate::net::dump_e1000_status();
            }
        }
        if let Some(ref mut c) = s.http {
            ns.http_poll(c, ms as u64);
            match &c.state {
                HttpState::Done(data) => {
                    let text = core::str::from_utf8(data).unwrap_or("<binary>");
                    log(
                        tick,
                        &alloc::format!("HTTP OK ({} bytes): {}", data.len(), text.trim_end()),
                    );
                    ns.http_close(c);
                    s.http = None;
                    s.phase = 99;
                }
                HttpState::Failed => {
                    log(tick, "HTTP failed");
                    ns.http_close(c);
                    s.http = None;
                    s.phase = 99;
                }
                _ => {}
            }
        }
    } else if tick % 10 == 0 {
        log(tick, "NETSTACK not initialized");
    }

    match s.phase {
        // Phase 0: init netstack + detect env (imediato se MAC já existe)
        0 => {
            let mac = NET_CONFIG.lock().mac;
            if mac != [0; 6] {
                init_netstack(mac);
                if !s.dev_env_detected {
                    let is_dev = crate::net::detect_dev_env();
                    NET_CONFIG.lock().is_dev_env = is_dev;
                    s.dev_env_detected = true;
                    if is_dev {
                        log(tick, "Dev env (QEMU/VBox) — static IP early + DHCP best-effort");
                    } else {
                        log(tick, "HW real — DHCP primary");
                    }
                }
                s.phase = 1;
            }
        }
        // Phase 1: configure → DNS → HTTP smoke
        1 => {
            let nic = has_ethernet_nic();
            let is_dev = NET_CONFIG.lock().is_dev_env || crate::env::is_sandbox();
            if !s.dev_env_detected {
                s.bridge_mode = crate::net::detect_qemu_net_mode() == QemuNetMode::Bridge;
            }
            let bridge = s.bridge_mode;

            // User/slirp only: static 10.0.2.15. Bridge/TAP: DHCP (never force slirp IP).
            if nic && is_dev && !bridge && !s.static_applied {
                if let Some(ref mut ns) = *NETSTACK.lock() {
                    if !ns.dhcp_done {
                        apply_static_qemu(ns, tick);
                    }
                    s.static_applied = true;
                }
            }

            // DHCP poll — HW / bridge primary; user sandbox may upgrade if OFFER arrives
            if nic {
                let dhcp_got = {
                    let mut ns_guard = NETSTACK.lock();
                    if let Some(ref mut ns) = *ns_guard {
                        // Apos static em user/slirp, dhcp_done ja true — nao reentrar.
                        // Em HW/bridge: poll ate Configured.
                        if !ns.dhcp_done {
                            let (got, gw, dns) = ns.dhcp_poll(ms as i64);
                            if tick % 50 == 0 {
                                log(
                                    tick,
                                    &alloc::format!(
                                        "DHCP poll: got={} tx={} rx={}",
                                        got,
                                        crate::netstack::net_tx_count(),
                                        crate::netstack::net_rx_count()
                                    ),
                                );
                            }
                            if got {
                                let mut cfg = NET_CONFIG.lock();
                                cfg.configured = true;
                                cfg.online = true;
                                cfg.gateway_ip = gw;
                                if dns != [0, 0, 0, 0] {
                                    cfg.dns_ip = dns;
                                } else if is_dev && !bridge {
                                    cfg.dns_ip = [10, 0, 2, 3];
                                }
                                if cfg.ip == [0, 0, 0, 0] && is_dev && !bridge {
                                    cfg.ip = [10, 0, 2, 15];
                                }
                                let (m, i) = (cfg.mac, cfg.ip);
                                drop(cfg);
                                // SESSION_234: sincroniza MAC/IP para o transporte P2P do k_nano (R0).
                                k_nano::net::set_nic_config(m, i);
                                hermes_crate::net::persist_dhcp_config();
                                log(
                                    tick,
                                    &alloc::format!(
                                        "DHCP OK! gw={}.{}.{}.{} dns={}.{}.{}.{}",
                                        gw[0], gw[1], gw[2], gw[3],
                                        dns[0], dns[1], dns[2], dns[3]
                                    ),
                                );
                                publish_configured();
                            }
                        }
                        ns.dhcp_done
                    } else {
                        false
                    }
                };
                // HW: fallback static só após ~30s
                if !is_dev && !dhcp_got && !s.static_applied && tick >= 600 {
                    if let Some(ref mut ns) = *NETSTACK.lock() {
                        apply_static_qemu(ns, tick);
                        s.static_applied = true;
                        log(tick, "HW DHCP timeout — static fallback (may be wrong for LAN)");
                    }
                }
            } else if !s.static_applied && tick >= 2 {
                // Sem NIC Ethernet: serial tunnel — ainda seta stack local para apps
                if let Some(ref mut ns) = *NETSTACK.lock() {
                    apply_static_qemu(ns, tick);
                    s.static_applied = true;
                    log(tick, "No Ethernet NIC — static + SLIP fallback path");
                }
            }

            // Espera configuração
            let ready = NETSTACK
                .lock()
                .as_ref()
                .map_or(false, |ns| ns.dhcp_done);
            if !ready {
                return;
            }

            // DNS + HTTP via smoltcp (mesmo medium da NIC) — NÃO dns_resolve_manual/slip
            // Threshold baixo: hang pós-Runtime pode matar o scheduler cedo.
            if tick >= 2 && s.http.is_none() && s.dns_tries < 4 {
                if let Some(ref mut ns) = *NETSTACK.lock() {
                    s.dns_tries += 1;
                    let dns_srv = NET_CONFIG.lock().dns_ip;
                    log(
                        tick,
                        &alloc::format!(
                            "DNS resolve google.com (try {}) via {}.{}.{}.{} [smoltcp/NIC]",
                            s.dns_tries, dns_srv[0], dns_srv[1], dns_srv[2], dns_srv[3]
                        ),
                    );
                    if let Some(ip) = ns.dns_resolve("google.com", dns_srv) {
                        s.target_ip = ip;
                        log(
                            tick,
                            &alloc::format!(
                                "DNS OK: {}.{}.{}.{}",
                                ip[0], ip[1], ip[2], ip[3]
                            ),
                        );
                        s.http = Some(ns.http_new(ip, 80, "/"));
                        s.phase = 2;
                    } else {
                        log(tick, "DNS timeout");
                    }
                }
            }
            // Fallback: IP público conhecido (Google) se DNS falhar
            if s.dns_tries >= 4 && s.http.is_none() {
                log(tick, "DNS exhausted — HTTP smoke via hardcoded IP");
                s.target_ip = [142, 250, 190, 14];
                if let Some(ref mut ns) = *NETSTACK.lock() {
                    s.http = Some(ns.http_new(s.target_ip, 80, "/"));
                    s.phase = 2;
                }
            }
        }
        // Health
        _ => {
            if tick % 200 == 0 && NET_CONFIG.lock().configured {
                log(
                    tick,
                    &alloc::format!(
                        "Health TX={} RX={} online={}",
                        crate::netstack::net_tx_count(),
                        crate::netstack::net_rx_count(),
                        NET_CONFIG.lock().online
                    ),
                );
            }
            if tick == 300 || tick == 500 {
                let report = crate::netdiag::run_network_test();
                for line in report.lines() {
                    k_nano::slog_bin!("Log", "msg", "{}", line);
                }
            }
        }
    }

    // ─── P2P Mesh broadcast (ADR-0081) — chamado do BEI tick hook ───
    // (bei_init.rs) para rodar a cada scheduler tick.
}

// Transporte P2P movido para k_nano (SESSION_234, ADR-0081): o bin agora
// chama `k_nano::net::mesh::p2p_tick()` no bei_tick hook. Não-heartbeats são
// publicados no EVENT_BUS ("P2P_PACKET") e consumidos via
// `hermes_crate::skill_sync::poll_p2p()` / `skill_marketplace::poll_p2p()`.
