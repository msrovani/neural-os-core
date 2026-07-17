//! ADR-0040 — re-export k_nano::mhi para MHI_REGISTRY unico (Optimizer + DiskAgent).
//! Tick wired: hermes OptimizerAgent → k_nano::mhi::mhi_tick.

pub use k_nano::mhi::*;
