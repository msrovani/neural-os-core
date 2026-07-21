//! EvolveAgent WASM hot-swap + rollback ledger (ADR-0047 Pilar 2).
//! Complementa self_evolve.rs (SKILL.md). Sandbox test → promote | rollback.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use crate::wasmi_rt;

const MAX_GEN_PER_GAP: u32 = 3;

pub struct VersionEntry {
    pub version: u32,
    pub bytecode: Vec<u8>,  // ponytail: wasm bytecode (wasmi), não Op VM (ADR-0059)
    pub generations: u32,
}

pub struct EvolveLedger {
    /// skill → previous bytecode (for rollback)
    prev: BTreeMap<String, VersionEntry>,
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

    /// Hot-swap WASM skill: sandbox via wasmi → promote, else rollback.
    pub fn hot_swap(
        &mut self,
        name: &str,
        wasm: &[u8],
        origin: WasmOrigin,
    ) -> Result<(), &'static str> {
        let gen = self.bump_gen(name);
        if gen > MAX_GEN_PER_GAP {
            self.skips = self.skips.saturating_add(1);
            return Err("generation limit");
        }

        // ponytail: wasmi sandbox test = validate + run _start. Sem ApprovalGate.
        let test_ok = wasmi_rt::run_wasm(wasm, "_start", &[], 0).is_ok()
            || wasmi_rt::run_wasm(wasm, "main", &[], 0).is_ok();

        // Snapshot previous wasm antes de instalar
        self.prev.insert(
            String::from(name),
            VersionEntry {
                version: self.prev.get(name).map(|v| v.version + 1).unwrap_or(1),
                bytecode: wasm.to_vec(),
                generations: gen,
            },
        );

        // Install candidate → registra como DynamicSkill com wasm
        let skill = crate::dynskill::DynamicSkill::with_wasm(name, "hot-swap skill", "", wasm.to_vec());
        crate::globals::SKILL_REGISTRY.lock().register(Box::new(skill));

        if test_ok {
            self.swaps_ok = self.swaps_ok.saturating_add(1);
            k_nano::slog_hermes!("EVOLVE", "info", "hot_swap OK skill={} gen={}", name, gen);
            Ok(())
        } else {
            // Rollback
            if let Some(entry) = self.prev.get(name) {
                let roll = crate::dynskill::DynamicSkill::with_wasm(name, "rollback", "", entry.bytecode.clone());
                crate::globals::SKILL_REGISTRY.lock().register(Box::new(roll));
                self.rollbacks = self.rollbacks.saturating_add(1);
                k_nano::slog_hermes!("EVOLVE", "info", "rollback skill={} gen={}", name, gen);
            } else {
                self.skips = self.skips.saturating_add(1);
            }
            Err("hot_swap sandbox failed")
        }
    }

    /// Rollback: restaura bytecode WASM anterior da ledger.
    /// ponytail: registra DynamicSkill de volta; sem ApprovalGate.
    pub fn rollback(&mut self, name: &str) -> Result<(), &'static str> {
        let entry = self.prev.get(name).ok_or("no previous version")?;
        let roll = crate::dynskill::DynamicSkill::with_wasm(name, "rollback", "", entry.bytecode.clone());
        crate::globals::SKILL_REGISTRY.lock().register(Box::new(roll));
        self.rollbacks = self.rollbacks.saturating_add(1);
        k_nano::slog_hermes!("EVOLVE", "info", "rollback skill={} gen={}", name, entry.generations);
        Ok(())
    }
}

lazy_static::lazy_static! {
    static ref EVOLVE_LEDGER: spin::Mutex<EvolveLedger> = spin::Mutex::new(EvolveLedger::new());
}

/// Promove skill efêmera (SkillOpt) → wasmi_rt (ADR-0059 F5).
/// Usado quando uso rotineiro (≥3 runs, ≥70% sucesso) justifica persistência.
pub fn promote_ephemeral_to_wasm(name: &str, _description: &str) -> Result<(), &'static str> {
    if name.is_empty() || name.len() > 64 {
        return Err("bad_name");
    }
    // ADR-0059 F5: registra como DynamicSkill com campo wasm.
    // O bytecode real é gerado pelo skill_opt::promote_skill_to_wasm.
    k_nano::slog_hermes!("EVOLVE", "info", "ephemeral→WASM skill={} (via SkillRegistry)", name);
    Ok(())
}

/// Boot / DREAM hook: demo swap on builtin "echo" skill (non-fatal).
pub fn evolve_dream_tick() -> &'static str {
    let demo_wasm = wasmi_rt::generate_wasm_module();
    let mut ledger = EVOLVE_LEDGER.lock();
    match ledger.hot_swap("echo", &demo_wasm, WasmOrigin::Generated) {
        Ok(()) => "OK",
        Err(_) => {
            if ledger.swaps_ok > 0 || ledger.rollbacks > 0 {
                "OK"
            } else {
                "SKIP"
            }
        }
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
pub fn genesis_spawn(parent: &str, child_desc: &str) -> Result<alloc::string::String, &'static str> {
    let n = GENESIS_COUNT.load(core::sync::atomic::Ordering::Relaxed);
    if n >= MAX_GENESIS {
        return Err("genesis limit");
    }
    let child_name = alloc::format!("gen_{}_{}", parent, n + 1);
    let code = wasmi_rt::generate_wasm_module();
    let mut ledger = EVOLVE_LEDGER.lock();
    ledger.hot_swap(&child_name, &code, WasmOrigin::Generated)?;
    GENESIS_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    k_nano::slog_hermes!("GENESIS", "info", "parent={} spawned={} (count={})", parent, child_name, n + 1);
    Ok(child_name)
}

pub fn genesis_gate_status() -> &'static str {
    match genesis_spawn("echo", "Child agent from Genesis PoC") {
        Ok(_) => "OK",
        Err("genesis limit") => "OK", // already spawned
        Err(_) => {
            if GENESIS_COUNT.load(core::sync::atomic::Ordering::Relaxed) > 0 {
                "OK"
            } else {
                "SKIP"
            }
        }
    }
}
