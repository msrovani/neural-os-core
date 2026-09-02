//! Flags de demo / e2e QEMU — off por default (builds HW/release).
//!
//! Ativar HIT clima QEMU:
//!   `cargo nk --features weather-e2e`
//!   ou `tools/run_weather_e2e.ps1` (documentado).

/// Skinny STT-sim → seed clima → constrained lexicon (Sprint 107 e2e).
/// `false` em HW real: não força seed/lexicon de teste; mantém logs `[STATUS]`/`[GEN]`/etc.
pub const RUN_WEATHER_E2E_SKINNY: bool = cfg!(feature = "weather-e2e");

/// Demos ADR-0041 P4–P9 que usam `clone_current()` (N7 ADR-0102) — off em imagem HW enxuta.
pub const RUN_CAP_DEMOS: bool = cfg!(feature = "cap-demos");
