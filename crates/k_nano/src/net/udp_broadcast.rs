//! UDP Broadcast — descoberta de nos na LAN.
//! Fase A da ADR-0081: transporte UDP para Brain Mesh.
//!
//! Ponte entre k_nano (NoProto + Mesh) e hermes (NETSTACK + smoltcp UDP).
//! k_nano expoe as funcoes de serializacao; hermes fornece o socket.
//!
//! Integracao: NetAgent chama `send_discovery()` e `recv_packet()`.
//!
//! Depende de: smoltcp UDP socket no NETSTACK (hermes).

use crate::net::noproto::{AiosTaskPacket, TaskType, PacketFlags};
use crate::identity;
use alloc::vec::Vec;
use alloc::vec;
use core::mem;
use core::sync::atomic::{AtomicU64, Ordering};

/// Tamanho do pacote NoProto em bytes (repr(C, packed) = 36 bytes).
pub const PACKET_SIZE: usize = mem::size_of::<AiosTaskPacket>();

/// Cria pacote de discovery para broadcast.
pub fn make_discovery(source_id: u8, clock: u64) -> AiosTaskPacket {
    AiosTaskPacket {
        magic: 0x41494F53,
        clock,
        source_id,
        dest_id: 0xFF,
        task_type: TaskType::Sync,
        priority: 0,
        tensor_len: 0,
        param_len: 0,
        flags: PacketFlags { persist: false, require_ack: false, compressed: false, encrypted: false, _reserved: 0 },
        reserved: [0; 8],
    }
}

/// Cria pacote de heartbeat para broadcast.
pub fn make_heartbeat(source_id: u8, clock: u64) -> AiosTaskPacket {
    AiosTaskPacket {
        magic: 0x41494F53,
        clock,
        source_id,
        dest_id: 0xFF,
        task_type: TaskType::Heartbeat,
        priority: 1,
        tensor_len: 0,
        param_len: 0,
        flags: PacketFlags { persist: false, require_ack: false, compressed: false, encrypted: false, _reserved: 0 },
        reserved: [0; 8],
    }
}

/// Serializa pacote para envio via UDP — zero-copy sobre #[repr(C, packed)].
pub fn serialize(packet: &AiosTaskPacket) -> Vec<u8> {
    let size = mem::size_of::<AiosTaskPacket>();
    let mut buf = vec![0u8; size];
    unsafe {
        core::ptr::copy_nonoverlapping(
            packet as *const AiosTaskPacket as *const u8,
            buf.as_mut_ptr(),
            size,
        );
    }
    buf
}

/// Tenta parsear buffer UDP recebido como pacote NoProto.
pub fn parse(data: &[u8]) -> Option<AiosTaskPacket> {
    if data.len() < mem::size_of::<AiosTaskPacket>() {
        return None;
    }
    // Leitura direta do buffer via repr(C, packed) — zero-copy
    let packet = unsafe {
        core::ptr::read_unaligned(data.as_ptr() as *const AiosTaskPacket)
    };
    // Valida magic number
    if packet.magic != 0x41494F53 {
        return None;
    }
    Some(packet)
}

// ── Step 6: Ed25519 signing / verification ──────────────────────────

/// Assina o payload serializado do pacote NoProto com a chave de sessão.
/// Retorna o pacote original + assinatura de 64 bytes concatenada.
///
/// Uso: `let signed = sign_packet(&serialize(&pkt));`
pub fn sign_packet(serialized: &[u8]) -> Option<Vec<u8>> {
    let sig = identity::sign_session(serialized)?;
    let mut out = Vec::with_capacity(serialized.len() + identity::SIGNATURE_LEN);
    out.extend_from_slice(serialized);
    out.extend_from_slice(&sig);
    Some(out)
}

/// Verifica a assinatura Ed25519 no final de um pacote recebido.
/// O pacote deve ter: [NoProto header] + [64-byte signature].
/// Retorna o payload sem assinatura se a verificação passar.
pub fn verify_packet<'a>(data: &'a [u8], pk: &[u8; identity::PUBLIC_KEY_LEN]) -> Option<&'a [u8]> {
    if data.len() < mem::size_of::<AiosTaskPacket>() + identity::SIGNATURE_LEN {
        return None;
    }
    let sig_offset = data.len() - identity::SIGNATURE_LEN;
    let pkt_data = &data[..sig_offset];
    let sig_bytes = &data[sig_offset..];
    let mut sig = [0u8; identity::SIGNATURE_LEN];
    sig.copy_from_slice(sig_bytes);
    if identity::verify_signature(pk, pkt_data, &sig) {
        Some(pkt_data)
    } else {
        None
    }
}

// ── Transporte P2P R0 (ADR-0081 Fase A) ──────────────────────────────────
// Porta 42069, broadcast 255.255.255.255, NIC real (e1000/VirtIO/RTL8139).
// Movido do bin (SESSION_234): o transporte mesh agora vive em k_nano — o bin
// só chama `mesh::p2p_tick()` e consome pacotes via EVENT_BUS ("P2P_PACKET").

/// Contadores de RX/TX do transporte P2P (independentes do smoltcp do bin).
static NET_TX_COUNT: AtomicU64 = AtomicU64::new(0);
static NET_RX_COUNT: AtomicU64 = AtomicU64::new(0);

/// Total de frames TX do transporte P2P (k_nano).
pub fn k_nano_tx_count() -> u64 { NET_TX_COUNT.load(Ordering::Relaxed) }

/// Total de frames RX do transporte P2P (k_nano).
pub fn k_nano_rx_count() -> u64 { NET_RX_COUNT.load(Ordering::Relaxed) }

