//! @dead — OpCode VM verifier nunca invocado (WASM substituiu).
//! Mantido para sprint futuro de Skill Sandbox (eBPF-style).
//! Veja `main.rs` seção \"DEAD MODULES\" para contexto.
#![allow(dead_code)]

// Re-export from k_nano (canonical location)
pub use k_nano::verify::*;
