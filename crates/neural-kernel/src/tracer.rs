//! @dead — span tracer nunca instanciado (SelfCritique substituiu).
//! Mantido para sprint futuro de Distributed Tracing.
//! Veja `main.rs` seção \"DEAD MODULES\" para contexto.
#![allow(dead_code)]

// Re-export from k_nano (canonical location)
pub use k_nano::tracer::*;
