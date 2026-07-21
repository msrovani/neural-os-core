use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use crate::mcp::McpManifest;
use crate::skill::Skill;

/// ADR-0059 F5: DynamicSkill com campo `wasm` opcional para hot-promote.
pub struct DynamicSkill {
    manifest: McpManifest,
    instructions: String,
    /// Bytecode WASM compilado (wasmi); `Some` quando promovido de efêmero.
    pub wasm: Option<Vec<u8>>,
}

impl DynamicSkill {
    pub fn new(name: &str, description: &str, instructions: &str) -> Self {
        DynamicSkill {
            manifest: McpManifest {
                name: String::from(name),
                description: String::from(description),
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
            contracts: Vec::new(),
        }
    }

    fn execute(&self, payload: &[u8]) -> Result<Vec<u8>, &'static str> {
        let input = core::str::from_utf8(payload).unwrap_or("");
        let result = alloc::format!(
            "[{}] instrucao: {}\n  input: {}",
            self.manifest.name, self.instructions, input
        );
        Ok(result.into_bytes())
    }
}
