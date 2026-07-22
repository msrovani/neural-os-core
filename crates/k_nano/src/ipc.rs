//! Inter-Process Communication for Neural-OS-Core
//! 
//! Implements transparent messaging between local cores and remote nodes
//! via the CellChannel abstraction (Silicon & Network Telepathy).

pub mod mesh;

pub use mesh::{CellChannel, CellMessageDescriptor, LocalCellChannel, RemoteCellChannel, ChannelType};
