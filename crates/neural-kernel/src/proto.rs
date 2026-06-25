use alloc::vec::Vec;
use crate::serial_println;

// Ethernet frame helpers
fn eth_broadcast() -> [u8; 6] { [0xFF; 6] }
fn eth_type_ipv4() -> u16 { 0x0800 }
fn eth_type_arp() -> u16 { 0x0806 }

fn make_eth_header(dst: [u8; 6], src: [u8; 6], ethertype: u16) -> [u8; 14] {
    let mut hdr = [0u8; 14];
    hdr[0..6].copy_from_slice(&dst);
    hdr[6..12].copy_from_slice(&src);
    hdr[12] = (ethertype >> 8) as u8;
    hdr[13] = (ethertype & 0xFF) as u8;
    hdr
}

fn check_eth_header(pkt: &[u8], expected_dst: [u8; 6], expected_type: u16) -> bool {
    if pkt.len() < 14 { return false; }
    if &pkt[0..6] != &expected_dst { return false; }
    let typ = (pkt[12] as u16) << 8 | (pkt[13] as u16);
    typ == expected_type
}

fn ip_checksum(data: &[u8]) -> u16 {
    let mut sum = 0u32;
    let mut i = 0;
    while i + 1 < data.len() {
        sum += (data[i] as u32) << 8 | (data[i + 1] as u32);
        i += 2;
    }
    if i < data.len() { sum += (data[i] as u32) << 8; }
    while sum >> 16 != 0 { sum = (sum & 0xFFFF) + (sum >> 16); }
    !(sum as u16)
}

fn make_ip_header(src: [u8; 4], dst: [u8; 4], proto: u8, payload_len: u16) -> [u8; 20] {
    let total_len = 20 + payload_len;
    let mut hdr = [0u8; 20];
    hdr[0] = 0x45; // Version 4, IHL 5
    hdr[2] = (total_len >> 8) as u8;
    hdr[3] = (total_len & 0xFF) as u8;
    hdr[8] = 64; // TTL
    hdr[9] = proto; // protocol
    hdr[12..16].copy_from_slice(&src);
    hdr[16..20].copy_from_slice(&dst);
    let cksum = ip_checksum(&hdr);
    hdr[10] = (cksum >> 8) as u8;
    hdr[11] = (cksum & 0xFF) as u8;
    hdr
}

// === ARP ===
pub unsafe fn arp_request(local_mac: [u8; 6], target_ip: [u8; 4]) {
    use crate::net::E1000;
    let mut guard = E1000.lock();
    let driver = guard.as_mut().unwrap();

    let mut pkt = Vec::with_capacity(42);
    pkt.extend_from_slice(&make_eth_header(eth_broadcast(), local_mac, eth_type_arp()));

    // ARP header
    pkt.extend_from_slice(&[0x00, 0x01]); // HTYPE: Ethernet
    pkt.extend_from_slice(&[0x08, 0x00]); // PTYPE: IPv4
    pkt.push(6);  // HLEN
    pkt.push(4);  // PLEN
    pkt.extend_from_slice(&[0x00, 0x01]); // OPER: Request
    pkt.extend_from_slice(&local_mac);     // Sender MAC
    pkt.extend_from_slice(&[0, 0, 0, 0]); // Sender IP (will be filled below)
    pkt.extend_from_slice(&[0xFF; 6]);     // Target MAC (broadcast)
    pkt.extend_from_slice(&target_ip);     // Target IP

    // Fill sender IP (our IP from config)
    let our_ip = crate::net::NET_CONFIG.lock().ip;
    pkt[28..32].copy_from_slice(&our_ip); // Sender IP in ARP header

    driver.send(&pkt);
    serial_println!("[ARP] Request enviado para {}.{}.{}.{}", target_ip[0], target_ip[1], target_ip[2], target_ip[3]);
}

