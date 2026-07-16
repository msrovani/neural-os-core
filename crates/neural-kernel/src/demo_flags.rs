//! Flags de demo / e2e QEMU — off por default (builds HW/release).
//!
//! Ativar HIT clima QEMU:
//!   `cargo nk --features weather-e2e`
//!   ou `tools/run_weather_e2e.ps1` (documentado).

/// Skinny STT-sim → seed clima → constrained lexicon (Sprint 107 e2e).
/// `false` em HW real: não força seed/lexicon de teste; mantém logs `[STATUS]`/`[GEN]`/etc.
pub const RUN_WEATHER_E2E_SKINNY: bool = cfg!(feature = "weather-e2e");
