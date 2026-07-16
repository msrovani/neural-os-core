# SESSION 116 — ADR-0042 N2.5 k_ai wired

**Data:** 2026-07-16  
**Objetivo:** Wire `k_ai` no bin `neural-kernel`; remover espelhos trust/self_heal.

## Root blocker resolvido
- `k_nano` + `neural-kernel` ambos tinham `#[global_allocator]` → link clash.
- Fix: feature `global-alloc` (default OFF) em `k_nano`; bin mantém `allocator.rs`.

## Mudanças
| Item | Ação |
|------|------|
| `k_nano/Cargo.toml` | feature `global-alloc` |
| `k_nano/src/allocator.rs` | gate `#[global_allocator]` + `#[alloc_error_handler]` |
| `neural-kernel/Cargo.toml` | deps `k-nano`, `k_ai` |
| `neural-kernel/src/memory.rs` | `pub use k_nano::memory::*` |
| `main.rs` | `pub use k_ai::{trust,self_heal}`; `pub use k_nano::globals::EVENT_BUS` |
| `trust.rs`, `self_heal.rs` | **deletados** |

## Build
```
cargo clean -p neural-kernel && cargo nk
→ Finished release 0 errors (30s)
```

## Próximo wire
- **N3.5 cortex:** `cortex.rs`, `bpe.rs`, `tensor.rs`, `trinity.rs`, `nn.rs`, `delta.rs`, `burn_flex.rs`, `tv_dsl.rs`, `arena.rs`, `global_arena.rs`, `r3.rs`, `cortex_mmap.rs`
- **N4.6 hermes:** `hermes.rs`, `cron.rs`, `safety.rs`, `security.rs`, `optimizer.rs`, `netstack.rs`, `wasm*.rs`, `skill_*.rs`, `browser_agent.rs`, `network_agent.rs`, …
- **N5.7 jarbas:** `display/*`, `audio/*`, `jarvis.rs`, `jarbas_fb.rs`, `virtio_gpu.rs`, `uvc_driver.rs`, `vision_agent.rs`
