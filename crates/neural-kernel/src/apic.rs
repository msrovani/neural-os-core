// Legacy re-export wrapper -- prefer direct use k_nano::module over crate::module
//! ADR-0042 emagrecer Onda 5 — re-export k_nano::apic (mouse_gsi IRQ12→44).
pub use k_nano::apic::*;
