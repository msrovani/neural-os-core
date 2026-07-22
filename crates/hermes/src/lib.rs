#![no_std]
#![allow(dead_code)]
#![allow(static_mut_refs)]
#![allow(unused_unsafe)]

extern crate alloc;

// ─── hermes: Agent Runtime & Network ───
// Agent framework, intent routing, network stack, WASM runtime, skills
// Depends on k_nano, cortex, and k_ia.

pub mod actor_registry;
// ponytail: adaptation module disabled (pre-existing k_nano::hardware dep broken)
// pub mod adaptation;
pub mod agents;
pub mod app_store;
pub mod approval;
pub mod hitl_ui;
pub mod apps;
pub mod browser_agent;
pub mod cron;
pub mod elf_loader;
pub mod hermes;
pub mod hub;
pub mod mcp;
pub mod net;
pub mod net_bridge;
pub mod netdiag;
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
pub mod memory_store;
pub mod marketplace;
pub mod cognitive_bridge;
pub mod expert_skills;
pub mod skill_observer;
pub mod self_evolve;
pub mod evolve;
pub mod hw_pnp;
pub mod hal_offer;
pub mod package_hub;
pub mod decode_harness;
pub mod structured_decode;
pub mod wasm;
pub mod wasm_exec;
pub mod wasm_rt;
pub mod wasmi_rt;
pub mod app_factory;
pub mod dynskill;
pub mod gguf_wasm;
pub mod micropython_wasm;
pub mod aios_api;
pub mod skill_opt;
pub mod rustpython_no_std;
pub mod email_agent;
pub mod fs;
pub mod link_watcher;
pub mod globals;
pub mod wifi_agent;
pub mod wifi_protocol;
// ADR-0041 H3: MMIO WiFi BE em k-hal; hermes = FE
pub use k_hal::net::generic_wifi;
pub use k_hal::net::wifi_compat;
pub use k_hal::net::wifi_iwlwifi;
pub use k_hal::net::wifi_msix;
pub mod voice_skill;
