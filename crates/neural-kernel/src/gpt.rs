//! ADR-0042 emagrecer Onda 1/4 — re-export k_nano::gpt.
// ponytail: bin re-export waiting for E5 migration; suppress dead-code
#[allow(unused_imports)]
pub use k_nano::gpt::*;
