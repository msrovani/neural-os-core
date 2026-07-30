// Instâncias globais de drivers de rede — armazenamento canônico em nic_globals.
// Hermes possui a stack completa (NETSTACK, DHCP, etc.); k_nano guarda refs HW.
pub use crate::nic_globals::*;

pub mod mesh;
pub mod noproto;
pub mod transport;

pub use noproto::{AiosTaskPacket, NoProtoParser, PacketFlags, TaskType, AIOS_MAGIC, PACKET_HEADER_SIZE};
pub use transport::{HybridTransport, TransportConfig, TransportMode, TransportError};
