use alloc::vec::Vec;
use alloc::format;
use crate::net::{RTL8139, NET_CONFIG};
use crate::serial_println;

const ETH_ARP: u16 = 0x0806;
const ETH_IPV4: u16 = 0x0800;
const IPPROTO_ICMP: u8 = 1;
const IPPROTO_UDP: u8 = 17;

const GITHUB_IP: [u8; 4] = [140, 82, 121, 3];
const NTP_SERVER: [u8; 4] = [200, 160, 7, 186];
const NTP_EPOCH: u64 = 2208988800;
const BR_TZ_OFFSET: u64 = 10800;

enum PacketClass {
    ArpRequest { sender_mac: [u8; 6], target_ip: [u8; 4] },
    ArpReply { sender_mac: [u8; 6] },
    IcmpEcho { src_ip: [u8; 4], ident: u16, seq: u16, payload: Vec<u8> },
    IcmpEchoReply { ident: u16 },
    UdpPacket { dst_port: u16 },
    DhcpOffer { yiaddr: [u8; 4] },
    Unknown { eth_type: u16, len: usize },
}

fn classify(pkt: &[u8]) -> PacketClass {
    if pkt.len() < 14 { return PacketClass::Unknown { eth_type: 0, len: pkt.len() }; }
    let eth_type = (pkt[12] as u16) << 8 | (pkt[13] as u16);

    match eth_type {
        ETH_ARP => {
            if pkt.len() < 42 { return PacketClass::Unknown { eth_type, len: pkt.len() }; }
            let oper = (pkt[20] as u16) << 8 | (pkt[21] as u16);
            let sender_mac = [pkt[22], pkt[23], pkt[24], pkt[25], pkt[26], pkt[27]];
            match oper {
                1 => PacketClass::ArpRequest { sender_mac, target_ip: [pkt[38], pkt[39], pkt[40], pkt[41]] },
                2 => PacketClass::ArpReply { sender_mac },
                _ => PacketClass::Unknown { eth_type, len: pkt.len() },
            }
        }
        ETH_IPV4 => {
            if pkt.len() < 14 + 20 { return PacketClass::Unknown { eth_type, len: pkt.len() }; }
            let ip_proto = pkt[23];
            let src_ip = [pkt[26], pkt[27], pkt[28], pkt[29]];
            match ip_proto {
                IPPROTO_ICMP => {
                    if pkt.len() < 14 + 20 + 8 { return PacketClass::Unknown { eth_type, len: pkt.len() }; }
                    let icmp_type = pkt[14 + 20];
                    let ident = (pkt[14 + 20 + 4] as u16) << 8 | (pkt[14 + 20 + 5] as u16);
                    let seq = (pkt[14 + 20 + 6] as u16) << 8 | (pkt[14 + 20 + 7] as u16);
                    match icmp_type {
                        8 => PacketClass::IcmpEcho { src_ip, ident, seq, payload: pkt[14 + 20 + 8..].to_vec() },
                        0 => PacketClass::IcmpEchoReply { ident },
                        _ => PacketClass::Unknown { eth_type, len: pkt.len() },
                    }
                }
                IPPROTO_UDP => {
                    if pkt.len() < 14 + 20 + 8 { return PacketClass::Unknown { eth_type, len: pkt.len() }; }
                    let dst_port = (pkt[36] as u16) << 8 | (pkt[37] as u16);
                    if dst_port == 68 {
                        classify_dhcp_offer(pkt)
                    } else {
                        PacketClass::UdpPacket { dst_port }
                    }
                }
                _ => PacketClass::Unknown { eth_type, len: pkt.len() },
            }
        }
        _ => PacketClass::Unknown { eth_type, len: pkt.len() },
    }
}

fn classify_dhcp_offer(pkt: &[u8]) -> PacketClass {
    let dhcp_start = 14 + 20 + 8;
    if pkt.len() < dhcp_start + 240 { return PacketClass::Unknown { eth_type: 0x0800, len: pkt.len() }; }
    if &pkt[dhcp_start + 236..dhcp_start + 240] != &[99, 130, 83, 99] {
        return PacketClass::Unknown { eth_type: 0x0800, len: pkt.len() };
    }
    let yiaddr = [pkt[dhcp_start + 16], pkt[dhcp_start + 17], pkt[dhcp_start + 18], pkt[dhcp_start + 19]];
    PacketClass::DhcpOffer { yiaddr }
}