pub fn parse_arp_reply(pkt: &[u8]) -> Option<[u8; 6]> {
    if pkt.len() < 42 { return None; }
    // Check ARP reply: Ethernet type 0x0806, ARP oper=2
    let eth_type = (pkt[12] as u16) << 8 | (pkt[13] as u16);
    if eth_type != 0x0806 { return None; }
    let arp_oper = (pkt[20] as u16) << 8 | (pkt[21] as u16);
    if arp_oper != 2 { return None; }
    let mut mac = [0u8; 6];
    mac.copy_from_slice(&pkt[22..28]);
    Some(mac)
}

// === ICMP Echo ===
pub unsafe fn icmp_echo_request(
    driver: &mut crate::e1000::E1000Driver, our_mac: [u8; 6], dst_mac: [u8; 6],
    src_ip: [u8; 4], dst_ip: [u8; 4], ident: u16, seq: u16,
) {
    let mut pkt = Vec::with_capacity(14 + 20 + 8 + 32);
    pkt.extend_from_slice(&make_eth_header(dst_mac, our_mac, eth_type_ipv4()));

    let payload: [u8; 32] = [0x00; 32]; // ICMP data
    let icmp_len = 8 + 32;
    let ip_hdr = make_ip_header(src_ip, dst_ip, 1, icmp_len as u16);
    pkt.extend_from_slice(&ip_hdr);

    // ICMP header: Echo Request (type=8, code=0)
    pkt.push(8); // type
    pkt.push(0); // code
    pkt.extend_from_slice(&[0x00, 0x00]); // checksum placeholder
    pkt.extend_from_slice(&ident.to_be_bytes());
    pkt.extend_from_slice(&seq.to_be_bytes());
    pkt.extend_from_slice(&payload);

    // ICMP checksum
    let icmp_start = 14 + 20;
    let cksum = ip_checksum(&pkt[icmp_start..]);
    pkt[icmp_start + 2] = (cksum >> 8) as u8;
    pkt[icmp_start + 3] = (cksum & 0xFF) as u8;

    driver.send(&pkt);
}

pub fn parse_icmp_reply(pkt: &[u8], our_mac: [u8; 6], _expected_src: [u8; 4], ident: u16, seq: u16) -> Option<u64> {
    // Expect: dst=our_mac, type=IP, IP proto=1, ICMP type=0 (echo reply)
    if pkt.len() < 14 + 20 + 8 { return None; }
    if &pkt[0..6] != &our_mac { return None; }
    if (pkt[12] as u16) << 8 | (pkt[13] as u16) != 0x0800 { return None; }
    if &pkt[23..24] != &[1u8] { return None; } // IP proto ICMP
    // ICMP: type 0 (echo reply)
    if pkt[14 + 20] != 0 { return None; }
    let recv_ident = (pkt[14 + 20 + 4] as u16) << 8 | (pkt[14 + 20 + 5] as u16);
    let recv_seq = (pkt[14 + 20 + 6] as u16) << 8 | (pkt[14 + 20 + 7] as u16);
    if recv_ident != ident || recv_seq != seq { return None; }
    Some(1) // RTT in units (1 = success)
}

