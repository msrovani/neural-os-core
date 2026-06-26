use crate::net::{NETSTACK, NET_CONFIG};
use crate::netstack::{NetStack, HttpConn, HttpState};
use crate::serial_println;

const GOOGLE_IP: [u8; 4] = [142, 250, 80, 110];
const GW_MAC: [u8; 6] = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];

fn log(tick: u64, msg: &str) {
    serial_println!("[NET @t={}] {}", tick, msg);
}

fn init_netstack(mac: [u8; 6]) {
    let mut ns = NetStack::new(mac);
    ns.set_ip([10, 0, 2, 15]);
    *NETSTACK.lock() = Some(ns);
}

pub async fn network_agent_daemon() {
    let mut tick: u64 = 0;
    let mut state: u8 = 0;
    let mut http: Option<HttpConn> = None;

    loop {
        if let Some(ref mut ns) = *NETSTACK.lock() {
            ns.poll(tick as i64);

            if let Some(ref mut c) = http {
                ns.http_poll(c, tick);
                match &c.state {
                    HttpState::Done(data) => {
                        log(tick, &alloc::format!("HTTP OK: {} bytes", data.len()));
                        ns.http_close(c);
                        http = None;
                        state = 99;
                    }
                    HttpState::Failed => {
                        log(tick, "HTTP failed");
                        ns.http_close(c);
                        http = None;
                        state = 99;
                    }
                    _ => {}
                }
            }
        }

        match state {
            0 => {
                if NETSTACK.lock().is_some() {
                    NET_CONFIG.lock().mac = NET_CONFIG.lock().mac; // force existence
                    NET_CONFIG.lock().ip = [10, 0, 2, 15];
                    NET_CONFIG.lock().gateway_mac = GW_MAC;
                    NET_CONFIG.lock().configured = true;
                    NET_CONFIG.lock().online = true;
                    log(tick, "Online");
                    state = 1;
                } else if tick >= 10 {
                    let mac = NET_CONFIG.lock().mac;
                    if mac != [0; 6] {
                        init_netstack(mac);
                        state = 1;
                    } else {
                        state = 99;
                        log(tick, "No NIC");
                    }
                }
            }
            1 => {
                if tick >= 30 && NETSTACK.lock().is_some() {
                    let mut guard = NETSTACK.lock();
                    let ns = guard.as_mut().unwrap();
                    http = Some(ns.http_new(GOOGLE_IP, 80, "/"));
                    log(tick, "HTTP GET google.com:80");
                    state = 2;
                }
            }
            _ => {
                if tick % 200 == 0 && NET_CONFIG.lock().configured {
                    log(tick, "Health");
                }
            }
        }

        tick = tick.wrapping_add(1);
        crate::task::yield_now().await;
    }
}
 