//! DynamicSkill — skill gerada por LLM (texto) ou promovida com bytecode WASM (ADR-0059 F5).
//! Honesty (SESSION_377): `wasm: Some` **não** executa wasmi aqui — fail-closed até bridge.
//! CapGate (SESSION_383 / IDEA #600): `required_tokens = [DYNSKILL_TOKEN]` (0xD1), nunca Legacy(1).

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use crate::mcp::McpManifest;
use crate::skill::Skill;

/// Token CapGate para DynamicSkill / mesh promote (≠ Legacy(1) do EventBus de sistema).
pub const DYNSKILL_TOKEN: u64 = 0xD1;

/// Lane B: ponte de execução WASM (sem dependência circular).
/// `skill-registry` é folha (só depende de `event-bus`) e não pode chamar o
/// wasmi do hermes. O hermes instala aqui o executor real via
/// `install_wasm_exec_bridge` (lazy, no `register_wasm_skill*`); sem bridge,
/// `execute` com `wasm` continua fail-closed (`wasm_runtime_unwired`).
pub type WasmExecFn = fn(wasm: &[u8], payload: &[u8]) -> Result<Vec<u8>, &'static str>;

static WASM_EXEC_BRIDGE: AtomicUsize = AtomicUsize::new(0);

/// Instala o executor WASM real (hermes wasmi). Idempotente.
pub fn install_wasm_exec_bridge(f: WasmExecFn) {
    WASM_EXEC_BRIDGE.store(f as usize, Ordering::SeqCst);
}

fn wasm_exec_bridge() -> Option<WasmExecFn> {
    let v = WASM_EXEC_BRIDGE.load(Ordering::SeqCst);
    if v == 0 {
        None
    } else {
        // SAFETY: só armazenado via install_wasm_exec_bridge (WasmExecFn).
        Some(unsafe { core::mem::transmute::<usize, WasmExecFn>(v) })
    }
}

/// ADR-0059 F5: DynamicSkill com campo `wasm` opcional para hot-promote.
pub struct DynamicSkill {
    manifest: McpManifest,
    instructions: String,
    /// Bytecode WASM; execução exige runtime bridge (ainda não wired nesta crate).
    pub wasm: Option<Vec<u8>>,
}

impl DynamicSkill {
    pub fn new(name: &str, description: &str, instructions: &str) -> Self {
        DynamicSkill {
            manifest: McpManifest {
                name: String::from(name),
                description: String::from(description),
                required_tokens: vec![DYNSKILL_TOKEN],
                preconditions: Vec::new(),
                context_links: Vec::new(),
                output_schema: crate::OutputSchema::String,
                idempotent: false,
                contracts: Vec::new(),
            },
            instructions: String::from(instructions),
            wasm: None,
        }
    }

    /// ADR-0059 F5: cria DynamicSkill com bytecode WASM (promoted from ephemeral).
    /// Execute permanece fail-closed até runtime bridge / hermes wasmi.
    pub fn with_wasm(name: &str, description: &str, instructions: &str, wasm: Vec<u8>) -> Self {
        DynamicSkill {
            manifest: McpManifest {
                name: String::from(name),
                description: String::from(description),
                required_tokens: vec![DYNSKILL_TOKEN],
                preconditions: Vec::new(),
                context_links: Vec::new(),
                output_schema: crate::OutputSchema::Any,
                idempotent: false,
                contracts: Vec::new(),
            },
            instructions: String::from(instructions),
            wasm: Some(wasm),
        }
    }
}

impl Skill for DynamicSkill {
    fn manifest(&self) -> McpManifest {
        McpManifest {
            name: self.manifest.name.clone(),
            description: self.manifest.description.clone(),
            required_tokens: self.manifest.required_tokens.clone(),
            preconditions: self.manifest.preconditions.clone(),
            context_links: self.manifest.context_links.clone(),
            output_schema: self.manifest.output_schema.clone(),
            idempotent: self.manifest.idempotent,
            contracts: self.manifest.contracts.clone(),
        }
    }

    fn execute(&self, payload: &[u8]) -> Result<Vec<u8>, &'static str> {
        if let Some(wasm) = self.wasm.as_ref() {
            // Lane B: com bridge instalado (hermes wasmi) executa de verdade;
            // sem bridge, fail-closed honesto (SESSION_377 / ADR-0059 F5).
            if let Some(run) = wasm_exec_bridge() {
                return run(wasm, payload);
            }
            // Never fake wasmi success — AIOS honesty (SESSION_377 / ADR-0059 F5).
            return Err("wasm_runtime_unwired");
        }
        let input = core::str::from_utf8(payload).unwrap_or("");
        let result = alloc::format!(
            "[{}] instrucao: {}\n  input: {}",
            self.manifest.name, self.instructions, input
        );
        Ok(result.into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Skill;

    #[test]
    fn wasm_exec_bridge_delegates_when_installed() {
        fn fake_exec(_wasm: &[u8], payload: &[u8]) -> Result<Vec<u8>, &'static str> {
            Ok(alloc::format!("fake:{}", payload.len()).into_bytes())
        }
        install_wasm_exec_bridge(fake_exec);
        let s = DynamicSkill::with_wasm("y", "d", "i", vec![0, 0x61, 0x73, 0x6d]);
        let out = s.execute(b"hi").expect("bridge deve delegar");
        assert_eq!(out, b"fake:2");
        // ponytail: bridge é monotônico (sem uninstall) — demais testes neste
        // binário não instalam bridge nem dependem de fail-closed após este.
    }

    #[test]
    fn text_skill_ok() {
        let s = DynamicSkill::new("echo_dyn", "d", "say hi");
        let out = s.execute(b"yo").unwrap();
        let t = core::str::from_utf8(&out).unwrap();
        assert!(t.contains("instrucao"));
        assert!(t.contains("yo"));
    }

    #[test]
    fn dynskill_requires_capgate_token_not_legacy_one() {
        assert_eq!(DYNSKILL_TOKEN, 0xD1);
        let s = DynamicSkill::new("a", "b", "c");
        assert_eq!(s.manifest().required_tokens, vec![DYNSKILL_TOKEN]);
        let w = DynamicSkill::with_wasm("w", "d", "i", vec![0]);
        assert_eq!(w.manifest().required_tokens, vec![DYNSKILL_TOKEN]);
    }
}
