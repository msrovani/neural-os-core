//! WASM Sandbox — parser mínimo de módulos WASM.
//! Valida magic + versão, extrai exports/imports, funções disponíveis.
//! Sem dependências externas — parser manual para no_std bare-metal.

use alloc::string::String;
use alloc::string::ToString;
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
