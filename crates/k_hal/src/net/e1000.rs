//! e1000/e1000e Gigabit Ethernet — FACADE R1.
//!
//! A implementação REAL (MMIO, rings, ECS/reset, init) vive em `k_nano::e1000`
//! (R0). Este módulo apenas re-exporta esse subestado (`pub use k_nano::e1000::*`)
//! para clientes R1 — não é backend independente. (M9 onda 4: o comentário
//! antigo dizia o contrário — "k_nano delega aqui" — e era falso.)

pub use k_nano::e1000::*;
