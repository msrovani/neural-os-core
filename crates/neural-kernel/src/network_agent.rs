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
}

static NET_STATE: Mutex<NetState> = Mutex::new(NetState { tick: 0, phase: 0, http: None, target_ip: [0; 4], dns_tries: 0 });

/// Non-async tick function for NetAgent — uses persistent internal state
pub fn network_agent_tick() {
    let mut s = NET_STATE.lock();
    let tick = s.tick;
    s.tick = tick.wrapping_add(1);

    if let Some(ref mut ns) = *NETSTACK.lock() {
        ns.poll(tick as i64);

        // Phase 0.5: DHCP auto-config (poll each tick until done)
        if s.phase == 0 && !ns.dhcp_done {
            let (done, gw, dns_srv) = ns.dhcp_poll(tick as i64);
            if done {
                NET_CONFIG.lock().gateway_ip = gw;
                NET_CONFIG.lock().dns_ip = dns_srv;
                NET_CONFIG.lock().configured = true;
                NET_CONFIG.lock().online = true;
                log(tick, &alloc::format!("DHCP OK: gw={}.{}.{}.{} dns={}.{}.{}.{}",
                    gw[0], gw[1], gw[2], gw[3],
                    dns_srv[0], dns_srv[1], dns_srv[2], dns_srv[3]));
                s.phase = 1;
            }
        }

        // HTTP polling (phase 2+)
        if let Some(ref mut c) = s.http {
            ns.http_poll(c, tick);
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
    }

    // Timeout: if DHCP doesn't complete within 200 ticks, use static IP
    if s.phase == 0 && tick > 200 && !s.http.is_some() {
        log(tick, "DHCP timeout. Using static IP 10.0.2.15");
        let mac = NET_CONFIG.lock().mac;
        if mac != [0; 6] {
            if NETSTACK.lock().is_none() {
                init_netstack(mac);
            }
            NET_CONFIG.lock().configured = true;
            NET_CONFIG.lock().online = true;
            s.phase = 1;
        }
    }

    match s.phase {
        0 => {
            // Init netstack if NIC ready
            if tick >= 10 && NET_CONFIG.lock().mac != [0; 6] {
                init_netstack(NET_CONFIG.lock().mac);
            }
        }
        1 => {
            // DNS query after DHCP (up to 3 tries)
            if tick >= 20 && !s.http.is_some() && s.dns_tries < 3 {
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
        _ => {
            if tick % 200 == 0 && NET_CONFIG.lock().configured {
                log(tick, "Health");
            }
        }
    }
}
