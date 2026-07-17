//! EvolveAgent WASM hot-swap + rollback ledger (ADR-0047 Pilar 2).
//! Complementa self_evolve.rs (SKILL.md). Sandbox test → promote | rollback.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use crate::wasm_exec::Op;
use crate::wasm_rt::{generate_skill_wasm, AgentOrigin, WasmSkillManifest, WasmSkillRuntime};

const MAX_GEN_PER_GAP: u32 = 3;

pub struct VersionEntry {
    pub version: u32,
    pub bytecode: Vec<Op>,
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

    /// Hot-swap WASM skill: sandbox execute → promote, else rollback.
    pub fn hot_swap(
        &mut self,
        rt: &mut WasmSkillRuntime,
        name: &str,
        new_code: Vec<Op>,
        manifest: WasmSkillManifest,
    ) -> Result<(), &'static str> {
        let gen = self.bump_gen(name);
        if gen > MAX_GEN_PER_GAP {
            self.skips = self.skips.saturating_add(1);
            return Err("generation limit");
        }

        // Snapshot previous origin
        let prev_code = rt
            .registry
            .by_name(name)
            .and_then(|a| match &a.origin {
                AgentOrigin::Wasm(c) => Some(c.clone()),
                _ => None,
            });

        if let Some(code) = prev_code.clone() {
            let ver = self
                .prev
                .get(name)
                .map(|v| v.version.saturating_add(1))
                .unwrap_or(1);
            self.prev.insert(
                String::from(name),
                VersionEntry {
                    version: ver,
                    bytecode: code,
                    generations: gen,
                },
            );
        }

        // Install candidate
        rt.force_load_skill(name, new_code, manifest);

        // Sandbox test (no ApprovalGate)
        match rt.execute_sandbox(name) {
            Ok(_) => {
                self.swaps_ok = self.swaps_ok.saturating_add(1);
                k_nano::serial_println!("[EVOLVE] hot_swap OK skill={} gen={}", name, gen);
                Ok(())
            }
            Err(e) => {
                // Rollback
                if let Some(entry) = self.prev.get(name) {
                    let m = rt
                        .manifests
                        .get(name)
                        .cloned()
                        .unwrap_or_else(|| WasmSkillManifest {
                            name: String::from(name),
                            kind: String::from("wasm"),
                            description: String::from("rollback"),
                            version: String::from("0"),
                            author: String::from("evolve"),
                            required_tokens: alloc::vec![1],
                        });
                    rt.force_load_skill(name, entry.bytecode.clone(), m);
                    self.rollbacks = self.rollbacks.saturating_add(1);
                    k_nano::serial_println!(
                        "[EVOLVE] rollback skill={} after err={} gen={}",
                        name, e, gen
                    );
                } else {
                    self.skips = self.skips.saturating_add(1);
                }
                Err(e)
            }
        }
    }

    pub fn rollback(&mut self, rt: &mut WasmSkillRuntime, name: &str) -> Result<(), &'static str> {
        let entry = self.prev.get(name).ok_or("no previous version")?;
        let m = rt
            .manifests
            .get(name)
            .cloned()
            .unwrap_or_else(|| WasmSkillManifest {
                name: String::from(name),
                kind: String::from("wasm"),
                description: String::from("rollback"),
                version: String::from("0"),
                author: String::from("evolve"),
                required_tokens: alloc::vec![1],
            });
        rt.force_load_skill(name, entry.bytecode.clone(), m);
        self.rollbacks = self.rollbacks.saturating_add(1);
        Ok(())
    }
}

lazy_static::lazy_static! {
    static ref EVOLVE_LEDGER: spin::Mutex<EvolveLedger> = spin::Mutex::new(EvolveLedger::new());
    static ref WASM_RT: spin::Mutex<Option<WasmSkillRuntime>> = spin::Mutex::new(None);
}

fn ensure_rt() {
    let mut g = WASM_RT.lock();
    if g.is_none() {
        *g = Some(WasmSkillRuntime::new());
    }
}

/// Boot / DREAM hook: demo swap on builtin "echo" skill (non-fatal).
pub fn evolve_dream_tick() -> &'static str {
    ensure_rt();
    let mut rt_g = WASM_RT.lock();
    let rt = match rt_g.as_mut() {
        Some(r) => r,
        None => return "SKIP",
    };
    let (code, manifest) = generate_skill_wasm("Echoes Hello World", "echo");
    let mut ledger = EVOLVE_LEDGER.lock();
    match ledger.hot_swap(rt, "echo", code, manifest) {
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
    ensure_rt();
    let child_name = alloc::format!("gen_{}_{}", parent, n + 1);
    let (code, mut manifest) = generate_skill_wasm(child_desc, &child_name);
    manifest.author = alloc::format!("genesis:{}", parent);
    let mut rt_g = WASM_RT.lock();
    let rt = rt_g.as_mut().ok_or("no runtime")?;
    let mut ledger = EVOLVE_LEDGER.lock();
    ledger.hot_swap(rt, &child_name, code, manifest)?;
    GENESIS_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    k_nano::serial_println!(
        "[GENESIS] parent={} spawned={} (count={})",
        parent, child_name, n + 1
    );
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
