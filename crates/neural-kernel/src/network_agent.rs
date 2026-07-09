extern crate alloc;
use crate::net::{NETSTACK, NET_CONFIG};
use crate::netstack::{HttpConn, HttpState};
use crate::serial_println;
use spin::Mutex;

fn log(tick: u64, msg: &str) {
    serial_println!("[NET @t={}] {}", tick, msg);
}

fn init_netstack(mac: [u8; 6]) {
    let ns = crate::netstack::NetStack::new(mac);
    *NETSTACK.lock() = Some(ns);
}

struct NetState {
    tick: u64,
    phase: u8,
    http: Option<HttpConn>,
    target_ip: [u8; 4],
    dns_tries: u32,
    dev_env_detected: bool,
}

static NET_STATE: Mutex<NetState> = Mutex::new(NetState { tick: 0, phase: 0, http: None, target_ip: [0; 4], dns_tries: 0, dev_env_detected: false });

pub fn network_agent_tick() {
    let mut s = NET_STATE.lock();
    let tick = s.tick;
    s.tick = tick.wrapping_add(1);
    let ms = tick * 55;

    // Debug inicial - mais frequente
    if tick == 0 || tick == 1 || tick == 2 || tick == 5 || tick == 10 {
        log(tick, &alloc::format!("NetAgent tick started (tick={})", tick));
    }

    // Poll interface
    if let Some(ref mut ns) = *NETSTACK.lock() {
        ns.poll(ms as i64);
        if tick % 50 == 0 {
            log(tick, &alloc::format!("poll: tx={} rx={}",
                crate::netstack::net_tx_count(), crate::netstack::net_rx_count()));
        }
        if tick % 100 == 0 {
            unsafe { crate::net::dump_e1000_status(); }
        }
        if let Some(ref mut c) = s.http {
            ns.http_poll(c, ms as u64);
            match &c.state {
                HttpState::Done(data) => {
                    let text = core::str::from_utf8(data).unwrap_or("<binary>");
                    log(tick, &alloc::format!("HTTP OK ({} bytes): {}", data.len(), text.trim_end()));
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
    } else {
        if tick % 10 == 0 {
            log(tick, "NETSTACK not initialized");
        }
    }

    match s.phase {
        // Phase 0: init netstack + detect dev env
        0 => {
            if tick >= 10 {
                let mac = NET_CONFIG.lock().mac;
                if mac != [0; 6] {
                    init_netstack(mac);
                    // Detect dev environment (QEMU/VBox vs HW real)
                    if !s.dev_env_detected {
                        let is_dev = crate::net::detect_dev_env();
                        NET_CONFIG.lock().is_dev_env = is_dev;
                        s.dev_env_detected = true;
                        if is_dev {
                            log(tick, "Dev environment detected (QEMU/VBox) - will use static IP");
                        } else {
                            log(tick, "HW real detected - will use DHCP");
                        }
                    }
                    s.phase = 1;
                }
            }
        }
        // Phase 1: DHCP (HW real) or static IP (dev env) → DNS → HTTP
        1 => {
            let is_dev = NET_CONFIG.lock().is_dev_env;
            
            // Dev environment: use static IP immediately
            if is_dev {
                if !NETSTACK.lock().as_ref().map_or(false, |ns| ns.dhcp_done) {
                    if let Some(ref mut ns) = *NETSTACK.lock() {
                        ns.set_static_ip();
                        NET_CONFIG.lock().configured = true;
                        NET_CONFIG.lock().online = true;
                        // Serial tunnel: DNS precisa ser real (8.8.8.8), nao 10.0.2.3
                        if crate::env::is_sandbox() {
                            NET_CONFIG.lock().dns_ip = [8, 8, 8, 8];
                            NET_CONFIG.lock().gateway_ip = [8, 8, 8, 8];
                            log(tick, "Sandbox serial: DNS set to 8.8.8.8");
                        }
                        log(tick, "Dev env: using static IP 10.0.2.15/24");
                    }
                }
            } else {
                // HW real: try DHCP first
                let dhcp_done = {
                    let mut ns_guard = NETSTACK.lock();
                    if let Some(ref mut ns) = *ns_guard {
                        if !ns.dhcp_done {
                            let (got, gw, dns) = ns.dhcp_poll(ms as i64);
                            if tick % 50 == 0 {
                                log(tick, &alloc::format!("DHCP poll: got={} tx={} rx={}",
                                    got, crate::netstack::net_tx_count(), crate::netstack::net_rx_count()));
                            }
                            if got {
                                NET_CONFIG.lock().configured = true;
                                NET_CONFIG.lock().online = true;
                                NET_CONFIG.lock().dns_ip = dns;
                                NET_CONFIG.lock().gateway_ip = gw;
                                log(tick, &alloc::format!("DHCP OK. gw={}.{}.{}.{} dns={}.{}.{}.{}",
                                    gw[0], gw[1], gw[2], gw[3],
                                    dns[0], dns[1], dns[2], dns[3]));
                            }
                        }
                        ns.dhcp_done
                    } else { false }
                };
                // Fallback: static IP se DHCP timeout (~30s = 600 ticks)
                if !dhcp_done && tick >= 600 {
                    if let Some(ref mut ns) = *NETSTACK.lock() {
                        ns.set_static_ip();
                        NET_CONFIG.lock().configured = true;
                        NET_CONFIG.lock().online = true;
                        log(tick, "DHCP timeout, using static IP 10.0.2.15/24");
                    }
                }
            }
            
            // Se DHCP (ou static fallback) ainda nao configurou, espera
            if !NETSTACK.lock().as_ref().map_or(false, |ns| ns.dhcp_done) { return; }
            if tick >= 40 && !s.http.is_some() && s.dns_tries < 3 {
                if let Some(ref mut ns) = *NETSTACK.lock() {
                    s.dns_tries += 1;
                    let dns_srv = NET_CONFIG.lock().dns_ip;
                    log(tick, &alloc::format!("DNS resolve google.com (try {}) via {}.{}.{}.{}",
                        s.dns_tries, dns_srv[0], dns_srv[1], dns_srv[2], dns_srv[3]));
                    if let Some(ip) = ns.dns_resolve("google.com", dns_srv) {
                        s.target_ip = ip;
                        log(tick, &alloc::format!("DNS OK: {}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3]));
                        s.http = Some(ns.http_new(ip, 80, "/"));
                        s.phase = 2;
                    } else {
                        log(tick, "DNS timeout");
                    }
                }
            }
            // Fallback: hardcoded IP after 3 fails
            if s.dns_tries >= 3 && !s.http.is_some() {
                log(tick, "DNS exhausted, using fallback IP");
                s.target_ip = [142, 250, 80, 110];
                if let Some(ref mut ns) = *NETSTACK.lock() {
                    s.http = Some(ns.http_new(s.target_ip, 80, "/"));
                    s.phase = 2;
                }
            }
        }
        // Health + resultados
        _ => {
            if tick % 200 == 0 && NET_CONFIG.lock().configured {
                log(tick, &alloc::format!("Health TX={} RX={}",
                    crate::netstack::net_tx_count(), crate::netstack::net_rx_count()));
            }
            // Mostra diagnostico completo quando HTTP termina
            if tick == 300 || tick == 500 {
                let report = crate::netdiag::run_network_test();
                for line in report.lines() {
                    serial_println!("{}", line);
                }
            }
        }
    }
}
