//! ADR-0059 F5 Bridge: re-exporta `DynamicSkill` do `skill-registry` + `with_wasm`.
//! O crate `hermes` expõe `DynamicSkill` para o binário (`neural-kernel`) e módulos
//! internos (`evolve.rs`, `skill_opt.rs`).

pub use skill_registry::dynskill::DynamicSkill;