// === DHCP ===
pub unsafe fn dhcp_discover(attempt: u32) -> bool {
    use crate::net::E1000;
    let guard = E1000.lock();
    let driver = guard.as_ref().unwrap();
    let our_mac = driver.mac();
    drop(guard);

    let mut guard = E1000.lock();
    let driver = guard.as_mut().unwrap();

    let xid = 0x12345678 + attempt;
    let mut pkt = Vec::with_capacity(14 + 20 + 8 + 240 + 64);
    pkt.extend_from_slice(&make_eth_header(eth_broadcast(), our_mac, eth_type_ipv4()));

    let udp_len = 8 + 240 + 64;
    let ip_hdr = make_ip_header([0, 0, 0, 0], [255, 255, 255, 255], 17, udp_len as u16);
    pkt.extend_from_slice(&ip_hdr);

    // UDP header (src=68, dst=67)
    pkt.extend_from_slice(&[0x00, 68]); // src port
    pkt.extend_from_slice(&[0x00, 67]); // dst port
    pkt.extend_from_slice(&(udp_len as u16).to_be_bytes());
    pkt.extend_from_slice(&[0x00, 0x00]); // checksum placeholder

    // DHCP header (BOOTP format)
    pkt.push(1);  // op = BOOTREQUEST
    pkt.push(1);  // htype = Ethernet
    pkt.push(6);  // hlen = 6
    pkt.push(0);  // hops
    pkt.extend_from_slice(&xid.to_be_bytes()); // xid
    pkt.extend_from_slice(&[0, 0]); // secs
    pkt.extend_from_slice(&[0x80, 0x00]); // flags (broadcast)
    pkt.extend_from_slice(&[0; 4]); // ciaddr
    pkt.extend_from_slice(&[0; 4]); // yiaddr
    pkt.extend_from_slice(&[0; 4]); // siaddr
    pkt.extend_from_slice(&[0; 4]); // giaddr
    pkt.extend_from_slice(&our_mac); // chaddr
    pkt.extend_from_slice(&[0; 10]); // pad to 16 bytes
    pkt.extend_from_slice(&[0; 192]); // sname (64) + file (128)
    // DHCP magic cookie
    pkt.extend_from_slice(&[99, 130, 83, 99]);
    // DHCP options
    pkt.push(53); pkt.push(1); pkt.push(1);  // DHCP Discover
    pkt.push(55); pkt.push(3); pkt.push(1); pkt.push(3); pkt.push(6); // Param req: subnet, router, dns
    pkt.push(12); pkt.push(11); pkt.extend_from_slice(b"neural-aios"); // Hostname
    pkt.push(255); // End

    driver.send(&pkt);
    drop(guard);
    serial_println!("[DHCP] Discover enviado (xid=0x{:08x})", xid);

    // Wait for OFFER
    for _ in 0..20000000 {
        let mut guard = E1000.lock();
        let driver = guard.as_mut().unwrap();
        if let Some(packet) = driver.recv() {
            if let Some((yiaddr, router, dns)) = parse_dhcp_offer(&packet, our_mac, xid) {
                drop(guard);
                serial_println!("[DHCP] Offer recebido: IP {}.{}.{}.{}", yiaddr[0], yiaddr[1], yiaddr[2], yiaddr[3]);
                // Send Request
                let mut cfg = crate::net::NET_CONFIG.lock();
                cfg.ip = yiaddr;
                if let Some(r) = router { cfg.gateway_ip = r; }
                if let Some(d) = dns { cfg.dns_ip = d; }
                drop(cfg);
                dhcp_request(yiaddr, router.unwrap_or([10, 0, 2, 1]), xid);
                for _ in 0..20000000 { core::hint::spin_loop(); }
                let mut guard = E1000.lock();
                let driver = guard.as_mut().unwrap();
                if let Some(packet) = driver.recv() {
                    if parse_dhcp_ack(&packet, our_mac, xid) {
                        serial_println!("[DHCP] ACK recebido. IP configurado.");
                        crate::net::NET_CONFIG.lock().configured = true;
                        return true;
                    }
                }
                return false;
            }
        }
        core::hint::spin_loop();
    }
    false
}

