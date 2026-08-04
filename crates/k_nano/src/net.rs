// Instâncias globais de drivers de rede — armazenamento canônico em nic_globals.
// Hermes possui a stack completa (NETSTACK, DHCP, etc.); k_nano guarda refs HW.
pub use crate::nic_globals::*;

pub mod mesh;
pub mod noproto;
pub mod transport;
pub mod udp_broadcast;
// p2p_sim: simulação ADR-0081 stale — referencia API antiga (NodeCapabilities
// pré-7a97556, CpuArch removido, serialize_header antigo, KeyPair::generate).
// Rewrite = refactor grande; fora do escopo. Gate p/ cargo test no host.
#[cfg(all(test, feature = "p2p-sim"))]
pub mod p2p_sim;

pub use noproto::{AiosTaskPacket, NoProtoParser, PacketFlags, TaskType, AIOS_MAGIC, PACKET_HEADER_SIZE};
pub use transport::{HybridTransport, TransportConfig, TransportMode, TransportError};
