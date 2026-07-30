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
