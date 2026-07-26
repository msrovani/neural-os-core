//! BitNet IDE (IDEA #394) — IDE on-device com Cortex-assisted code generation.
//!
//! "Descreva uma skill" → LLM gera WAT → compila WASM → registra no ecossistema.
//!
//! AIOS na veia: crie software no próprio OS, sem compilador cruzado.
//!
//! ## Fluxo
//! 1. `set_description("uma skill que soma dois números")`
//! 2. `generate_skill()` → LLM gera WAT → WASM assembly → log + registro
//! 3. `IdeResult::Success { skill_name, wasm_size }`
//!
//! ## Pendente (ponytail)
//! - LLM real em vez de WAT hardcoded
//! - wat2wasm real em vez de WASM hardcoded
//! - PackageHub flow completo com SKILL.md assinado

#![allow(dead_code)]

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

/// Resultado da geração de uma skill.
#[derive(Debug, Clone)]
pub enum IdeResult {
    Success {
        skill_name: String,
        wasm_size: usize,
    },
    Failed {
        error: String,
    },
}

/// Motor da IDE — estado e pipeline de geração.
pub struct BitNetIde {
    /// Descrição textual da skill a ser gerada.
    pub buffer: String,
}

impl BitNetIde {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
        }
    }

    /// Define a descrição da skill.
    pub fn set_description(&mut self, desc: &str) {
        self.buffer = String::from(desc);
    }

    /// Gera uma skill a partir da descrição no buffer.
    ///
    /// 1. Chama Cortex LLM para gerar WAT
    /// 2. Monta WASM a partir do WAT
    /// 3. Registra no ecossistema via PackageHub
    pub fn generate_skill(&mut self) -> IdeResult {
        let description = self.buffer.clone();
        if description.is_empty() {
            return IdeResult::Failed {
                error: String::from("Empty description"),
            };
        }

        // Step 1: LLM gera WAT a partir da descrição
        let wat = self.llm_generate_wat(&description);

        // Step 2: WAT → WASM
        let wasm = self.assemble_wat(&wat);

        // Step 3: Registra no ecossistema
        let skill_name = self.sanitize_name(&description);
        self.register_skill(&skill_name, &wasm);

        IdeResult::Success {
            skill_name,
            wasm_size: wasm.len(),
        }
    }

    /// LLM gera código WAT a partir de descrição em linguagem natural.
    // ponytail: hardcoded demo WAT; real impl chama cortex LLM
    fn llm_generate_wat(&self, _description: &str) -> Vec<u8> {
        br#"(module (func (export "add") (param i32 i32) (result i32) local.get 0 local.get 1 i32.add))"#.to_vec()
    }

    /// Monta WAT → WASM binário.
    // ponytail: hardcoded WASM mínimo (module return 42); real impl precisa wat2wasm
    fn assemble_wat(&self, _wat: &[u8]) -> Vec<u8> {
        vec![
            0x00, 0x61, 0x73, 0x6d, // magic \0asm
            0x01, 0x00, 0x00, 0x00, // version 1
            0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f, // type section: () -> i32
            0x03, 0x02, 0x01, 0x00, // function section: 1 func
            0x0a, 0x06, 0x01, 0x04, 0x00, 0x41, 0x2a, 0x0b, // code: return 42
        ]
    }

    /// Registra a skill gerada no ecossistema.
    // ponytail: log-only — real impl precisa SKILL.md assinado + PackageHub approval flow
    fn register_skill(&self, name: &str, wasm: &[u8]) {
        k_nano::slog_jarbas!(
            "IDE", "info",
            "skill '{}' generated ({} bytes) — pending full PackageHub registration",
            name, wasm.len()
        );
    }

    /// Sanitiza nome para até 20 caracteres alfanuméricos.
    fn sanitize_name(&self, desc: &str) -> String {
        desc.chars()
            .take(20)
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_empty() {
        let mut ide = BitNetIde::new();
        let result = ide.generate_skill();
        match result {
            IdeResult::Failed { error } => assert_eq!(error, "Empty description"),
            _ => panic!("expected Failed for empty"),
        }
    }

    #[test]
    fn test_generate_success() {
        let mut ide = BitNetIde::new();
        ide.set_description("soma dois números");
        let result = ide.generate_skill();
        match result {
            IdeResult::Success { skill_name, wasm_size } => {
                assert!(skill_name.contains("soma"));
                assert!(wasm_size > 0);
            }
            _ => panic!("expected Success"),
        }
    }

    #[test]
    fn test_sanitize_name() {
        let ide = BitNetIde::new();
        assert_eq!(ide.sanitize_name("hello world!"), "hello_world_");
        assert_eq!(ide.sanitize_name("abc"), "abc");
        assert_eq!(ide.sanitize_name("a very long name here!!!!").len(), 20);
    }
}
