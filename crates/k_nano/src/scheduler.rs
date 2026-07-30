//! Scheduler — delegated to agent-core crate
//! Core pair allocator for ADR-0057 compute dispatch.

pub mod cfs;
pub mod core_pair;
pub use core_pair::{CorePairAllocator, CorePairState, CoreRole, BipoleMode};
