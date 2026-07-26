//! RustPython no_std — Sprint 106-5 (viabilidade)
//!
//! RustPython **não é no_std nativo** (depende de `std`).
//! Rota oficial para Python no Neural OS: **MicroPython via WASM** (`micropython_wasm.rs`).
//!
//! Este módulo documenta a decisão arquitetural e expõe helpers mínimos
//! para agentes que precisam referenciar a rota Python sem embed nativo.

/// Indica se a rota nativa RustPython está disponível (sempre false no bare-metal).
pub const NATIVE_AVAILABLE: bool = false;

/// Rota recomendada para execução Python.
pub const RECOMMENDED_ROUTE: &str = "micropython_wasm";

/// Mensagem de diagnóstico para logs de boot.
pub fn viability_report() -> &'static str {
    "[RustPython] no_std nativo indisponível — use MicroPython/WASM (Sprint 106-6)"
}






