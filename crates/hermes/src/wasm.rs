//! WASM Bridge — ADR-0059 F3: roteia `register_wasm_skill` → `wasmi_rt`.
//!
//! ## Deprecação
//! O interpretador `WasmExecutor` (stack-based, ~30 opcodes) foi **substituído**
//! pelo `wasmi` (v0.47, no_std, fuel) — ADR-0059 Caminho A. O `parse_wasm` e
//! metadados permanecem para compatibilidade; `register_wasm_skill` agora cria
//! um `WasmSkill` cujo `execute()` chama `wasmi_rt::run_wasm` em vez do executor
//! custom. `WasmExecutor` mantido como referência histórica (`#[cfg(feature="legacy-wasm")]`).
//!
//! Migração completa (aposentar `wasm_exec.rs` + `wasm.rs` → só `wasmi_rt.rs`) = ADR-0059 limpeza residual.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

const WASM_MAGIC: [u8; 4] = [0x00, 0x61, 0x73, 0x6D]; // \0asm
const WASM_VERSION: [u8; 4] = [0x01, 0x00, 0x00, 0x00];

#[derive(Debug, Clone)]
pub struct WasmExport {
    pub name: String,
    pub kind: u8, // 0=func, 1=table, 2=mem, 3=global
    pub index: u32,
}

#[derive(Debug, Clone)]
pub struct WasmModule {
    pub functions: u32,
    pub exports: Vec<WasmExport>,
}

/// Parseia cabeçalho WASM e tabela de exports
pub fn parse_wasm(bytecode: &[u8]) -> Result<WasmModule, &'static str> {
    if bytecode.len() < 8 {
        return Err("Wasm too short");
    }
    if bytecode[0..4] != WASM_MAGIC {
        return Err("Invalid WASM magic");
    }
    if bytecode[4..8] != WASM_VERSION {
        return Err("Unsupported WASM version");
    }

    let mut off = 8u32;
    let mut functions = 0u32;
    let mut exports = Vec::new();

    while (off as usize) < bytecode.len() {
        let section_id = bytecode[off as usize];
        off += 1;
        if off as usize + 4 > bytecode.len() { break; }
        let section_len = u32::from_le_bytes([
            bytecode[off as usize],
            bytecode[off as usize + 1],
            bytecode[off as usize + 2],
            bytecode[off as usize + 3],
        ]);
        off += 4;

        let section_end = off + section_len;
        if section_end as usize > bytecode.len() { break; }

        match section_id {
            1 => { /* Type section */ }
            3 => { // Function section
                if (off as usize) < bytecode.len() {
                    functions = bytecode[off as usize] as u32;
                }
            }
            7 => { // Export section
                if off as usize >= bytecode.len() { break; }
                let count = bytecode[off as usize] as usize;
                off += 1;
                for _ in 0..count {
                    if off as usize + 1 > bytecode.len() { break; }
                    let name_len = bytecode[off as usize] as usize;
                    off += 1;
                    if off as usize + name_len > bytecode.len() { break; }
                    let name = core::str::from_utf8(&bytecode[off as usize..off as usize + name_len])
                        .unwrap_or("?")
                        .to_string();
                    off += name_len as u32;
                    if off as usize + 2 > bytecode.len() { break; }
                    let kind = bytecode[off as usize];
                    let index = u32::from_le_bytes([
                        bytecode[off as usize],
                        bytecode[off as usize + 1],
                        bytecode[off as usize + 2],
                        bytecode[off as usize + 3],
                    ]);
                    off += 2;
                    if kind == 0 {
                        exports.push(WasmExport { name, kind, index });
                    }
                }
            }
            _ => {}
        }
        off = section_end;
    }

    Ok(WasmModule { functions, exports })
}

/// Verifica se um nome de função é exportada no módulo
pub fn has_export(module: &WasmModule, name: &str) -> bool {
    module.exports.iter().any(|e| e.name == name)
}

/// Lista exports como string (para debug / /learn)
pub fn list_exports(module: &WasmModule) -> String {
    let mut out = String::from("Exports:\n");
    for e in &module.exports {
        out.push_str(&alloc::format!("  {} (func {})\n", e.name, e.index));
    }
    out
}

// ---------------------------------------------------------------------------
// Bridge wasmi_rt — `register_wasm_skill` agora executa via wasmi real
// ---------------------------------------------------------------------------

