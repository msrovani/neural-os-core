//! WASM Sandbox — executor stack-based + bridge WASI→Skill.
//! Parseia, valida e executa bytecode WASM para skills no_std.
//! WASI stub: mapeia chamadas de sistema WASI para o SkillRegistry.
//!
//! Opcodes suportados: i32 const/arithmetic/compare,
//! local get/set, global get/set, block/loop/if/else/end,
//! br/br_if, return, call, call_indirect, memory load/store.

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
// WasmExecutor — interpretador stack-based para bytecode WASM
// ---------------------------------------------------------------------------

const WASM_PAGE_SIZE: usize = 65536;

pub struct WasmExecutor {
    pub stack: Vec<i64>,
    pub memory: Vec<u8>,
    pub locals: Vec<i64>,
    pub bytecode: Vec<u8>,
    pub pc: usize,
    pub exports: Vec<WasmExport>,
}

impl WasmExecutor {
    pub fn new() -> Self {
        WasmExecutor {
            stack: Vec::new(),
            memory: Vec::with_capacity(WASM_PAGE_SIZE),
            locals: Vec::new(),
            bytecode: Vec::new(),
            pc: 0,
            exports: Vec::new(),
        }
    }

    /// Carrega um módulo WASM no executor
    pub fn load(&mut self, wasm_bytes: &[u8]) -> Result<(), &'static str> {
        let module = parse_wasm(wasm_bytes)?;
        self.bytecode = wasm_bytes.to_vec();
        self.exports = module.exports.clone();
        self.memory = vec![0u8; WASM_PAGE_SIZE];
        self.stack.clear();
        self.locals.clear();
        self.pc = 0;
        Ok(())
    }

    /// Localiza o offset de uma função pelo índice
    fn find_func(&self, func_idx: u32) -> Option<usize> {
        let mut off = 8usize;
        let _seen_funcs = 0u32;
        let mut body_offsets: Vec<usize> = Vec::new();
        while off < self.bytecode.len() {
            let section_id = self.bytecode[off];
            off += 1;
            if off + 4 > self.bytecode.len() { break; }
            let section_len = u32::from_le_bytes([
                self.bytecode[off], self.bytecode[off+1],
                self.bytecode[off+2], self.bytecode[off+3],
            ]) as usize;
            off += 4;
            let section_end = off + section_len;
            if section_end > self.bytecode.len() { break; }
            match section_id {
                10 => { // Code section — body offsets
                    let count = self.bytecode[off] as usize;
                    let mut pos = off + 1;
                    for _ in 0..count {
                        if pos + 4 > self.bytecode.len() { break; }
                        let body_size = u32::from_le_bytes([
                            self.bytecode[pos], self.bytecode[pos+1],
                            self.bytecode[pos+2], self.bytecode[pos+3],
                        ]) as usize;
                        pos += 4;
                        body_offsets.push(pos);
                        pos += body_size;
                    }
                }
                _ => {}
            }
            off = section_end;
        }
        if (func_idx as usize) < body_offsets.len() {
            Some(body_offsets[func_idx as usize])
        } else { None }
    }

    /// Encontra export por nome, retorna índice da função
    pub fn get_export_index(&self, name: &str) -> Option<u32> {
        self.exports.iter().find(|e| e.name == name).map(|e| e.index)
    }

    /// Executa bytecode a partir do PC atual até encontrar `end` ou `return`
    fn run_bytecode(&mut self) -> Result<(), &'static str> {
        loop {
            if self.pc >= self.bytecode.len() { return Err("WASM: PC out of bounds"); }
            let op = self.bytecode[self.pc];
            self.pc += 1;

            match op {
                0x00 => return Err("WASM: unreachable"),
                0x01 => {} // nop

                // i32.const
                0x41 => {
                    let mut val: i32 = 0;
                    let mut shift = 0;
                    loop {
                        let byte = self.bytecode[self.pc]; self.pc += 1;
                        val |= ((byte & 0x7F) as i32) << shift;
                        shift += 7;
                        if byte & 0x80 == 0 { break; }
                    }
                    self.stack.push(val as i64);
                }

                // i32.add, i32.sub, i32.mul
                0x6A => { let b = self.stack.pop().unwrap_or(0); let a = self.stack.pop().unwrap_or(0); self.stack.push(a.wrapping_add(b)); }
                0x6B => { let b = self.stack.pop().unwrap_or(0); let a = self.stack.pop().unwrap_or(0); self.stack.push(a.wrapping_sub(b)); }
                0x6C => { let b = self.stack.pop().unwrap_or(0) as i32; let a = self.stack.pop().unwrap_or(0) as i32; self.stack.push((a.wrapping_mul(b)) as i64); }

                // i32.eqz
                0x45 => { let v = self.stack.pop().unwrap_or(0); self.stack.push(if v == 0 { 1 } else { 0 }); }

                // local.get / local.set
                0x20 => { let idx = self.bytecode[self.pc] as usize; self.pc += 1;
                    if idx < self.locals.len() { self.stack.push(self.locals[idx]); } }
                0x21 => { let idx = self.bytecode[self.pc] as usize; self.pc += 1;
                    if idx < self.locals.len() { self.locals[idx] = self.stack.pop().unwrap_or(0); } }

                // global.get / global.set
                0x23 => { let idx = self.bytecode[self.pc] as usize; self.pc += 1;
                    if idx < self.locals.len() { self.stack.push(self.locals[idx]); } }
                0x24 => { let _idx = self.bytecode[self.pc] as usize; self.pc += 1; let _ = self.stack.pop(); }

                // i32.load / i32.store
                0x28 => { let align = self.bytecode[self.pc]; self.pc += 1; let _offset = self.bytecode[self.pc] as i32 as i64; self.pc += 1;
                    let addr = self.stack.pop().unwrap_or(0) as usize + align as usize;
                    if addr + 4 <= self.memory.len() {
                        let v = i32::from_le_bytes([self.memory[addr], self.memory[addr+1], self.memory[addr+2], self.memory[addr+3]]);
                        self.stack.push(v as i64);
                    } else { return Err("WASM: i32.load out of bounds"); }
                }
                0x36 => { let align = self.bytecode[self.pc]; self.pc += 1; let _ = self.bytecode[self.pc] as i32 as i64; self.pc += 1;
                    let val = self.stack.pop().unwrap_or(0) as i32;
                    let addr = self.stack.pop().unwrap_or(0) as usize + align as usize;
                    if addr + 4 <= self.memory.len() {
                        let bytes = val.to_le_bytes();
                        self.memory[addr..addr+4].copy_from_slice(&bytes);
                    } else { return Err("WASM: i32.store out of bounds"); }
                }

                // block, loop, if/else/end
                0x02 => { // block — pula para end (0x0B)
                    let mut depth = 1;
                    while depth > 0 && self.pc < self.bytecode.len() {
                        match self.bytecode[self.pc] {
                            0x02 | 0x03 | 0x04 => depth += 1,
                            0x0B => depth -= 1,
                            _ => {}
                        }
                        self.pc += 1;
                    }
                }
                0x03 => { // loop — igual ao block mas volta ao inicio
                    let start = self.pc;
                    let mut depth = 1;
                    while depth > 0 && self.pc < self.bytecode.len() {
                        match self.bytecode[self.pc] {
                            0x02 | 0x03 | 0x04 => depth += 1,
                            0x0B => depth -= 1,
                            _ => {}
                        }
                        self.pc += 1;
                    }
                    self.pc = start; // loop infinito se não houver br
                }
                0x04 => { // if
                    let cond = self.stack.pop().unwrap_or(0);
                    let mut depth = 1;
                    let if_start = self.pc;
                    let mut else_pc = None;
                    let mut end_pc = None;
                    while depth > 0 && self.pc < self.bytecode.len() {
                        match self.bytecode[self.pc] {
                            0x05 => if depth == 1 { else_pc = Some(self.pc + 1); }
                            0x02 | 0x03 | 0x04 => depth += 1,
                            0x0B => { depth -= 1; if depth == 0 { end_pc = Some(self.pc); } }
                            _ => {}
                        }
                        self.pc += 1;
                    }
                    if cond == 0 {
                        if let Some(else_start) = else_pc {
                            self.pc = else_start;
                        } else if let Some(end) = end_pc {
                            self.pc = end + 1;
                        }
                    } else {
                        self.pc = if_start;
                    }
                }
                0x05 => {} // else — tratado pelo if
                0x0B => { return Ok(()); } // end

                // br / br_if
                0x0C => { let label_idx = self.bytecode[self.pc] as usize; self.pc += 1;
                    if label_idx < 16 { return Ok(()); } } // br para fora
                0x0D => { let label_idx = self.bytecode[self.pc] as usize; self.pc += 1;
                    let cond = self.stack.pop().unwrap_or(0);
                    if cond != 0 && label_idx < 16 { return Ok(()); } }

                // return
                0x0F => { return Ok(()); }

                // call
                0x10 => {
                    let func_idx = u32::from_le_bytes([self.bytecode[self.pc], self.bytecode[self.pc+1],
                        self.bytecode[self.pc+2], self.bytecode[self.pc+3]]);
                    self.pc += 4;
                    if let Some(body_off) = self.find_func(func_idx) {
                        let saved_pc = self.pc;
                        let saved_locals = self.locals.clone();
                        // Argumentos: pop args da stack e vira locals
                        let mut new_locals = Vec::new();
                        for i in (0..self.stack.len().saturating_sub(1)).rev() {
                            if new_locals.len() < 8 {
                                new_locals.push(self.stack.remove(i));
                            }
                        }
                        new_locals.reverse();
                        self.locals = new_locals;
                        self.pc = body_off;
                        self.run_bytecode()?;
                        self.locals = saved_locals;
                        self.pc = saved_pc;
                    }
                }

                // drop
                0x1A => { self.stack.pop(); }
                // select
                0x1B => { let c = self.stack.pop().unwrap_or(0); let b = self.stack.pop().unwrap_or(0); let a = self.stack.pop().unwrap_or(0);
                    self.stack.push(if c != 0 { b } else { a }); }

                // i32.eq / i32.ne / i32.lt_s / i32.gt_s
                0x46 => { let b = self.stack.pop().unwrap_or(0); let a = self.stack.pop().unwrap_or(0); self.stack.push(if a == b { 1 } else { 0 }); }
                0x47 => { let b = self.stack.pop().unwrap_or(0); let a = self.stack.pop().unwrap_or(0); self.stack.push(if a != b { 1 } else { 0 }); }
                0x48 => { let b = self.stack.pop().unwrap_or(0) as i32; let a = self.stack.pop().unwrap_or(0) as i32; self.stack.push(if a < b { 1 } else { 0 }); }
                0x4A => { let b = self.stack.pop().unwrap_or(0) as i32; let a = self.stack.pop().unwrap_or(0) as i32; self.stack.push(if a > b { 1 } else { 0 }); }

                // memory.size / memory.grow
                0x3F => { let _ = self.bytecode[self.pc]; self.pc += 1; self.stack.push((self.memory.len() / WASM_PAGE_SIZE) as i64); }
                0x40 => { let _ = self.bytecode[self.pc]; self.pc += 1;
                    let pages = self.stack.pop().unwrap_or(0) as usize;
                    let old = self.memory.len() / WASM_PAGE_SIZE;
                    self.memory.resize((old + pages) * WASM_PAGE_SIZE, 0);
                    self.stack.push(old as i64);
                }

                _ => { return Err("WASM: opcode desconhecido"); }
            }
        }
    }

    /// Executa função exportada do módulo carregado
    pub fn call_export(&mut self, name: &str, args: &[i64]) -> Result<Option<i64>, &'static str> {
        let idx = self.get_export_index(name).ok_or("WASM: export nao encontrado")?;
        let body_off = self.find_func(idx).ok_or("WASM: corpo da funcao nao encontrado")?;
        self.stack.clear();
        self.locals.clear();
        for arg in args { self.stack.push(*arg); }
        // Os primeiros N args da stack viram locals 0..N
        let new_locals: Vec<i64> = args.to_vec();
        self.locals = new_locals;
        self.pc = body_off;
        self.run_bytecode()?;
        Ok(self.stack.pop())
    }

    /// Limpa estado interno
    pub fn reset(&mut self) {
        self.stack.clear();
        self.locals.clear();
        self.pc = 0;
    }
}

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
    crate::serial_println!("[WASM] Registrando '{}' ({} exports)...", name, module.exports.len());

    let skill = WasmSkill::new(bytecode, name, desc, module.exports.clone());
    crate::SKILL_REGISTRY.lock().register(Box::new(skill));
    {
        let mut bridge = WASM_SKILL_BRIDGE.lock();
        bridge.skill_name = String::from(name);
        bridge.registered = true;
    }
    crate::serial_println!("[WASM] Skill '{}' registrada com {} exports.", name, module.exports.len());
    Ok(())
}

