use alloc::string::String;
use alloc::vec::Vec;

pub struct WasmSandbox {
    pub loaded_modules: Vec<WasmModule>,
    pub max_memory_pages: u32,
}

pub struct WasmModule {
    pub name: String,
    pub bytecode: Vec<u8>,
    pub exports: Vec<String>,
}

impl WasmSandbox {
    pub fn new() -> Self {
        WasmSandbox { loaded_modules: Vec::new(), max_memory_pages: 256 }
    }

    pub fn load(&mut self, name: &str, bytecode: &[u8]) -> Result<(), &'static str> {
        if bytecode.len() < 8 || bytecode[0] != 0x00 || bytecode[1] != 0x61
            || bytecode[2] != 0x73 || bytecode[3] != 0x6D {
            return Err("Invalid WASM magic header");
        }
        let exports = self.scan_exports(bytecode);
        self.loaded_modules.push(WasmModule {
            name: String::from(name),
            bytecode: bytecode.to_vec(),
            exports,
        });
        Ok(())
    }

    pub fn execute(&self, _module: &str, _func: &str, _args: &[u8]) -> Result<Vec<u8>, &'static str> {
        // Stub: no real WASM runtime yet (requires wasmi crate)
        Err("WASM runtime not available — wasmi crate pending")
    }

    fn scan_exports(&self, bytecode: &[u8]) -> Vec<String> {
        let mut exports = Vec::new();
        let mut i = 8;
        while i + 1 < bytecode.len() {
            if bytecode[i] == 0x07 {
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

    pub fn module_count(&self) -> usize { self.loaded_modules.len() }
}
