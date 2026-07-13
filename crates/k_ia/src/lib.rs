#![no_std]
#![allow(dead_code)]
#![allow(static_mut_refs)]
#![allow(unused_unsafe)]

extern crate alloc;

// ─── k_ia: Cognitive & AI Infrastructure ───
// Self-healing, cognitive systems, training, memory, trust, inventory
// Depends on k_nano (hardware) and cortex (BitNet engine).

pub mod agency;
pub mod agency_importer;
pub mod audit;
pub mod boot_log_agent;
pub mod chunker;
pub mod cognitive;
pub mod context_window;
pub mod conversation;
pub mod gguf;
// ponytail: hal.rs moved to LEGACY/v1.5-dead-k2chj/k_ia/ (dead code)
pub mod hw_agents;
pub mod inventory;
pub mod memory_agent;
pub mod memory_systems;
pub mod profile;
pub mod self_heal;
pub mod training_agent;
pub mod fs;
pub mod trust;
pub mod shutdown;
pub mod usage;
