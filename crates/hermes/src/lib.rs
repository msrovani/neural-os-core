#![no_std]
#![allow(dead_code)]
#![allow(static_mut_refs)]
#![allow(unused_unsafe)]

extern crate alloc;

// ─── hermes: Agent Runtime & Network ───
// Agent framework, intent routing, network stack, WASM runtime, skills
// Depends on k_nano, cortex, and k_ia.

pub mod actor_registry;
pub mod agents;
pub mod app_store;
pub mod approval;
pub mod apps;
pub mod browser_agent;
pub mod cron;
pub mod elf_loader;
pub mod hermes;
pub mod hub;
pub mod mcp;
pub mod net;
pub mod netfs;
pub mod netstack;
pub mod network_agent;
pub mod optimizer;
pub mod orchestrator;
pub mod plugin_hub;
pub mod rss_agent;
pub mod safety;
pub mod search_agent;
pub mod security;
pub mod self_update;
pub mod shell;
pub mod skill_gen;
pub mod skill_loader;
pub mod skill_market;
pub mod skill_observer;
pub mod structured_decode;
pub mod wasm;
pub mod wasm_exec;
pub mod wasm_rt;
pub mod email_agent;
pub mod fs;
pub mod link_watcher;
pub mod generic_wifi;
pub mod wifi_agent;
pub mod wifi_protocol;
pub mod wifi_compat;
// ponytail: wifi_aer/dma/apic.rs moved to LEGACY/v1.5-dead-k2chj/hermes/ (dead code)
pub mod wifi_iwlwifi;
pub mod wifi_msix;
pub mod voice_skill;
