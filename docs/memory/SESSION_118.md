# SESSION 118 — ADR-0042 N4.6 (hermes wired no bin)

**Data:** 2026-07-16  
**Versão:** v1.7.10  
**Pista:** Monolith cleanup pós-N3.5 (`9a9ab57`)

## Objetivo

Wire crate `hermes` no bin `neural-kernel` via dep direta + `pub use hermes_crate::{…}`; remover espelhos compatíveis; manter integração bin-only onde API diverge (net stack, agents, fs, CapGate).

## Feito

| Item | Status |
|------|--------|
| `hermes-crate = { package = "hermes", path = "../hermes" }` em `neural-kernel/Cargo.toml` | ✅ |
| `pub use hermes_crate::{actor_registry, apps, cron, hermes, safety, security, wasm*, wifi*, …}` | ✅ |
| Alias `hermes-crate` evita shadow com módulos re-exportados | ✅ |
| Fix `cortex.rs` → `cortex_crate::HardwareRegisterMap` (generic_wifi re-export) | ✅ |
| Gate `[N4-HERMES] full_wire=OK(hermes-crate)` | ✅ |
| `cargo clean -p neural-kernel && cargo nk` | ✅ 0 erros |
| Espelhos removidos (37 arquivos) | ✅ |

## Deletados (37)

`actor_registry.rs`, `approval.rs`, `app_store.rs`, `browser_agent.rs`, `cron.rs`, `elf_loader.rs`, `generic_wifi.rs`, `hermes.rs`, `hub.rs`, `mcp.rs`, `optimizer.rs`, `plugin_hub.rs`, `rustpython_no_std.rs`, `safety.rs`, `search_agent.rs`, `security.rs`, `self_update.rs`, `skill_gen.rs`, `skill_loader.rs`, `skill_observer.rs`, `skill_opt.rs`, `structured_decode.rs`, `voice_skill.rs`, `wasm.rs`, `wasm_exec.rs`, `wasm_rt.rs`, `wifi_agent.rs`, `wifi_compat.rs`, `wifi_iwlwifi.rs`, `wifi_msix.rs`, `wifi_protocol.rs`, `orchestrator.rs`, `skill_market.rs`, `apps/mod.rs`, `apps/hermes_app.rs`, `apps/settings_app.rs`, `apps/power_app.rs`

## Residual monólito (N4.6)

| Arquivo / módulo | Motivo |
|------------------|--------|
| `agents.rs` | Fleet nativo + HermesAgent; lazy_static globals em `main.rs` (≠ `hermes::globals`) |
| `cognitive.rs` | Engine cognitivo Sprint 95 — não existe no crate `hermes` |
| `net.rs`, `netstack.rs`, `network_agent.rs`, `netdiag.rs`, `netfs.rs`, `link_watcher.rs` | NETSTACK singleton + `virtio_net` init acoplado ao bin |
| `rtl8139.rs`, `e1000.rs`, `virtio_net.rs` | Drivers NIC; `agents.rs` chama `crate::virtio_net::init_driver_virtio()` |
| `fs/*` | VFS monólito (`inference_fs_agent`, `mhi_scheduler`, …) |
| `aios_api.rs` | CapGate P3 (`syscall::Cap`) — crate usa `globals::read_vfs` stub |
| `micropython_wasm.rs` | Loader VFS via `crate::fs` (≠ `hermes::globals`) |

## Próximo

- **N5.7** wire `jarbas` (padrão N3.5/N4.6)
