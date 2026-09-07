//! Intel I225/I226 (igc) 2.5G Ethernet — backend MMIO (ADR-0041 H3 / ADR-0062 P7).
//! Migrated from k_nano (FASE B). k_nano::i225 delegates here via facade.
//! Limits: MMIO reg access; QEMU nao emula (validacao plena = HW real).

pub use k_nano::i225::*;
