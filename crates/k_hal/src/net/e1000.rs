//! e1000/e1000e Gigabit Ethernet — backend MMIO (ADR-0041 H3).
//! Migrated from k_nano (FASE B). k_nano::e1000 delegates here via facade.
//! Limits: MMIO reg access + descriptor rings; policy in k_nano nic_globals.

pub use k_nano::e1000::*;
