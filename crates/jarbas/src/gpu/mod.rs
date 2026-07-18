//! GPU FE — reexport k-hal BE (ADR-0041 H2). MMIO vive em k_hal; jarbas só FE + cube.

pub use k_hal::gpu::*;

/// Demo FE (DoubleBuffer) — não toca BAR GPU.
pub mod cube;
