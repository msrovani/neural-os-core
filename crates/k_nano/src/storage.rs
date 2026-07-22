//! Storage Module for Neural-OS-Core
//! 
//! Provides NVMe driver and TicKV integration for persistent storage.

pub mod nvme;
pub mod tickv;

pub use nvme::{NvmeController, NvmeQueue, NvmeResult};
pub use tickv::{TicKVAdapter, TicKVStorage};
