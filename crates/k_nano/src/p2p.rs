//! P2P Orchestration Module for Neural-OS-Core
//! 
//! This module provides peer-to-peer communication between AIOS Master and Nodes
//! using Lamport logical clocks, NoProto zero-copy serialization, and hybrid transport.

pub mod clock;
pub mod noproto;

pub use clock::{LogicalClock, VectorClock};
pub use noproto::{AiosTaskPacket, NoProtoParser};
