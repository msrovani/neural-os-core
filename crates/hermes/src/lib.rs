#![cfg_attr(not(test), no_std)]
#![allow(dead_code)]
#![allow(static_mut_refs)]
#![allow(unused_unsafe)]

extern crate alloc;

// ─── hermes: Agent Runtime & Network ───
// Agent framework, intent routing, network stack, WASM runtime, skills
// Depends on k_nano, cortex, and k_ia.

// pub mod actor_registry; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
// pub mod adaptation; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
pub mod agents;
// pub mod app_store; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
pub mod approval;
pub mod hitl_ui;
pub mod apps;
pub mod browser_agent;
// pub mod cron; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
pub mod cross_os;
// pub mod elf_loader; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
pub mod hermes;
pub mod hub;
pub mod mcp;
pub mod mcp_server;
pub mod net;
pub mod net_bridge;
pub mod netdiag;
pub mod netfs;
pub mod netstack;
pub mod network_agent;
// pub mod optimizer; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
// pub mod orchestrator; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
pub mod plugin_hub;
// pub mod rss_agent; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
// pub mod safety; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
// pub mod search_agent; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
pub mod security;
pub mod self_update;
// pub mod sgdb_agent; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
pub mod shell;
pub mod skill_gen;
pub mod skill_loader;
pub mod skill_manifest;
pub mod skill_market;
// pub mod skill_marketplace; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
pub mod memory_store;
pub mod memory;
pub mod marketplace;
pub mod membrane;
// pub mod native_agents; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
// pub mod intent_bus; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
pub mod cognitive_bridge;
pub mod executive;
// pub mod expert_skills; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
pub mod skill_observer;
pub mod self_evolve;
pub mod evolve;
pub mod hw_pnp;
pub mod hal_offer;
pub mod package_hub;
pub mod permission_gate;
// pub mod quarantine; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
pub mod decode_harness;
pub mod structured_decode;
// pub mod wasi_host; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
pub mod wasmi_rt;
pub mod wasm_build;
// pub mod app_factory; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
pub mod dynskill;
// pub mod gguf_wasm; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
pub mod micropython_wasm;
pub mod affect;
pub mod emotion;
pub mod soul;
pub use affect::*;
pub use emotion::*;
pub use soul::*;
// pub mod notification_gate; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
// pub mod aios_api; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
pub mod skill_opt;
// pub mod skill_sync; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
pub mod mesh_knowledge;
// pub mod email_agent; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
pub mod fs;
pub mod neural_fs;
pub mod vfs;
// pub mod link_watcher; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
pub mod globals;
pub mod runtime_observe;
// pub mod wifi_agent; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
pub mod wifi_protocol;
// pub mod wpa2_hs; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
// pub mod ipc_bus; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
pub mod ntp;
pub mod async_io;
// pub mod git_thin; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
pub mod theme_bridge;
pub mod manpages;
// pub mod cf_challenge; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
// ADR-0041 H3: MMIO WiFi BE em k-hal; hermes = FE
pub use k_hal::net::generic_wifi;
pub use k_hal::net::wifi_compat;
pub use k_hal::net::wifi_iwlwifi;
pub use k_hal::net::wifi_msix;
// ADR-0062 E3 — SoftMAC BE via k-hal; hermes re-exporta
pub use k_hal::net::wifi_softmac;
// pub mod voice_skill; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
// pub mod trinity_inject; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
// pub mod proactive; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
// pub mod net_fallback; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
pub mod stream_packet;
// pub mod chat_tree; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
// pub mod graph_engine; // DEAD CODE — 0 callers (HERMES_AUDIT.md)
pub mod matrix_learn;
pub mod tls;