/// Skill que executa WASM bytecode
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
        // Verifica se o bytecode WASM é válido
        parse_wasm(&self.bytecode).map(|_| ())
    }

    fn execute(&self, payload: &[u8]) -> Result<Vec<u8>, &'static str> {
        let mut executor = WasmExecutor::new();
        executor.load(&self.bytecode)?;

        // Converte payload em argumento i64
        let args: Vec<i64> = if payload.is_empty() {
            vec![0i64]
        } else if let Ok(text) = core::str::from_utf8(payload) {
            let num: i64 = text.trim().parse().unwrap_or(text.len() as i64);
            vec![num]
        } else {
            vec![payload.len() as i64]
        };

        // Tenta executar a função "main" ou "_start"
        let func_name = if executor.get_export_index("main").is_some() { "main" }
                       else if executor.get_export_index("_start").is_some() { "_start" }
                       else { self.exports.first().map(|e| &e.name[..]).unwrap_or("") };

        if func_name.is_empty() {
            return Err("WASM: nenhuma funcao exportada para executar");
        }

        executor.call_export(func_name, &args)?;

        // Converte resultado da stack em output textual
        if let Some(result) = executor.stack.last() {
            Ok(alloc::format!("[WASM] {} → {}", func_name, result).into_bytes())
        } else {
            Ok(alloc::format!("[WASM] {} executado.", func_name).into_bytes())
        }
    }
}
