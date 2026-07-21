//! MicroPython via WASM — ADR-0042 residual → delegado para hermes_crate::micropython_wasm.
//! ponytail: re-export da impl real no hermes, sem duplicação.

pub use hermes_crate::micropython_wasm::*;