use crate::wasmi_rt;

// ---------------------------------------------------------------------------
// WASI→Skill Bridge
// ---------------------------------------------------------------------------

struct WasmSkillBridge {
    skill_name: String,
    registered: bool,
}

static WASM_SKILL_BRIDGE: spin::Mutex<WasmSkillBridge> = spin::Mutex::new(WasmSkillBridge {
    skill_name: String::new(),
    registered: false,
});

/// Registra uma skill WASM no SkillRegistry
pub fn register_wasm_skill(bytecode: &[u8], name: &str, desc: &str) -> Result<(), &'static str> {
    let module = parse_wasm(bytecode)?;
    k_nano::slog_hermes!("Wasm", "info", "Registrando '{}' ({} exports)...", name, module.exports.len());

    let skill = WasmSkill::new(bytecode, name, desc, module.exports.clone());
    crate::globals::SKILL_REGISTRY.lock().register(Box::new(skill));
    {
        let mut bridge = WASM_SKILL_BRIDGE.lock();
        bridge.skill_name = String::from(name);
        bridge.registered = true;
    }
    k_nano::slog_hermes!("Wasm", "info", "Skill '{}' registrada com {} exports.", name, module.exports.len());
    Ok(())
}

/// Skill que executa WASM bytecode via wasmi real.
pub struct WasmSkill {
    bytecode: Vec<u8>,
    name: String,
    desc: String,
    exports: Vec<WasmExport>,
}

impl WasmSkill {
    pub fn new(bytecode: &[u8], name: &str, desc: &str, exports: Vec<WasmExport>) -> Self {
        WasmSkill {
            bytecode: bytecode.to_vec(),
            name: String::from(name),
            desc: String::from(desc),
            exports,
        }
    }
}

/// Heurística para converter payload em argumentos i32 para WASM.
/// Tenta: parse como int, depois byte len, depois 0.
fn payload_to_args(payload: &[u8]) -> Vec<i32> {
    if payload.is_empty() {
        vec![0]
    } else if let Ok(text) = core::str::from_utf8(payload) {
        match text.trim().parse::<i32>() {
            Ok(n) => vec![n],
            Err(_) => vec![payload.len() as i32],
        }
    } else {
        vec![payload.len() as i32]
    }
}

impl skill_registry::Skill for WasmSkill {
    fn manifest(&self) -> skill_registry::McpManifest {
        skill_registry::McpManifest {
            name: self.name.clone(),
            description: self.desc.clone(),
            required_tokens: vec![1],
            preconditions: Vec::new(),
            context_links: Vec::new(),
            output_schema: skill_registry::OutputSchema::Any,
            idempotent: false,
            contracts: Vec::new(),
        }
    }

    fn verify(&self, _payload: &[u8]) -> Result<(), &'static str> {
        // Verifica se o bytecode WASM é válido pelo wasmi
        let mut c = wasmi::Config::default();
        c.consume_fuel(true);
        wasmi::Module::new(&wasmi::Engine::new(&c), &self.bytecode)
            .map(|_| ())
            .map_err(|_| "WASM: bytecode inválido")
    }

    fn execute(&self, payload: &[u8]) -> Result<Vec<u8>, &'static str> {
        // Tenta "main" primeiro, depois "_start", depois primeira export
        let func_name = self.exports.iter().find(|e| e.name == "main" || e.name == "_start")
            .or_else(|| self.exports.first())
            .map(|e| &e.name[..])
            .unwrap_or("");

        if func_name.is_empty() {
            return Err("WASM: nenhuma função exportada");
        }

        let args = payload_to_args(payload);
        // caps: skill padrão → 1 (skill token)
        match wasmi_rt::run_wasm(&self.bytecode, func_name, &args, 1) {
            Ok(result) => Ok(alloc::format!("[WASM] {} → {}", func_name, result).into_bytes()),
            Err(e) => {
                // ponytail: fallback para 0 args se main() não aceitar argumento
                if func_name == "main" || func_name == "_start" {
                    wasmi_rt::run_wasm(&self.bytecode, func_name, &[], 1)
                        .map(|r| alloc::format!("[WASM] {} → {}", func_name, r).into_bytes())
                        .map_err(|_| e)
                } else {
                    Err(e)
                }
            }
        }
    }
}