fn parse_dhcp_offer(pkt: &[u8], our_mac: [u8; 6], xid: u32) -> Option<([u8; 4], Option<[u8; 4]>, Option<[u8; 4]>)> {
    if pkt.len() < 14 + 20 + 8 + 240 { return None; }
    let dst = &pkt[0..6];
    let is_broadcast = dst == &[0xFF; 6];
    if dst != &our_mac && !is_broadcast { return None; } // Not for us
    let eth = (pkt[12] as u16) << 8 | (pkt[13] as u16);
    if eth != 0x0800 { return None; }
    if pkt[23] != 17 { return None; } // UDP
    let sp = (pkt[34] as u16) << 8 | (pkt[35] as u16);
    if sp != 67 { return None; } // DHCP server port

    // Check DHCP magic cookie
    let dhcp_start = 14 + 20 + 8;
    if &pkt[dhcp_start + 236..dhcp_start + 240] != &[99, 130, 83, 99] {
        return None;
    }
    // Check xid
    let pkt_xid = u32::from_be_bytes([pkt[dhcp_start + 4], pkt[dhcp_start + 5], pkt[dhcp_start + 6], pkt[dhcp_start + 7]]);
    if pkt_xid != xid { return None; }

    let mut yiaddr = [0u8; 4];
    yiaddr.copy_from_slice(&pkt[dhcp_start + 16..dhcp_start + 20]);

    // Parse DHCP options for router and DNS
    let mut router = None;
    let mut dns = None;
    let mut off = dhcp_start + 240;
    while off + 1 < pkt.len() {
        let opt_type = pkt[off];
        if opt_type == 255 { break; }
        if opt_type == 0 { off += 1; continue; }
        let opt_len = pkt[off + 1] as usize;
        if off + 2 + opt_len > pkt.len() { break; }
        if opt_type == 3 && opt_len >= 4 { // Router
            router = Some([pkt[off + 2], pkt[off + 3], pkt[off + 4], pkt[off + 5]]);
        }
        if opt_type == 6 && opt_len >= 4 { // DNS
            dns = Some([pkt[off + 2], pkt[off + 3], pkt[off + 4], pkt[off + 5]]);
        }
        off += 2 + opt_len;
    }
    Some((yiaddr, router, dns))
}

unsafe fn dhcp_request(requested_ip: [u8; 4], server_ip: [u8; 4], xid: u32) {
    use crate::net::E1000;
    let guard = E1000.lock();
    let driver = guard.as_ref().unwrap();
    let our_mac = driver.mac();
    drop(guard);
    let mut guard = E1000.lock();
    let driver = guard.as_mut().unwrap();

    let mut pkt = Vec::with_capacity(14 + 20 + 8 + 240 + 64);
    pkt.extend_from_slice(&make_eth_header(eth_broadcast(), our_mac, eth_type_ipv4()));
    let udp_len = 8 + 240 + 64;
    let ip_hdr = make_ip_header([0, 0, 0, 0], [255, 255, 255, 255], 17, udp_len as u16);
    pkt.extend_from_slice(&ip_hdr);
    pkt.extend_from_slice(&[0x00, 68, 0x00, 67]); // ports 68, 67
    pkt.extend_from_slice(&(udp_len as u16).to_be_bytes());
    pkt.extend_from_slice(&[0x00, 0x00]);

    pkt.push(1); pkt.push(1); pkt.push(6); pkt.push(0);
    pkt.extend_from_slice(&xid.to_be_bytes());
    pkt.extend_from_slice(&[0, 0, 0x80, 0x00]);
    pkt.extend_from_slice(&[0; 4]); pkt.extend_from_slice(&requested_ip);
    pkt.extend_from_slice(&[0; 4]); pkt.extend_from_slice(&[0; 4]);
    pkt.extend_from_slice(&our_mac);
    pkt.extend_from_slice(&[0; 10]);
    pkt.extend_from_slice(&[0; 192]);
    pkt.extend_from_slice(&[99, 130, 83, 99]);
    pkt.push(53); pkt.push(1); pkt.push(3); // DHCP Request
    pkt.push(50); pkt.push(4); pkt.extend_from_slice(&requested_ip); // Requested IP
    pkt.push(54); pkt.push(4); pkt.extend_from_slice(&server_ip); // Server ID
    pkt.push(12); pkt.push(11); pkt.extend_from_slice(b"neural-aios");
    pkt.push(255);

    driver.send(&pkt);
    serial_println!("[DHCP] Request enviado para IP {}.{}.{}.{}", requested_ip[0], requested_ip[1], requested_ip[2], requested_ip[3]);
}

