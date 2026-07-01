#![allow(dead_code)]
use alloc::vec::Vec;

pub fn eth_broadcast() -> [u8; 6] { [0xFF; 6] }
pub fn eth_type_ipv4() -> u16 { 0x0800 }

pub fn eth_header(dst: [u8; 6], src: [u8; 6], ethertype: u16) -> [u8; 14] {
    let mut hdr = [0u8; 14];
    hdr[0..6].copy_from_slice(&dst);
    hdr[6..12].copy_from_slice(&src);
    hdr[12] = (ethertype >> 8) as u8;
    hdr[13] = (ethertype & 0xFF) as u8;
    hdr
}

pub fn ip_checksum(data: &[u8]) -> u16 {
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

pub fn ip_header(src: [u8; 4], dst: [u8; 4], proto: u8, payload_len: u16) -> [u8; 20] {
    let total_len = 20 + payload_len;
    let mut hdr = [0u8; 20];
    hdr[0] = 0x45;
    hdr[2] = (total_len >> 8) as u8;
    hdr[3] = (total_len & 0xFF) as u8;
    hdr[8] = 64;
    hdr[9] = proto;
    hdr[12..16].copy_from_slice(&src);
    hdr[16..20].copy_from_slice(&dst);
    let cksum = ip_checksum(&hdr);
    hdr[10] = (cksum >> 8) as u8;
    hdr[11] = (cksum & 0xFF) as u8;
    hdr
}

pub fn parse_arp_reply(pkt: &[u8]) -> Option<[u8; 6]> {
    if pkt.len() < 42 { return None; }
    let eth_type = (pkt[12] as u16) << 8 | (pkt[13] as u16);
    if eth_type != 0x0806 { return None; }
    let arp_oper = (pkt[20] as u16) << 8 | (pkt[21] as u16);
    if arp_oper != 2 { return None; }
    let mut mac = [0u8; 6];
    mac.copy_from_slice(&pkt[22..28]);
    Some(mac)
}

pub fn parse_icmp_reply(pkt: &[u8], our_mac: [u8; 6], _expected_src: [u8; 4], ident: u16, seq: u16) -> Option<u64> {
    if pkt.len() < 14 + 20 + 8 { return None; }
    if &pkt[0..6] != &our_mac { return None; }
    if (pkt[12] as u16) << 8 | (pkt[13] as u16) != 0x0800 { return None; }
    if &pkt[23..24] != &[1u8] { return None; }
    if pkt[14 + 20] != 0 { return None; }
    let recv_ident = (pkt[14 + 20 + 4] as u16) << 8 | (pkt[14 + 20 + 5] as u16);
    let recv_seq = (pkt[14 + 20 + 6] as u16) << 8 | (pkt[14 + 20 + 7] as u16);
    if recv_ident != ident || recv_seq != seq { return None; }
    Some(1)
}

pub fn parse_http_response(pkt: &[u8], our_mac: [u8; 6]) -> Option<Vec<u8>> {
    if pkt.len() < 14 + 20 { return None; }
    if &pkt[0..6] != &our_mac { return None; }
    if (pkt[12] as u16) << 8 | (pkt[13] as u16) != 0x0800 { return None; }
    if pkt[23] != 6 { return None; }
    let ip_hdr_len = ((pkt[14] & 0x0F) as usize) * 4;
    let tcp_start = 14 + ip_hdr_len;
    let tcp_hdr_len = ((pkt[tcp_start + 12] >> 4) as usize) * 4;
    let data_start = tcp_start + tcp_hdr_len;
    if data_start >= pkt.len() { return None; }
    let body = Vec::from(&pkt[data_start..]);
    if let Some(pos) = body.windows(4).position(|w| w == b"\r\n\r\n") {
        Some(Vec::from(&body[pos + 4..]))
    } else {
        Some(body)
    }
}
