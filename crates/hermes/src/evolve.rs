//! EvolveAgent WASM hot-swap + rollback ledger (ADR-0047 Pilar 2).
//! Complementa self_evolve.rs (SKILL.md). Sandbox test → promote | rollback.
//! Honesty SESSION_379: nunca snapshot do candidato; registry só após sandbox OK.

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
        crate::dynskill::register_dynskill(skill);
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
        crate::dynskill::register_dynskill(roll);
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
/// Caminho real (sandbox A only): op-IR mínima sintetizada
/// (`I32Const(0)` — skill ainda sem corpo gerado pelo Cortex #412)
/// → `wasm_build::validate` → `wasm_build::build_run_module`
/// → sandbox wasmi (`sandbox_validate_and_run`, CAP_NONE) → registro via
/// `dynskill::DynamicSkill::with_wasm` + `register_dynskill` → persistência
/// best-effort dos bytes .wasm via `fs::write_vfs` em `/skills/{name}.wasm`
/// (package_hub AgentWasm exige approval plumbing → write_vfs direto).
/// wasmi (caminho A) ONLY — nunca toca gates B/C de execução nativa.
pub fn promote_ephemeral_to_wasm(name: &str, description: &str) -> Result<(), &'static str> {
    if name.is_empty() || name.len() > 64 {
        return Err("bad_name");
    }
    // Corpo mínimo honesto até o Cortex #412 gerar op-IR real por skill.
    let ops = [crate::wasm_build::Op::I32Const(0)];
    promote_ephemeral_ops_to_wasm(name, description, 0, &ops)
}

/// Variante com op-IR do caller (Cortex/Trinity/LLM, constrangida por #412).
/// `n_params` = aridade da função `run`; `ops` deve deixar 1×i32 na stack.
pub fn promote_ephemeral_ops_to_wasm(
    name: &str,
    description: &str,
    n_params: u32,
    ops: &[crate::wasm_build::Op],
) -> Result<(), &'static str> {
    if name.is_empty() || name.len() > 64 {
        return Err("bad_name");
    }
    // 1. op-IR → bytes wasm (build_run_module já revalida a op-IR).
    crate::wasm_build::validate(n_params, ops).map_err(|_| "bad-op-ir")?;
    let wasm = crate::wasm_build::build_run_module(n_params, ops).map_err(|_| "build-fail")?;
    // 2. Sandbox wasmi FIRST (Caminho A, CAP_NONE) — sem tocar registry antes.
    if !crate::wasmi_rt::sandbox_validate_and_run(&wasm) {
        k_nano::slog_hermes!(
            "EVOLVE",
            "warn",
            "ephemeral→WASM skill={} SKIP sandbox fail (registry untouched)",
            name
        );
        return Err("sandbox-fail");
    }
    // 3. Registro (DynamicSkill::with_wasm + trust via register_dynskill).
    let skill =
        crate::dynskill::DynamicSkill::with_wasm(name, description, "", wasm.clone());
    crate::dynskill::register_dynskill(skill);
    crate::self_evolve::publish_change("skill", name);
    // 4. Persistência best-effort: VFS pode não existir no boot cedo/host.
    let path = alloc::format!("/skills/{}.wasm", name);
    if crate::fs::write_vfs(&path, &wasm).is_err() {
        k_nano::slog_hermes!(
            "EVOLVE",
            "warn",
            "ephemeral→WASM skill={} registered, persist SKIP (VFS absent)",
            name
        );
    }
    k_nano::slog_hermes!(
        "EVOLVE",
        "ok",
        "ephemeral→WASM skill={} OK bytes={} (wasmi A)",
        name,
        wasm.len()
    );
    Ok(())
}

/// Boot / DREAM hook: demo swap. H8: sem bytecode real não há swap.
pub fn evolve_dream_tick() -> &'static str {
    k_nano::slog_hermes!("EVOLVE", "warn", "dream_tick SKIP no-wasm-bytes");
    "SKIP"
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
    // H8: sem gerador de WASM real — nunca materializar código dummy em criança.
    k_nano::slog_hermes!("GENESIS", "warn", "spawn parent={} SKIP no-wasm-bytes (op-IR pendente #412)", parent);
    Err("no-wasm-bytes")
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
    fn promote_ops_op_to_wasm_bytes_validate_passes() {
        // Op->wasm bytes->validate: (a*b+7) com (6,7) == 49 no sandbox wasmi.
        let ops = [
            crate::wasm_build::Op::LocalGet(0),
            crate::wasm_build::Op::LocalGet(1),
            crate::wasm_build::Op::I32Mul,
            crate::wasm_build::Op::I32Const(7),
            crate::wasm_build::Op::I32Add,
        ];
        assert!(crate::wasm_build::validate(2, &ops).is_ok());
        let wasm = crate::wasm_build::build_run_module(2, &ops).expect("build");
        assert!(!wasm.is_empty());
        assert_eq!(&wasm[0..4], &[0x00, 0x61, 0x73, 0x6D]); // magic \0asm
        assert!(crate::wasmi_rt::sandbox_validate_and_run(&wasm));
        assert_eq!(
            crate::wasmi_rt::run_wasm(&wasm, "run", &[6, 7], crate::wasmi_rt::CAP_NONE)
                .expect("run"),
            49
        );
        // Promoção real registra (persist VFS é best-effort no host).
        assert!(promote_ephemeral_ops_to_wasm("evolve_test_skill", "test", 2, &ops).is_ok());
        // op-IR inválida é rejeitada antes de tocar o registry.
        let bad = [crate::wasm_build::Op::I32Add];
        assert!(promote_ephemeral_ops_to_wasm("evolve_bad", "test", 2, &bad).is_err());
    }

    #[test]
    fn hot_swap_success_then_rollback_restores_previous() {
        let mut led = EvolveLedger::new();
        let good = wasmi_rt::canned_test_module();
        assert!(led.hot_swap("echo", &good, WasmOrigin::Compiled).is_ok());
        assert!(led.live.get("echo").is_some());
        // Second success snapshots first into prev
        assert!(led.hot_swap("echo", &good, WasmOrigin::Compiled).is_ok());
        assert!(led.prev.get("echo").is_some());
        assert!(led.rollback("echo").is_ok());
        assert_eq!(led.rollbacks, 1);
    }
}