/// Checksum IP (RFC 1071) — mesmo algoritmo do bin netstack.
fn ip_checksum(data: &[u8]) -> u16 {
    let mut sum = 0u32;
    for chunk in data.chunks(2) {
        let word = u16::from_be_bytes([chunk[0], *chunk.get(1).unwrap_or(&0)]);
        sum = sum.wrapping_add(word as u32);
    }
    while sum >> 16 != 0 { sum = (sum & 0xFFFF) + (sum >> 16); }
    !(sum as u16)
}

/// Envia via NIC real: VirtIO → E1000 → RTL8139 (gate canônico e1000).
/// Sem wifi/slip/I225 — esses paths continuam no smoltcp do bin.
unsafe fn nic_send_k(data: Vec<u8>) {
    if let Some(ref mut nic) = *crate::nic_globals::VIRTIO_DEV.lock() {
        nic.send(&data); return;
    }
    if let Some(ref mut nic) = *crate::nic_globals::E1000.lock() {
        nic.send(&data); return;
    }
    if let Some(ref mut nic) = *crate::nic_globals::RTL8139.lock() {
        nic.send(&data); return;
    }
}

/// Recebe do NIC real: VirtIO → E1000 → RTL8139.
unsafe fn nic_recv_k() -> Option<Vec<u8>> {
    if let Some(ref mut nic) = *crate::nic_globals::VIRTIO_DEV.lock() {
        if let Some(pkt) = nic.recv() { return Some(pkt); }
    }
    if let Some(ref mut nic) = *crate::nic_globals::E1000.lock() {
        if let Some(pkt) = nic.recv() { return Some(pkt); }
    }
    if let Some(ref mut nic) = *crate::nic_globals::RTL8139.lock() {
        if let Some(pkt) = nic.recv() { return Some(pkt); }
    }
    None
}

/// Monta frame Ethernet + IP + UDP com destino broadcast (FF:FF:FF:FF:FF:FF → 255.255.255.255).
/// Lê (sip, smac) do `nic_globals::NET_CONFIG` (sync via `set_nic_config` pelo bin).
pub fn build_udp_broadcast_frame(payload: &[u8], port: u16) -> Option<Vec<u8>> {
    let (sip, smac) = {
        let cfg = crate::nic_globals::NET_CONFIG.lock();
        let sip = if cfg.ip != [0; 4] { cfg.ip } else { [10, 0, 2, 15] };
        (sip, cfg.mac)
    };
    if smac == [0; 6] || payload.is_empty() {
        return None;
    }
    let src_port: u16 = 42069;
    let udp_len = (8 + payload.len()) as u16;
    let mut udp = Vec::with_capacity(udp_len as usize);
    udp.extend_from_slice(&src_port.to_be_bytes());
    udp.extend_from_slice(&port.to_be_bytes());
    udp.extend_from_slice(&udp_len.to_be_bytes());
    udp.extend_from_slice(&[0x00, 0x00]);
    udp.extend_from_slice(payload);

    let dst: [u8; 4] = [255, 255, 255, 255];
    let total_len = (20 + udp.len()) as u16;
    let mut ip = [0u8; 20];
    ip[0] = 0x45;
    ip[2..4].copy_from_slice(&total_len.to_be_bytes());
    ip[8] = 64;
    ip[9] = 17; // UDP
    ip[12..16].copy_from_slice(&sip);
    ip[16..20].copy_from_slice(&dst);
    let cs = ip_checksum(&ip);
    ip[10..12].copy_from_slice(&cs.to_be_bytes());

    let dmac = [0xFF; 6]; // broadcast Ethernet
    let mut frame = Vec::with_capacity(14 + 20 + udp.len());
    frame.extend_from_slice(&dmac);
    frame.extend_from_slice(&smac);
    frame.extend_from_slice(&[0x08, 0x00]);
    frame.extend_from_slice(&ip);
    frame.extend_from_slice(&udp);
    Some(frame)
}

/// Envia payload UDP para 255.255.255.255:port (broadcast mesh P2P).
pub fn udp_broadcast_send(payload: &[u8], port: u16) -> bool {
    let Some(frame) = build_udp_broadcast_frame(payload, port) else {
        return false;
    };
    unsafe { nic_send_k(frame) };
    NET_TX_COUNT.fetch_add(1, Ordering::Relaxed);
    true
}

/// Recebe um payload UDP (dst_port == port) do RX do NIC. Não bloqueia.
pub fn udp_broadcast_recv(port: u16) -> Option<Vec<u8>> {
    // Drena até achar um pacote UDP para nossa porta (ou esvazia o RX).
    for _ in 0..16 {
        let pkt = unsafe { nic_recv_k()? };
        NET_RX_COUNT.fetch_add(1, Ordering::Relaxed);
        if pkt.len() < 14 + 20 + 8 {
            continue;
        }
        if pkt[12] != 0x08 || pkt[13] != 0x00 {
            continue;
        }
        let ihl = (pkt[14] & 0x0f) as usize * 4;
        if ihl < 20 || pkt.len() < 14 + ihl + 8 {
            continue;
        }
        if pkt[14 + 9] != 17 {
            continue; // não-UDP
        }
        let udp = 14 + ihl;
        let dport = u16::from_be_bytes([pkt[udp + 2], pkt[udp + 3]]);
        if dport != port {
            continue;
        }
        let ulen = u16::from_be_bytes([pkt[udp + 4], pkt[udp + 5]]) as usize;
        if ulen < 8 || pkt.len() < udp + ulen {
            continue;
        }
        return Some(pkt[udp + 8..udp + ulen].to_vec());
    }
    None
}
