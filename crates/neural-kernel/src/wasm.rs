//! WASM App Sandbox — execução isolada de apps de terceiros.
//! Stub: carrega bytecode WASM, executa eventos, retorna resultados.
//! Quando wasmi estiver disponivel, substituir stubs por interpretador real.

use alloc::string::String;
use alloc::vec::Vec;
use alloc::vec;

pub struct WasmApp {
    pub name: String,
    pub bytecode: Vec<u8>,
    pub memory: Vec<u8>,
    pub exports: Vec<String>,
}

/// Sandbox que gerencia modulos WASM
pub struct WasmSandbox {
    pub apps: Vec<WasmApp>,
    pub max_memory_pages: u32,
}

impl WasmSandbox {
    pub fn new() -> Self {
        WasmSandbox { apps: Vec::new(), max_memory_pages: 256 }
    }

    /// Carrega um modulo WASM a partir de bytes
    pub fn load(&mut self, name: &str, bytecode: &[u8]) -> Result<u32, &'static str> {
        if bytecode.len() < 8 || bytecode[0] != 0x00 || bytecode[1] != 0x61
            || bytecode[2] != 0x73 || bytecode[3] != 0x6D {
            return Err("Invalid WASM magic header");
        }
        let exports = self.scan_exports(bytecode);
        let app_id = self.apps.len() as u32;
        let mem_size = (self.max_memory_pages as usize) * 65536;
        self.apps.push(WasmApp {
            name: String::from(name),
            bytecode: bytecode.to_vec(),
            memory: vec![0u8; mem_size],
            exports,
        });
        Ok(app_id)
    }

    /// Executa uma funcao exportada do modulo (stub)
    pub fn execute(&mut self, app_id: u32, func: &str, args: &[u8]) -> Result<Vec<u8>, &'static str> {
        let app = self.apps.get(app_id as usize).ok_or("App not found")?;
        if !app.exports.iter().any(|e| e == func) {
            return Err("Function not exported");
        }
        // Stub: retorna os argumentos como resposta (echo)
        // No futuro: interpretar bytecode WASM via wasmi
        Ok(args.to_vec())
    }

    fn scan_exports(&self, bytecode: &[u8]) -> Vec<String> {
        let mut exports = Vec::new();
        let mut i = 8;
        while i + 1 < bytecode.len() {
            if bytecode[i] == 0x07 { // export section
                i += 1;
                let count = bytecode[i] as usize;
                i += 1;
                for _ in 0..count {
                    if i + 1 >= bytecode.len() { break; }
                    let name_len = bytecode[i] as usize;
                    i += 1;
                    if i + name_len > bytecode.len() { break; }
                    if let Ok(name) = core::str::from_utf8(&bytecode[i..i + name_len]) {
                        exports.push(String::from(name));
                    }
                    i += name_len + 2;
                }
                break;
            }
            i += 1;
        }
        exports
    }

    pub fn app_count(&self) -> usize { self.apps.len() }
}
