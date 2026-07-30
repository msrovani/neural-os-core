#![no_std]
#![allow(dead_code)]
#![allow(static_mut_refs)]
#![allow(unused_unsafe)]

extern crate alloc;

// ─── k_ai: Cognitive & AI Infrastructure (Ring 1) ───
// Self-healing, trust, audit, agency, training, memory, inventory
// Depends on k_nano (foundation) and cortex (BitNet). Sem dep Ring 2 (jarbas/hermes).

pub mod agency;
pub mod agency_importer;
pub mod arch;
pub mod audit;
pub mod boot_log_agent;
pub mod merkle_audit;
pub mod chunker;
pub mod economy;
pub use economy::{CompressionTier, BudgetManager};
pub mod expert_lifecycle;
pub mod cognitive;
pub mod context_window;
pub mod conversation;
pub mod success_engine;
pub mod feedback_agent;
pub mod data_collector;
// E1a: gguf moved to cortex_crate
// ponytail: hal.rs moved to LEGACY/v1.5-dead-k2chj/k_ia/ (dead code)
pub mod hw_agents;
pub mod hw_capability;
pub mod inventory;
pub mod memory_agent;
pub mod model_fit;
pub mod memory_systems;
pub mod multi_user;
pub mod vision;

pub mod native_agent_seed;
pub mod profile;
pub mod self_heal;
pub mod self_heal_agent;
pub mod self_heal_disk;
pub mod training_agent;
pub mod fine_tuning_pipeline;
/// ADR-0081 C5: Federated Gradient Sharing (#312f).
pub mod fl_trainer;
pub use fl_trainer::FederatedTrainer;
pub mod fs;
pub mod trust;
pub mod shutdown;
pub mod usage;
pub mod workflow_learner;
pub mod ternary;
pub mod router;
pub mod safety_invariants;
pub mod security_detectors;
pub mod self_optimizing_scheduler;
/// ADR-0063 F2–F5 SGDB (MemoryDoc · Engine · ART · BQ) — MVP Onda 5.
pub mod self_learning;
pub mod sgdb;
/// Compat: re-export status do stub antigo.
pub mod sgdb_residual {
    pub use crate::sgdb::status_line as sgdb_residual_status;
}
