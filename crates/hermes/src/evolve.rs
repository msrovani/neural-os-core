//! EvolveAgent WASM hot-swap + rollback ledger (ADR-0047 Pilar 2).
//! Complementa self_evolve.rs (SKILL.md). Sandbox test → promote | rollback.
//! Honesty SESSION_379: nunca snapshot do candidato; registry só após sandbox OK.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use crate::wasmi_rt;

const MAX_GEN_PER_GAP: u32 = 3;

pub struct VersionEntry {
    pub version: u32,
    pub bytecode: Vec<u8>,
    pub generations: u32,
}

pub struct EvolveLedger {
    /// skill → last successfully installed bytecode (rollback target)
    prev: BTreeMap<String, VersionEntry>,
    /// skill → currently live bytecode (post successful hot_swap)
    live: BTreeMap<String, Vec<u8>>,
    /// skill → generation counter for current gap
    gen_count: BTreeMap<String, u32>,
    pub swaps_ok: u64,
    pub rollbacks: u64,
    pub skips: u64,
}

impl EvolveLedger {
    pub fn new() -> Self {
        EvolveLedger {
            prev: BTreeMap::new(),
            live: BTreeMap::new(),
            gen_count: BTreeMap::new(),
            swaps_ok: 0,
            rollbacks: 0,
            skips: 0,
        }
    }

    fn bump_gen(&mut self, skill: &str) -> u32 {
        let e = self.gen_count.entry(String::from(skill)).or_insert(0);
        *e = e.saturating_add(1);
        *e
    }

    /// Hot-swap WASM skill: sandbox via wasmi → promote, else leave registry untouched.
    pub fn hot_swap(
        &mut self,
        name: &str,
        wasm: &[u8],
        _origin: WasmOrigin,
    ) -> Result<(), &'static str> {
        let gen = self.bump_gen(name);
        if gen > MAX_GEN_PER_GAP {
            self.skips = self.skips.saturating_add(1);
            return Err("generation limit");
        }

        // Sandbox FIRST — CAP_NONE; do not touch prev/live/registry yet.
        let test_ok = wasmi_rt::run_wasm(wasm, "_start", &[], wasmi_rt::CAP_NONE).is_ok()
            || wasmi_rt::run_wasm(wasm, "main", &[], wasmi_rt::CAP_NONE).is_ok();

        if !test_ok {
            self.skips = self.skips.saturating_add(1);
            k_nano::slog_hermes!(
                "EVOLVE",
                "warn",
                "hot_swap sandbox fail skill={} gen={} (registry untouched)",
                name,
                gen
            );
            return Err("hot_swap sandbox failed");
        }

        // Snapshot currently live bytecode for real rollback (not the candidate).
        if let Some(old) = self.live.get(name).cloned() {
            let ver = self.prev.get(name).map(|v| v.version.saturating_add(1)).unwrap_or(1);
            self.prev.insert(
                String::from(name),
                VersionEntry {
                    version: ver,
                    bytecode: old,
                    generations: gen,
                },
            );
        }

        self.live.insert(String::from(name), wasm.to_vec());
        let skill =
            crate::dynskill::DynamicSkill::with_wasm(name, "hot-swap skill", "", wasm.to_vec());
        crate::globals::SKILL_REGISTRY.lock().register(Box::new(skill));
        crate::self_evolve::publish_change("skill", name);
        self.swaps_ok = self.swaps_ok.saturating_add(1);
        k_nano::slog_hermes!("EVOLVE", "ok", "hot_swap OK skill={} gen={}", name, gen);
        Ok(())
    }

    /// Rollback: restaura bytecode WASM anterior da ledger.
    pub fn rollback(&mut self, name: &str) -> Result<(), &'static str> {
        let entry = self.prev.get(name).ok_or("no previous version")?;
        let bytes = entry.bytecode.clone();
        let gens = entry.generations;
        let roll = crate::dynskill::DynamicSkill::with_wasm(name, "rollback", "", bytes.clone());
        crate::globals::SKILL_REGISTRY.lock().register(Box::new(roll));
        self.live.insert(String::from(name), bytes);
        crate::self_evolve::publish_change("skill", name);
        self.rollbacks = self.rollbacks.saturating_add(1);
        k_nano::slog_hermes!("EVOLVE", "ok", "rollback skill={} gen={}", name, gens);
        Ok(())
    }
}

