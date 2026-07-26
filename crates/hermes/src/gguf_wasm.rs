//! GGUF via skill WASM — isolado do kernel LLM (CURRENT_MODEL).
//! SkillMarket registra `wasm_skill_name`; bytecode opcional carrega runtime estilo llama.cpp.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use skill_registry::{McpManifest, OutputSchema, Skill};

use crate::skill_market;
use crate::wasm::{register_wasm_skill, WasmSkill};

/// Metadados de um pacote GGUF exposto como skill WASM.
#[derive(Clone)]
pub struct GgufWasmPackage {
    /// Nome canônico SkillMarket / SkillRegistry (`wasm_skill_name`).
    pub wasm_skill_name: String,
    /// Path lógico no VFS / ecosystem (ex: `/mnt/neural/ecosystem/models/foo.gguf`).
    pub gguf_path: String,
    pub description: String,
    /// Bytecode WASM (llama.cpp-wasm ou stub); vazio = registro catalog-only.
    pub wasm_bytecode: Vec<u8>,
}

/// Registra skill GGUF no SkillRegistry + SkillMarket.
/// Se `wasm_bytecode` válido → `register_wasm_skill`; senão skill nativa que só declara o pacote
/// (inferência real fica no sandbox WASM quando o bytecode for fornecido).
pub fn register_gguf_wasm_skill(pkg: GgufWasmPackage) -> Result<(), &'static str> {
    let name = pkg.wasm_skill_name.clone();
    if name.is_empty() || name.len() > 64 {
        return Err("gguf_wasm: wasm_skill_name invalido");
    }
    for b in name.bytes() {
        if !(b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
            return Err("gguf_wasm: wasm_skill_name charset");
        }
    }

    if !pkg.wasm_bytecode.is_empty() {
        let desc = alloc::format!(
            "GGUF WASM skill path={} — {}",
            pkg.gguf_path,
            pkg.description
        );
        register_wasm_skill(&pkg.wasm_bytecode, &name, &desc)?;
    } else {
        let skill = GgufCatalogSkill {
            name: name.clone(),
            desc: alloc::format!(
                "GGUF catalog (WASM pending) path={} — {}",
                pkg.gguf_path,
                pkg.description
            ),
            gguf_path: pkg.gguf_path.clone(),
        };
        crate::globals::SKILL_REGISTRY.lock().register(Box::new(skill));
        k_nano::slog_hermes!(
            "GGUF",
            "info",
            "catalog skill '{}' path={} (sem bytecode — sandbox WASM deferido)",
            name,
            pkg.gguf_path
        );
    }

    skill_market::record_outcome("wasm", &name, 0, true);
    k_nano::slog_hermes!(
        "GGUF",
        "info",
        "SkillMarket wasm_skill_name={} registered",
        name
    );
    Ok(())
}

/// Conveniência: nome = `gguf_<stem>`.
pub fn register_gguf_path(gguf_path: &str, wasm_bytecode: &[u8]) -> Result<String, &'static str> {
    let stem = gguf_path
        .rsplit('/')
        .next()
        .unwrap_or(gguf_path)
        .rsplit('\\')
        .next()
        .unwrap_or(gguf_path);
    let stem = stem.split('.').next().unwrap_or(stem);
    let mut wasm_skill_name = String::from("gguf_");
    for c in stem.chars().take(48) {
        if c.is_ascii_alphanumeric() {
            wasm_skill_name.push(c.to_ascii_lowercase());
        } else {
            wasm_skill_name.push('_');
        }
    }
    register_gguf_wasm_skill(GgufWasmPackage {
        wasm_skill_name: wasm_skill_name.clone(),
        gguf_path: String::from(gguf_path),
        description: String::from("specialized GGUF via WASM"),
        wasm_bytecode: wasm_bytecode.to_vec(),
    })?;
    Ok(wasm_skill_name)
}

struct GgufCatalogSkill {
    name: String,
    desc: String,
    gguf_path: String,
}

impl Skill for GgufCatalogSkill {
    fn manifest(&self) -> McpManifest {
        McpManifest {
            name: self.name.clone(),
            description: self.desc.clone(),
            required_tokens: alloc::vec![1],
            preconditions: Vec::new(),
            context_links: alloc::vec![self.gguf_path.clone()],
            output_schema: OutputSchema::Any,
            idempotent: true,
            contracts: Vec::new(),
        }
    }

    fn verify(&self, _payload: &[u8]) -> Result<(), &'static str> {
        Ok(())
    }

    fn execute(&self, payload: &[u8]) -> Result<Vec<u8>, &'static str> {
        // Isolamento: NÃO chama Cortex CURRENT_MODEL. Resposta honesta até WASM llama chegar.
        let prompt = core::str::from_utf8(payload).unwrap_or("");
        let msg = alloc::format!(
            "[gguf_wasm] skill={} path={} — inference deferred to WASM llama.cpp sandbox (prompt_len={})",
            self.name,
            self.gguf_path,
            prompt.len()
        );
        skill_market::record_outcome("wasm", &self.name, 1, true);
        Ok(msg.into_bytes())
    }
}

/// Re-export tipo para quem registra bytecode tipado.
pub type GgufWasmSkill = WasmSkill;