fn build_arp_reply(local_mac: [u8; 6], our_ip: [u8; 4], target_mac: [u8; 6], target_ip: [u8; 4]) -> Vec<u8> {
    let mut pkt = Vec::with_capacity(42);
    pkt.extend_from_slice(&crate::proto::eth_header(target_mac, local_mac, ETH_ARP));
    pkt.extend_from_slice(&[0x00, 0x01, 0x08, 0x00, 6, 4, 0x00, 0x02]);
    pkt.extend_from_slice(&local_mac);
    pkt.extend_from_slice(&our_ip);
    pkt.extend_from_slice(&target_mac);
    pkt.extend_from_slice(&target_ip);
    pkt
}

fn build_icmp_echo_reply(dst_mac: [u8; 6], local_mac: [u8; 6], our_ip: [u8; 4], src_ip: [u8; 4], ident: u16, seq: u16, payload: &[u8]) -> Vec<u8> {
    let mut pkt = Vec::with_capacity(14 + 20 + 8 + payload.len());
    pkt.extend_from_slice(&crate::proto::eth_header(dst_mac, local_mac, ETH_IPV4));
    pkt.extend_from_slice(&crate::proto::ip_header(our_ip, src_ip, IPPROTO_ICMP, (8 + payload.len()) as u16));
    pkt.push(0); pkt.push(0);
    pkt.extend_from_slice(&[0, 0]);
    pkt.extend_from_slice(&ident.to_be_bytes());
    pkt.extend_from_slice(&seq.to_be_bytes());
    pkt.extend_from_slice(payload);
    let cksum = crate::proto::ip_checksum(&pkt[14 + 20..]);
    pkt[14 + 20 + 2] = (cksum >> 8) as u8;
    pkt[14 + 20 + 3] = (cksum & 0xFF) as u8;
    pkt
}

fn build_icmp_echo_request(local_mac: [u8; 6], dst_mac: [u8; 6], our_ip: [u8; 4], target_ip: [u8; 4], ident: u16, seq: u16) -> Vec<u8> {
    let mut pkt = Vec::with_capacity(14 + 20 + 8 + 32);
    pkt.extend_from_slice(&crate::proto::eth_header(dst_mac, local_mac, ETH_IPV4));
    pkt.extend_from_slice(&crate::proto::ip_header(our_ip, target_ip, IPPROTO_ICMP, 40));
    pkt.push(8); pkt.push(0);
    pkt.extend_from_slice(&[0, 0]);
    pkt.extend_from_slice(&ident.to_be_bytes());
    pkt.extend_from_slice(&seq.to_be_bytes());
    let payload: [u8; 32] = [0; 32];
    pkt.extend_from_slice(&payload);
    let cksum = crate::proto::ip_checksum(&pkt[14 + 20..]);
    pkt[14 + 20 + 2] = (cksum >> 8) as u8;
    pkt[14 + 20 + 3] = (cksum & 0xFF) as u8;
    pkt
}

fn build_ntp_request(local_mac: [u8; 6], dst_mac: [u8; 6], our_ip: [u8; 4], server_ip: [u8; 4], our_port: u16) -> Vec<u8> {
    let ntp_payload: [u8; 48] = {
        let mut b = [0u8; 48];
        b[0] = 0x1B;
        b
    };
    let udp_len = 8 + 48;
    let mut pkt = Vec::with_capacity(14 + 20 + udp_len as usize);
    pkt.extend_from_slice(&crate::proto::eth_header(dst_mac, local_mac, ETH_IPV4));
    pkt.extend_from_slice(&crate::proto::ip_header(our_ip, server_ip, IPPROTO_UDP, udp_len));
    pkt.extend_from_slice(&our_port.to_be_bytes());
    pkt.extend_from_slice(&123u16.to_be_bytes());
    pkt.extend_from_slice(&udp_len.to_be_bytes());
    pkt.extend_from_slice(&[0, 0]);
    pkt.extend_from_slice(&ntp_payload);
    pkt
}

