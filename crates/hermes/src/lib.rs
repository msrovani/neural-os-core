#![cfg_attr(not(test), no_std)]
#![allow(dead_code)]
#![allow(static_mut_refs)]
#![allow(unused_unsafe)]

extern crate alloc;

// ─── hermes: Agent Runtime & Network ───
// Agent framework, intent routing, network stack, WASM runtime, skills
// Depends on k_nano, cortex, and k_ia.

pub mod agents;
pub mod approval;
// s410m: forget cognitivo HITL — registry de alvos pendentes do approval gate.
pub mod forget;
#[cfg(test)]
mod forget_tests;
pub mod hitl_ui;
pub mod apps;
pub mod browser_agent;
pub mod cron; // T-026: Cron/LogAgent POST /api/logs com backoff
pub mod ota; // T-022/T-030: OTA facade (net_bridge) + ChromeOS tries
pub mod provision; // T-024: NET_READY + first_boot gate
pub mod cross_os;
pub mod hermes;
pub mod hub;
pub mod lab_inject;
pub mod skill_lab; // F1.5 (OPCODE-0063): hook de lab LSK1 @0x02110000 (G gera / E reusa + roda)
pub mod mcp;
pub mod net;
pub mod net_bridge;
pub mod netdiag;
pub mod netfs;
pub mod netstack;
pub mod network_agent;
pub mod plugin_hub;
pub mod security;
pub mod hub_triage; // s432: triagem IA do HUB HEALTH (premissa máx. ADR-0088) — snapshot HUB\0 + pior-estado + proposta HITL
pub mod anti_frag; // s442: IA-observa→IA-age — TALC fragmentado vira AÇÃO (vocabulário fechado + HITL + verificação medida)
pub mod safety; // s390b: I1–I4 + SAFETY_CHECK (não órfão)
pub mod self_update;
// pub mod shell; // DELETED SESSION_379 residual (0 callers; Command::Install)
pub mod skill_gen;
pub mod skill_loader;
pub mod skill_manifest;
pub mod skill_market;
pub mod memory_store;
pub mod memory;
pub mod marketplace;
pub mod membrane;
pub mod cognitive_bridge;
pub mod site_policy;
pub mod typed_sites;
pub mod executive;
pub mod skill_observer;
pub mod self_evolve;
pub mod evolve;
pub mod hw_pnp;
pub mod hal_offer;
pub mod package_hub;
pub mod permission_gate;
pub mod decode_harness;
pub mod structured_decode;
pub mod wasmi_rt;
pub mod wasm_build;
pub mod app_factory; // ADR-0102: register_native_ring seam (isolation_ring)
pub mod dynskill;
pub mod micropython_wasm;
pub mod affect;
pub mod emotion;
pub mod soul;
pub use affect::*;
pub use emotion::*;
pub use soul::*;
// DEAD removed SESSION_379 residual: shell, notification_gate, aios_api, git_thin,
// cf_challenge, voice_skill, proactive, net_fallback, graph_engine
pub mod skill_opt;
pub mod bei; // ADR-0060 — BeiState/tick (emagreçer s359: saiu do bin)
pub mod skill_sync; // reativado s359 — wire bei_tick (HERMES_AUDIT mentia "0 callers")
pub mod skill_marketplace; // reativado s359 — mesh MKTP + poll_p2p
pub mod mesh_knowledge;
pub mod fs;
pub mod neural_fs;
pub mod vfs;
pub mod globals;
pub mod runtime_observe;
pub mod wifi_protocol;
pub mod wpa2_hs; // demo HS — ReadyForTraffic só com SoftMAC real (SESSION_379)
pub mod ipc_bus; // CapGate smoke + MessageBus wrap (SESSION_379 residual)
pub mod ntp;
pub mod async_io;
pub mod theme_bridge;
pub mod manpages;
pub mod hub_health;
pub mod audio_health; // AUDIO_HEALTH consumer — escala NO_GO persistente ao LLM (anti-loop SESSION_410)
pub mod fleet_health; // FLEET_HEALTH RX MCH\0 + agregação worst-of da frota no Master (SESSION_417)
pub mod sys_health; // SYS_HEALTH produtor+escalador (net/storage/gpu) — política única k_nano::sys_health (SESSION_415)
pub mod hw_inventory; // snapshot do HW detectado (produtor: HwDetectAgent)
// ADR-0041 H3: MMIO WiFi BE em k-hal; hermes = FE
pub use k_hal::net::generic_wifi;
pub use k_hal::net::wifi_compat;
pub use k_hal::net::wifi_iwlwifi;
pub use k_hal::net::wifi_msix;
// ADR-0062 E3 — SoftMAC BE via k-hal; hermes re-exporta
pub use k_hal::net::wifi_softmac;
pub mod trinity_inject;
pub mod stream_packet;
pub mod chat_tree;
pub mod tls;






