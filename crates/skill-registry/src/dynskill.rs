//! DynamicSkill — skill gerada por LLM (texto) ou promovida com bytecode WASM (ADR-0059 F5).
//! Honesty (SESSION_377): `wasm: Some` **não** executa wasmi aqui — fail-closed até bridge.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use crate::mcp::McpManifest;
use crate::skill::Skill;

/// Token reservado p/ promoção CapGate futura (hoje Hermes EventBus usa Legacy(1)).
pub const DYNSKILL_TOKEN: u64 = 0xD1;

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
                // Legacy(1) = path Hermes EventBus. HITL escalate em promote — não é CapGate real
                // (ADR-0052). DYNSKILL_TOKEN reservado p/ migração CapGate.
                required_tokens: vec![1],
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
                required_tokens: vec![1],
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
        if self.wasm.is_some() {
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
    fn wasm_skill_fails_closed() {
        let s = DynamicSkill::with_wasm("x", "d", "i", vec![0, 0x61, 0x73, 0x6d]);
        assert!(s.wasm.is_some());
        assert_eq!(s.execute(b"hi"), Err("wasm_runtime_unwired"));
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
    fn dynskill_exports_reserved_token() {
        assert_eq!(DYNSKILL_TOKEN, 0xD1);
        let s = DynamicSkill::new("a", "b", "c");
        assert_eq!(s.manifest().required_tokens, vec![1]);
    }
}