fn parse_dhcp_ack(pkt: &[u8], our_mac: [u8; 6], xid: u32) -> bool {
    if pkt.len() < 14 + 20 + 8 + 240 { return false; }
    let dst = &pkt[0..6];
    if dst != &our_mac && dst != &[0xFF; 6] { return false; }
    let dhcp_start = 14 + 20 + 8;
    let pkt_xid = u32::from_be_bytes([pkt[dhcp_start + 4], pkt[dhcp_start + 5], pkt[dhcp_start + 6], pkt[dhcp_start + 7]]);
    if pkt_xid != xid { return false; }
    if &pkt[dhcp_start + 236..dhcp_start + 240] != &[99, 130, 83, 99] { return false; }
    let mut off = dhcp_start + 240;
    while off + 1 < pkt.len() {
        if pkt[off] == 255 { break; }
        if pkt[off] == 0 { off += 1; continue; }
        let len = pkt[off + 1] as usize;
        if pkt[off] == 53 && len >= 1 && pkt[off + 2] == 5 { return true; } // DHCP ACK
        off += 2 + len;
    }
    false
}

// === DNS ===
pub unsafe fn dns_query(
    driver: &mut crate::e1000::E1000Driver, our_mac: [u8; 6], dst_mac: [u8; 6],
    src_ip: [u8; 4], dns_ip: [u8; 4], hostname: &str, txid: u16,
) {
    // Encode hostname in DNS format
    let mut dns_name = Vec::new();
    for part in hostname.split('.') {
        dns_name.push(part.len() as u8);
        dns_name.extend_from_slice(part.as_bytes());
    }
    dns_name.push(0); // Root label

    let dns_len = 12 + dns_name.len() + 4; // header + question
    let udp_len = 8 + dns_len;
    let total_len = 14 + 20 + udp_len;

    let mut pkt = Vec::with_capacity(total_len);
    pkt.extend_from_slice(&make_eth_header(dst_mac, our_mac, eth_type_ipv4()));

    let ip_hdr = make_ip_header(src_ip, dns_ip, 17, udp_len as u16);
    pkt.extend_from_slice(&ip_hdr);

    // UDP header
    let udp_src: u16 = 12345;
    pkt.extend_from_slice(&udp_src.to_be_bytes()); // src port
    pkt.extend_from_slice(&[0x00, 0x35]); // dst port 53
    pkt.extend_from_slice(&(udp_len as u16).to_be_bytes());
    pkt.extend_from_slice(&[0x00, 0x00]); // checksum = 0

    // DNS header
    pkt.extend_from_slice(&txid.to_be_bytes()); // txid
    pkt.extend_from_slice(&[0x01, 0x00]); // flags: standard query, RD
    pkt.extend_from_slice(&[0x00, 0x01]); // questions: 1
    pkt.extend_from_slice(&[0x00, 0x00]); // answers: 0
    pkt.extend_from_slice(&[0x00, 0x00]); // authority: 0
    pkt.extend_from_slice(&[0x00, 0x00]); // additional: 0

    // Question section
    pkt.extend_from_slice(&dns_name);
    pkt.extend_from_slice(&[0x00, 0x01]); // QTYPE: A
    pkt.extend_from_slice(&[0x00, 0x01]); // QCLASS: IN

    driver.send(&pkt);
}

