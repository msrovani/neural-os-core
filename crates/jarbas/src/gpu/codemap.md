# crates/jarbas/src/gpu/ — GPU front-end (ADR-0041 H2)

**Responsibility**: GPU FE for the R3 layer; all MMIO/BAR access stays in the
k_hal R1 backend — jarbas only re-exports and adds a software demo.

**Key symbols**: `pub use k_hal::gpu::*` (mod.rs: backend, GPU vendor;
`k_hal::gpu::backend::disable_intel_vga_plane` is used by `display::fb`);
`cube::{start_transition, render_crossfade, is_transitioning}` (integer-only
workspace crossfade drawn on `DoubleBuffer`, no FPU).

**Integration**: `display::fb::fb_remap_uc` calls the k_hal backend; the bin
re-exports `gpu` and drives the cube demo from the compositor path.
