# crates/jarbas/src/ — JARVIS Ring 3 sources

Full crate map (responsibility, patterns, flows, integration):
**[`../codemap.md`](../codemap.md)**.

Layout: `display/` (framebuffer, compositor/WM, cards — the bulk), `audio/`
(voice pipeline mirror), `cards/` (data-only card builders), `gpu/` (FE
re-export of `k_hal::gpu` + desktop cube), plus top-level persona/app files
(`jarvis.rs`, `vconsole.rs`, `ide.rs`, `uvc_driver.rs`, `virtio_gpu.rs`,
`vision_agent.rs`, `screensaver.rs`, `image_viewer.rs`, `clipboard_notify.rs`).

Entry point `lib.rs` declares the 12 public modules; the bin wires jarbas as
`jarbas-crate` and consumes `display`, `gpu`, `jarvis`, `uvc_driver`,
`virtio_gpu`, `vision_agent` directly.
