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
pub mod agency_seed;
// ponytail: arch module disabled (pre-existing SIMD intrinsics errors, unrelated to BEI)
// pub mod arch;
pub mod audit;
pub mod boot_log_agent;
pub mod chunker;
pub mod economy;
pub mod expert_lifecycle;
pub mod cognitive;
pub mod context_window;
pub mod conversation;
pub mod gguf;
// ponytail: hal.rs moved to LEGACY/v1.5-dead-k2chj/k_ia/ (dead code)
pub mod hw_agents;
pub mod hw_capability;
pub mod inventory;
pub mod memory_agent;
pub mod model_fit;
pub mod memory_systems;
pub mod native_agent_seed;
pub mod profile;
pub mod self_heal;
pub mod self_heal_agent;
pub mod training_agent;
pub mod fs;
pub mod trust;
pub mod shutdown;
pub mod usage;
pub mod ternary;
pub mod router;
