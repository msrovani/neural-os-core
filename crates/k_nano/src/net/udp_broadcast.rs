//! UDP Broadcast — descoberta de nos na LAN.
//! Fase A da ADR-0081: transporte UDP para Brain Mesh.
//!
//! Ponte entre k_nano (NoProto + Mesh) e hermes (NETSTACK + smoltcp UDP).
//! k_nano expoe as funcoes de serializacao; hermes fornece o socket.
//!
//! Integracao: NetAgent chama `send_discovery()` e `recv_packet()`.

use crate::net::noproto::{AiosTaskPacket, NoProtoParser, TaskType, PacketFlags};
use alloc::vec::Vec;

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
        flags: PacketFlags(0),
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
        flags: PacketFlags(0),
        reserved: [0; 8],
    }
}

/// Serializa pacote para envio via UDP.
pub fn serialize(packet: &AiosTaskPacket) -> Vec<u8> {
    NoProtoParser::serialize_header(packet)
}

/// Tenta parsear buffer UDP recebido como pacote NoProto.
pub fn parse(data: &[u8]) -> Option<AiosTaskPacket> {
    NoProtoParser::parse(data)
}
