//! ADR-0059 F5 Bridge: `DynamicSkill` + CapGate `DYNSKILL_TOKEN` (IDEA #600 / SESSION_383).
//!
//! Registro canônico: `register_dynskill` — SkillRegistry + `trust_allow(0xD1, name)`.
//! Legacy(1) sozinho **não** autoriza DynSkill.

pub use skill_registry::dynskill::{DynamicSkill, DYNSKILL_TOKEN};
use skill_registry::Skill;

use alloc::boxed::Box;
use event_bus::CapabilityToken;

/// Token CapGate para executar / publicar eventos de DynSkill.
#[inline]
pub fn dynskill_cap_token() -> CapabilityToken {
    CapabilityToken::Legacy(DYNSKILL_TOKEN)
}

/// Registra DynamicSkill e concede Trust `(DYNSKILL_TOKEN, name)`.
/// Sem isto, Contain/Enforce nega execute mesmo com token correto no manifesto.
pub fn register_dynskill(skill: DynamicSkill) {
    let name = skill.manifest().name.clone();
    crate::globals::SKILL_REGISTRY
        .lock()
        .register(Box::new(skill));
    let now = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
    crate::globals::TRUST_CACHE
        .lock()
        .trust_allow(DYNSKILL_TOKEN, &name, now);
    k_nano::slog_hermes!(
        "DynSkill",
        "ok",
        "registered+trust token=0x{:X} skill={}",
        DYNSKILL_TOKEN,
        name
    );
}
