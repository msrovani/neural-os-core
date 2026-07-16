# SESSION 119 — ADR-0042 N5.7 (jarbas wired no bin)

**Data:** 2026-07-16  
**Versão:** v1.7.11  
**Pista:** Monolith cleanup pós-N4.6 (`574cf41`)

## Objetivo

Wire crate `jarbas` no bin `neural-kernel` via dep direta + `pub use jarbas_crate::{…}`; remover espelhos display/gpu/persona; manter `audio/*` no monólito (ADR-0045 + Sprint107 wakeword).

## Feito

| Item | Status |
|------|--------|
| `jarbas-crate = { package = "jarbas", path = "../jarbas" }` em `neural-kernel/Cargo.toml` | ✅ |
| Feature `jarbas-bridge` removida — always-on (k_nano sem `global-alloc`) | ✅ |
| `pub use jarbas_crate::{display, gpu, jarvis, virtio_gpu, uvc_driver, vision_agent}` | ✅ |
| `paint_tts_response` / `boot_splash` portados para `jarbas/src/display/fb.rs` | ✅ |
| `VRAM_BUDDY` → `pub` em `jarbas/gpu/vram.rs` (memory_agent) | ✅ |
| Gate `[N5-JARBAS] full_wire=OK(jarbas-crate)` | ✅ |
| `jarbas_bridge.rs` — TOPIC_* via `jarbas_crate::audio` | ✅ |
| `cargo clean -p neural-kernel && cargo nk` | ✅ 0 erros |
| Espelhos removidos (29 arquivos) | ✅ |

## Deletados (29)

**display/** (9): `agent.rs`, `avatar.rs`, `compositor.rs`, `console.rs`, `fb.rs`, `font.rs`, `mod.rs`, `theme.rs`, `ttf_engine.rs`

**gpu/** (16): `mod.rs`, `detect.rs`, `backend.rs`, `bench.rs`, `ring.rs`, `intel.rs`, `nvidia.rs`, `amd.rs`, `firmware.rs`, `vram.rs`, `display_coex.rs`, `kv_dma.rs`, `msched.rs`, `xqueue.rs`, `cube.rs`, `xpu.rs`

**raiz** (4): `jarvis.rs`, `virtio_gpu.rs`, `uvc_driver.rs`, `vision_agent.rs`

## Residual monólito (N5.7)

| Arquivo / módulo | Motivo |
|------------------|--------|
| `audio/*` | ADR-0045 truth path; `voice.rs` tem Sprint107 wakeword (`wakeword_in`, `woken`) — diverge de `jarbas::audio` |
| `jarbas_fb.rs` | CapGate P4 FB demo — integração bin-only |
| `jarbas_bridge.rs` | Cross-check TOPIC_* monólito vs `jarbas_crate::audio` |

## Próximo

- Sprint Sound — unificar audio truth com jarbas quando wakeword path convergir
- Gate `v2.0.0` review (N1–N5 + wire crates ✅; qualidade voz pendente)
