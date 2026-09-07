//! RTL8139 Fast Ethernet — backend MMIO (ADR-0041 H3).
//! Migrated from k_nano (FASE B). k_nano::rtl8139 delegates here via facade.
//! Limits: port-based MMIO (0x300); policy in k_nano nic_globals.

pub use k_nano::rtl8139::*;
