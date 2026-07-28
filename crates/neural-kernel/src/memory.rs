// Legacy re-export wrapper -- prefer direct use k_nano::module over crate::module
//! ADR-0042 N2.5 — re-export k_nano::memory para GLOBAL_ALLOCATOR único (SelfHeal + boot).

pub use k_nano::memory::*;
