extern crate alloc;
use crate::net::{NETSTACK, NET_CONFIG};
use crate::netstack::{HttpConn, HttpState};
use crate::serial_println;
use spin::Mutex;

const GW_MAC: [u8; 6] = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];

fn log(tick: u64, msg: &str) {
    serial_println!("[NET @t={}] {}", tick, msg);
}

fn init_netstack(mac: [u8; 6]) {
    let mut ns = crate::netstack::NetStack::new(mac);
    ns.set_ip([10, 0, 2, 15]);
    *NETSTACK.lock() = Some(ns);
}

struct NetState {
    tick: u64,
    phase: u8,
    http: Option<HttpConn>,
    target_ip: [u8; 4],
}

static NET_STATE: Mutex<NetState> = Mutex::new(NetState { tick: 0, phase: 0, http: None, target_ip: [0; 4] });

/// Non-async tick function for NetAgent — uses persistent internal state
pub fn network_agent_tick() {
    let mut s = NET_STATE.lock();
    let tick = s.tick;
    s.tick = tick.wrapping_add(1);

    if let Some(ref mut ns) = *NETSTACK.lock() {
        ns.poll(tick as i64);

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

    match s.phase {
        0 => {
            if NETSTACK.lock().is_some() {
                NET_CONFIG.lock().ip = [10, 0, 2, 15];
                NET_CONFIG.lock().gateway_mac = GW_MAC;
                NET_CONFIG.lock().configured = true;
                NET_CONFIG.lock().online = true;
                log(tick, "Online");
                s.phase = 1;
            } else if tick >= 10 {
                let mac = NET_CONFIG.lock().mac;
                if mac != [0; 6] {
                    init_netstack(mac);
                    s.phase = 1;
                } else {
                    log(tick, "No NIC");
                    s.phase = 99;
                }
            }
        }
        1 => {
            if tick >= 30 {
                let mut guard = NETSTACK.lock();
                if let Some(ref mut ns) = *guard {
                    log(tick, "DNS resolving google.com...");
                    if let Some(ip) = ns.dns_resolve("google.com") {
                        s.target_ip = ip;
                        log(tick, &alloc::format!("DNS OK: {}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3]));
                        s.http = Some(ns.http_new(ip, 80, "/"));
                        s.phase = 2;
                    } else {
                        log(tick, "DNS timeout, using fallback IP");
                        s.target_ip = [142, 250, 80, 110];
                        s.http = Some(ns.http_new(s.target_ip, 80, "/"));
                        s.phase = 2;
                    }
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