fn parse_ntp_timestamp(pkt: &[u8]) -> Option<u64> {
    if pkt.len() < 14 + 20 + 8 + 48 { return None; }
    let dst_port = (pkt[36] as u16) << 8 | (pkt[37] as u16);
    if dst_port != 12345 { return None; }
    let ntp_start = 14 + 20 + 8;
    if pkt[ntp_start] & 0x3F != 0x24 { return None; }
    let seconds = u32::from_be_bytes([
        pkt[ntp_start + 40], pkt[ntp_start + 41],
        pkt[ntp_start + 42], pkt[ntp_start + 43],
    ]);
    Some(seconds as u64)
}

fn ntp_to_br_time(ntp_secs: u64) -> (u64, &'static str) {
    let unix = ntp_secs.saturating_sub(NTP_EPOCH);
    let br = unix.saturating_sub(BR_TZ_OFFSET);
    (br, "America/Sao_Paulo (UTC-3)")
}

fn format_datetime(secs: u64) -> alloc::string::String {
    let days = secs / 86400;
    let rem = secs % 86400;
    let h = (rem / 3600) as u8;
    let m = ((rem % 3600) / 60) as u8;
    let s = (rem % 60) as u8;

    let y = 1970u64;
    let mut d = days;
    let mut year = y;
    loop {
        let yr_days = if year % 400 == 0 || (year % 4 == 0 && year % 100 != 0) { 366 } else { 365 };
        if d < yr_days { break; }
        d -= yr_days;
        year += 1;
    }
    let leap = year % 400 == 0 || (year % 4 == 0 && year % 100 != 0);
    let month_days: [u64; 12] = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 1u8;
    for &md in &month_days {
        if d < md { break; }
        d -= md;
        month += 1;
    }
    let day = (d + 1) as u8;

    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        year as u16, month, day, h, m, s)
}

fn log(tick: u64, msg: &str) {
    serial_println!("[NET @t={}] {}", tick, msg);
}

