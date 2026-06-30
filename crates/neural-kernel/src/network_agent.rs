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

pub fn network_agent_tick() {
    let mut s = NET_STATE.lock();
    let tick = s.tick;
    s.tick = tick.wrapping_add(1);
    let ms = tick * 55;

    // Poll interface
    if let Some(ref mut ns) = *NETSTACK.lock() {
        ns.poll(ms as i64);
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
    }

    match s.phase {
        // Phase 0: init netstack + apply static IP when MAC is known
        0 => {
            if tick >= 10 {
                let mac = NET_CONFIG.lock().mac;
                if mac != [0; 6] {
                    init_netstack(mac);
                    // Apply static IP on next tick (phase 1)
                    s.phase = 1;
                }
            }
        }
        // Phase 1: set static IP + DNS/HTTP
        1 => {
            // Set static IP once
            if let Some(ref mut ns) = *NETSTACK.lock() {
                if !ns.has_static_ip {
                    ns.set_static_ip();
                    NET_CONFIG.lock().configured = true;
                    NET_CONFIG.lock().online = true;
                    log(tick, "Static IP: 10.0.2.15/24 gw=10.0.2.2");
                }
            }
            // DNS
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
        // Health
        _ => {
            if tick % 200 == 0 && NET_CONFIG.lock().configured {
                log(tick, "Health");
            }
        }
    }
}
