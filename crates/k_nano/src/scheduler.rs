//! Elastic Scheduler for Neural-OS-Core
//! 
//! Implements core pair allocation, MWAIT power management, and dynamic scaling
//! from 2-core bipole mode to N-core elastic scaling.

pub mod core_pair;

pub use core_pair::{CorePairAllocator, CorePairState, CoreRole, BipoleMode};
