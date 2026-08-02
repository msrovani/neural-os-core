//! Embedding / embedding-index (BGE, pseudo, TeamMemory) — delegação para k_ai.
//! Single source of truth: `k_ai::memory_systems`. Evita statics duplicados:
//! antes, o boot carregava BGE nas statics do bin e a k_ai nunca via o modelo,
//! então o recall semântico rodava em pseudo-64d mesmo após load OK.

pub use k_ai::memory_systems::*;