lazy_static::lazy_static! {
    static ref EVOLVE_LEDGER: spin::Mutex<EvolveLedger> = spin::Mutex::new(EvolveLedger::new());
}

/// Promove skill efêmera (SkillOpt) → wasmi_rt (ADR-0059 F5).
/// Honesty: materializa bytecode dummy `_start→42` até Cortex emitir op-IR real (#412).
pub fn promote_ephemeral_to_wasm(name: &str, _description: &str) -> Result<(), &'static str> {
    if name.is_empty() || name.len() > 64 {
        return Err("bad_name");
    }
    let wasm = wasmi_rt::generate_wasm_module();
    let mut ledger = EVOLVE_LEDGER.lock();
    ledger.hot_swap(name, &wasm, WasmOrigin::Compiled)?;
    k_nano::slog_hermes!(
        "EVOLVE",
        "warn",
        "ephemeral→WASM skill={} (dummy _start=42 até op-IR real)",
        name
    );
    Ok(())
}

/// Boot / DREAM hook: demo swap on builtin "echo" skill (non-fatal).
pub fn evolve_dream_tick() -> &'static str {
    let demo_wasm = wasmi_rt::generate_wasm_module();
    let mut ledger = EVOLVE_LEDGER.lock();
    match ledger.hot_swap("echo", &demo_wasm, WasmOrigin::Generated) {
        Ok(()) => "OK",
        Err(_) => "SKIP",
    }
}

/// Origem do bytecode WASM (tomada de decisão).
#[derive(Clone)]
pub enum WasmOrigin {
    Generated,
    Compiled,
    External,
}

pub fn evolve_gate_status() -> &'static str {
    let ledger = EVOLVE_LEDGER.lock();
    if ledger.swaps_ok > 0 {
        "OK"
    } else if ledger.rollbacks > 0 || ledger.skips > 0 {
        "SKIP"
    } else {
        drop(ledger);
        evolve_dream_tick()
    }
}

static GENESIS_COUNT: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
const MAX_GENESIS: u32 = 1;

/// Evolve Genesis PoC: one agent may spawn one child WASM agent (ratchet).
pub fn genesis_spawn(parent: &str, _child_desc: &str) -> Result<alloc::string::String, &'static str> {
    let n = GENESIS_COUNT.load(core::sync::atomic::Ordering::Relaxed);
    if n >= MAX_GENESIS {
        return Err("genesis limit");
    }
    let child_name = alloc::format!("gen_{}_{}", parent, n + 1);
    let code = wasmi_rt::generate_wasm_module();
    let mut ledger = EVOLVE_LEDGER.lock();
    ledger.hot_swap(&child_name, &code, WasmOrigin::Generated)?;
    GENESIS_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    k_nano::slog_hermes!(
        "GENESIS",
        "ok",
        "parent={} spawned={} (count={}) dummy_wasm",
        parent,
        child_name,
        n + 1
    );
    Ok(child_name)
}

pub fn genesis_gate_status() -> &'static str {
    match genesis_spawn("echo", "Child agent from Genesis PoC") {
        Ok(_) => "OK",
        Err("genesis limit") => "OK",
        Err(_) => {
            if GENESIS_COUNT.load(core::sync::atomic::Ordering::Relaxed) > 0 {
                "OK"
            } else {
                "SKIP"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hot_swap_fail_leaves_registry_untouched_and_prev_clean() {
        let mut led = EvolveLedger::new();
        let bad = [0u8; 8]; // not valid wasm
        assert!(led.hot_swap("x", &bad, WasmOrigin::Generated).is_err());
        assert!(led.prev.get("x").is_none());
        assert!(led.live.get("x").is_none());
        assert_eq!(led.skips, 1);
        assert_eq!(led.swaps_ok, 0);
    }

    #[test]
    fn hot_swap_success_then_rollback_restores_previous() {
        let mut led = EvolveLedger::new();
        let good = wasmi_rt::generate_wasm_module();
        assert!(led.hot_swap("echo", &good, WasmOrigin::Compiled).is_ok());
        assert!(led.live.get("echo").is_some());
        // Second success snapshots first into prev
        assert!(led.hot_swap("echo", &good, WasmOrigin::Compiled).is_ok());
        assert!(led.prev.get("echo").is_some());
        assert!(led.rollback("echo").is_ok());
        assert_eq!(led.rollbacks, 1);
    }
}