pub fn parse_dns_response(pkt: &[u8], our_mac: [u8; 6], _dns_ip: [u8; 4], txid: u16) -> Option<[u8; 4]> {
    if pkt.len() < 14 + 20 + 8 + 12 { return None; }
    if &pkt[0..6] != &our_mac { return None; }
    if (pkt[12] as u16) << 8 | (pkt[13] as u16) != 0x0800 { return None; }
    if pkt[23] != 17 { return None; } // UDP
    let sp = (pkt[34] as u16) << 8 | (pkt[35] as u16);
    if sp != 53 { return None; } // DNS source port

    let dns_start = 14 + 20 + 8;
    let pkt_txid = (pkt[dns_start] as u16) << 8 | (pkt[dns_start + 1] as u16);
    if pkt_txid != txid { return None; }

    // Skip header (12 bytes) + question to get to answer
    let qdcount = (pkt[dns_start + 4] as u16) << 8 | (pkt[dns_start + 5] as u16);
    if qdcount == 0 { return None; }

    // Parse question section to find where answer starts
    let mut off = dns_start + 12;
    // Skip QNAME (labels)
    while off < pkt.len() {
        let len = pkt[off] as usize;
        if len == 0 { off += 1; break; }
        if len >= 0xC0 { off += 2; break; } // Compressed
        off += 1 + len;
    }
    if off + 4 > pkt.len() { return None; }
    off += 4; // Skip QTYPE + QCLASS

    // Parse answer
    if off + 12 > pkt.len() { return None; }
    // Skip name (could be compressed pointer)
    if pkt[off] >= 0xC0 { off += 2; } else {
        while off < pkt.len() {
            let len = pkt[off] as usize;
            if len == 0 { off += 1; break; }
            off += 1 + len;
        }
    }
    off += 2; // type
    off += 2; // class
    off += 4; // TTL
    let rdlength = (pkt[off] as u16) << 8 | (pkt[off + 1] as u16);
    off += 2;

    if rdlength == 4 && off + 4 <= pkt.len() {
        let mut ip = [0u8; 4];
        ip.copy_from_slice(&pkt[off..off + 4]);
        return Some(ip);
    }
    None
}

// === HTTP GET ===
pub unsafe fn http_get_request(
    driver: &mut crate::e1000::E1000Driver, our_mac: [u8; 6], dst_mac: [u8; 6],
    src_ip: [u8; 4], dst_ip: [u8; 4], _port: u16, path: &str,
) {
    let request = alloc::format!(
        "GET {} HTTP/1.1\r\nHost: {}.{}.{}.{}\r\nConnection: close\r\n\r\n",
        path, dst_ip[0], dst_ip[1], dst_ip[2], dst_ip[3]
    );
    let request_bytes = request.as_bytes();
    let total_len = 14 + 20 + request_bytes.len();

    let mut pkt = alloc::vec![0u8; total_len];
    let eth = make_eth_header(dst_mac, our_mac, eth_type_ipv4());
    pkt[..14].copy_from_slice(&eth);

    let ip_hdr = make_ip_header(src_ip, dst_ip, 6, request_bytes.len() as u16);
    pkt[14..34].copy_from_slice(&ip_hdr);

    pkt[34..].copy_from_slice(request_bytes);

    driver.send(&pkt);
}

pub fn parse_http_response(pkt: &[u8], our_mac: [u8; 6]) -> Option<Vec<u8>> {
    if pkt.len() < 14 + 20 { return None; }
    if &pkt[0..6] != &our_mac { return None; }
    if (pkt[12] as u16) << 8 | (pkt[13] as u16) != 0x0800 { return None; }
    if pkt[23] != 6 { return None; } // TCP

    let ip_hdr_len = ((pkt[14] & 0x0F) as usize) * 4;
    let tcp_start = 14 + ip_hdr_len;
    let tcp_hdr_len = ((pkt[tcp_start + 12] >> 4) as usize) * 4;
    let data_start = tcp_start + tcp_hdr_len;

    if data_start >= pkt.len() { return None; }
    let body = Vec::from(&pkt[data_start..]);
    if body.is_empty() { return None; }

    // Try to find HTTP body after \r\n\r\n
    if let Some(pos) = body.windows(4).position(|w| w == b"\r\n\r\n") {
        let content = Vec::from(&body[pos + 4..]);
        if content.is_empty() {
            Some(body) // Return raw response if no body separator found
        } else {
            Some(content)
        }
    } else {
        Some(body)
    }
}
