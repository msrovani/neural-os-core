# crates/hermes/src/

## Summary

Source root of the hermes crate (Ring 3 orchestration, `no_std` + `alloc`,
124 `.rs` files: 90 top-level + 34 in submodules). Hosts intent routing and
the ReAct loop (`hermes.rs`), the native agent implementations (`agents.rs`),
the ADR-0059 WASM runtime (`wasmi_rt.rs`, `wasm_build.rs`, `app_factory.rs`,
`wasi_host.rs`), the package/skills ecosystem (`package_hub.rs`, `skill_*`),
network FE (`net*.rs`, `net_bridge.rs`), security (`membrane.rs`,
`permission_gate.rs`, `approval.rs`), and the FS stack (`vfs/`, `fs/`,
`neural_fs/`).

## Pointer

Full crate map (responsibility, design patterns, data/control flow,
integration points, submodule table, notable top-level modules):
**`../codemap.md`** (crate root).

Submodule maps: `agents/codemap.md`, `apps/codemap.md`, `cross_os/codemap.md`,
`fs/codemap.md`, `neural_fs/codemap.md`. `memory/` (1 file) and `vfs/` (2
files) are covered by the crate map's Submodule Map table and need no
dedicated files.
