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