pub async fn network_agent_daemon() {
    let our_ip = [10, 0, 2, 15];
    let gw_mac = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
    const SKIP_PING: u64 = 30;
    const SKIP_NTP: u64 = 60;
    const PING_IDENT: u16 = 0x4321;

    let mut tick: u64 = 0;
    let mut configured = false;
    let mut ping_sent = false;
    let mut ping_replied = false;
    let mut ntp_sent = false;
    let mut ntp_replied = false;
    let mut ntp_time: Option<alloc::string::String> = None;

    loop {
        let mut guard = RTL8139.lock();
        if let Some(ref mut driver) = *guard {
            loop {
                let pkt = unsafe { driver.recv() };
                let data = match pkt { Some(d) => d, None => break };
                let class = classify(&data);

                match class {
                    PacketClass::ArpRequest { sender_mac, target_ip } => {
                        if target_ip == our_ip {
                            let local_mac = driver.mac();
                            let reply = build_arp_reply(local_mac, our_ip, sender_mac, target_ip);
                            unsafe { driver.send(&reply); }
                            log(tick, &format!("ARP reply to {}.{}.{}.{}", target_ip[0], target_ip[1], target_ip[2], target_ip[3]));
                        }
                    }
                    PacketClass::IcmpEcho { src_ip, ident, seq, payload } => {
                        let local_mac = driver.mac();
                        let dst_mac = [data[6], data[7], data[8], data[9], data[10], data[11]];
                        let reply = build_icmp_echo_reply(dst_mac, local_mac, our_ip, src_ip, ident, seq, &payload);
                        unsafe { driver.send(&reply); }
                        log(tick, &format!("ICMP echo reply to {}.{}.{}.{}", src_ip[0], src_ip[1], src_ip[2], src_ip[3]));
                    }
                    PacketClass::IcmpEchoReply { ident, .. } => {
                        if ident == PING_IDENT {
                            ping_replied = true;
                            log(tick, "Ping reply from github.com (140.82.121.3)");
                        }
                    }
                    PacketClass::ArpReply { sender_mac } => {
                        log(tick, &format!("ARP reply from {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                            sender_mac[0], sender_mac[1], sender_mac[2],
                            sender_mac[3], sender_mac[4], sender_mac[5]));
                        if !configured { NET_CONFIG.lock().gateway_mac = sender_mac; }
                    }
                    PacketClass::UdpPacket { dst_port } => {
                        if dst_port == 12345 && !ntp_replied {
                            if let Some(ntp_secs) = parse_ntp_timestamp(&data) {
                                let (br_unix, tz) = ntp_to_br_time(ntp_secs);
                                let dt = format_datetime(br_unix);
                                ntp_time = Some(dt.clone());
                                ntp_replied = true;
                                log(tick, &format!("NTP reply from {}.{}.{}.{}: {} ({})",
                                    NTP_SERVER[0], NTP_SERVER[1], NTP_SERVER[2], NTP_SERVER[3],
                                    dt, tz));
                            }
                        }
                    }
                    PacketClass::DhcpOffer { yiaddr } => {
                        log(tick, &format!("DHCP offer: IP {}.{}.{}.{}", yiaddr[0], yiaddr[1], yiaddr[2], yiaddr[3]));
                    }
                    PacketClass::Unknown { eth_type, len } => {
                        if tick % 500 == 0 && eth_type != 0 {
                            log(tick, &format!("Unknown: len={} eth=0x{:04x}", len, eth_type));
                        }
                    }
                }
            }
        }
        drop(guard);

        if !configured && tick >= 10 {
            NET_CONFIG.lock().ip = our_ip;
            NET_CONFIG.lock().gateway_mac = gw_mac;
            NET_CONFIG.lock().configured = true;
            NET_CONFIG.lock().online = true;
            configured = true;
            log(tick, &format!("Online. IP: {}.{}.{}.{} GW: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                our_ip[0], our_ip[1], our_ip[2], our_ip[3],
                gw_mac[0], gw_mac[1], gw_mac[2], gw_mac[3], gw_mac[4], gw_mac[5]));
        }

        if configured && !ping_sent && tick >= SKIP_PING {
            let local_mac = NET_CONFIG.lock().mac;
            let gw = NET_CONFIG.lock().gateway_mac;
            let request = build_icmp_echo_request(local_mac, gw, our_ip, GITHUB_IP, PING_IDENT, 1);
            let mut guard = RTL8139.lock();
            if let Some(ref mut driver) = *guard {
                unsafe { driver.send(&request); }
                ping_sent = true;
                log(tick, "Ping to github.com (140.82.121.3) sent");
            }
            drop(guard);
        }

        if configured && ping_sent && !ntp_sent && tick >= (SKIP_PING + SKIP_NTP) {
            let local_mac = NET_CONFIG.lock().mac;
            let gw = NET_CONFIG.lock().gateway_mac;
            let request = build_ntp_request(local_mac, gw, our_ip, NTP_SERVER, 12345);
            let mut guard = RTL8139.lock();
            if let Some(ref mut driver) = *guard {
                unsafe { driver.send(&request); }
                ntp_sent = true;
                log(tick, &format!("NTP request to {}.{}.{}.{} sent", NTP_SERVER[0], NTP_SERVER[1], NTP_SERVER[2], NTP_SERVER[3]));
            }
            drop(guard);
        }

        if ping_sent && !ping_replied && tick >= (SKIP_PING + 20) {
            log(tick, "Ping timeout — no reply from github.com (NAT filtered)");
            ping_replied = true;
        }

        if ntp_sent && !ntp_replied && tick >= (SKIP_PING + SKIP_NTP + 40) {
            log(tick, &format!("NTP timeout — no reply from {}.{}.{}.{}", NTP_SERVER[0], NTP_SERVER[1], NTP_SERVER[2], NTP_SERVER[3]));
            ntp_replied = true;
        }

        if tick % 200 == 0 && configured {
            if let Some(ref t) = ntp_time {
                log(tick, &format!("Health — last NTP sync: {}", t));
            } else {
                log(tick, "Health");
            }
        }

        tick = tick.wrapping_add(1);
        crate::task::yield_now().await;
    }
}
