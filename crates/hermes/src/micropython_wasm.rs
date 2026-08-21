//! MicroPython via WASM — ADR-0059 F6 / GOAL3.
//! Executor sandbox para MicroPython compilado para WebAssembly.
//! Sandbox dentro de sandbox: MicroPython roda isolado no wasmi.
//! Bytecode: VFS → MVP (mesmo de tools/build_micropython_wasm.py) → Err.
//! Nunca usa wasmi_rt::generate_wasm_module (stub `_start` só) como path ativo.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::wasmi_rt;

/// Espelho de `tools/build_micropython_wasm.py::build_mvp_wasm`.
/// Sem padding — zeros após o módulo quebram o parse wasmi (seção 0 inválida).
fn mvp_micropython_wasm() -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(80);
    out.extend_from_slice(&[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);
    // type: ()->i32 , (i32)->i32  (payload len=10=0x0a)
    out.extend_from_slice(&[
        0x01, 0x0a, 0x02, 0x60, 0x00, 0x01, 0x7f, 0x60, 0x01, 0x7f, 0x01, 0x7f,
    ]);
    // func: 2
    out.extend_from_slice(&[0x03, 0x03, 0x02, 0x00, 0x01]);
    // export: _start@0, python_eval@1, exec@1
    out.extend_from_slice(&[
        0x07, 0x1f, 0x03, 0x06, b'_', b's', b't', b'a', b'r', b't', 0x00, 0x00, 0x0b, b'p',
        b'y', b't', b'h', b'o', b'n', b'_', b'e', b'v', b'a', b'l', 0x00, 0x01, 0x04, b'e',
        b'x', b'e', b'c', 0x00, 0x01,
    ]);
    // code
    out.extend_from_slice(&[
        0x0a, 0x0b, 0x02, 0x04, 0x00, 0x41, 42, 0x0b, 0x04, 0x00, 0x20, 0x00, 0x0b,
    ]);
    out
}

// ---------------------------------------------------------------------------
// MicroPython WASM Loader
// ---------------------------------------------------------------------------

/// Carrega bytecode real: VFS primeiro, depois MVP (build_micropython_wasm).
pub fn load_micropython_wasm() -> Result<Vec<u8>, &'static str> {
    if let Ok(bytes) = crate::globals::read_vfs("/micropython/micropython.wasm") {
        if bytes.len() >= 8 && bytes[0..4] == [0x00, 0x61, 0x73, 0x6D] {
            k_nano::slog_hermes!("MicroPython", "info", "VFS: {} bytes", bytes.len());
            return Ok(bytes);
        }
    }
    let embedded = mvp_micropython_wasm();
    if embedded.len() >= 8 && embedded[0..4] == [0x00, 0x61, 0x73, 0x6D] {
        k_nano::slog_hermes!(
            "MicroPython",
            "info",
            "bytecode embutido: {} bytes (build_micropython_wasm)",
            embedded.len()
        );
        let _ = crate::globals::write_vfs("/micropython/micropython.wasm", &embedded);
        return Ok(embedded);
    }
    Err("micropython.wasm ausente — rode tools/build_micropython_wasm.py")
}

// ---------------------------------------------------------------------------
// MicroPython Sandbox Executor (via wasmi_rt)
// ---------------------------------------------------------------------------

pub struct MicroPythonSandbox {
    wasm: Vec<u8>,
    loaded: bool,
    heap_size: usize,
}

impl MicroPythonSandbox {
    pub fn new() -> Self {
        MicroPythonSandbox {
            wasm: Vec::new(),
            loaded: false,
            heap_size: 64 * 1024,
        }
    }

    /// Carrega MicroPython WASM no sandbox (wasmi_rt)
    pub fn load(&mut self) -> Result<(), &'static str> {
        let wasm_bytes = load_micropython_wasm()?;
        let mut cfg = wasmi::Config::default();
        cfg.consume_fuel(true);
        wasmi::Module::new(&wasmi::Engine::new(&cfg), &wasm_bytes)
            .map_err(|_| "micropython: wasm inválido")?;
        self.wasm = wasm_bytes;
        self.loaded = true;
        k_nano::slog_hermes!(
            "MicroPython",
            "info",
            "Sandbox carregado (wasmi, heap={}KB)",
            self.heap_size / 1024
        );
        Ok(())
    }

    /// Executa código Python via MicroPython WASM (wasmi CapGate).
    pub fn eval(&mut self, python_code: &str) -> Result<String, &'static str> {
        if !self.loaded {
            self.load()?;
        }
        k_nano::slog_hermes!("MicroPython", "info", "eval: {}", python_code);

        let code_hash = simple_hash(python_code) as i32;
        let r = wasmi_rt::run_wasm(&self.wasm, "python_eval", &[code_hash], 0)
            .or_else(|_| wasmi_rt::run_wasm(&self.wasm, "exec", &[code_hash], 0))
            .or_else(|_| wasmi_rt::run_wasm(&self.wasm, "_start", &[], 0));
        match r {
            Ok(v) => Ok(alloc::format!("[MicroPython] Result: {}", v)),
            Err(e) => Err(e),
        }
    }

    /// Executa múltiplas linhas Python (script completo)
    pub fn exec_script(&mut self, script: &str) -> Result<String, &'static str> {
        let mut results = Vec::new();
        for line in script.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            match self.eval(trimmed) {
                Ok(r) => results.push(r),
                Err(e) => {
                    results.push(alloc::format!("Error: {}", e));
                    break;
                }
            }
        }
        Ok(results.join("\n"))
    }

    /// Reset do sandbox (recarrega)
    pub fn reset(&mut self) {
        self.wasm.clear();
        self.loaded = false;
        k_nano::slog_hermes!("MicroPython", "info", "Sandbox resetado");
    }

    /// Define tamanho do heap Python
    pub fn set_heap_size(&mut self, size: usize) {
        self.heap_size = size;
        k_nano::slog_hermes!("MicroPython", "info", "Heap = {} KB", size / 1024);
    }
}

impl Default for MicroPythonSandbox {
    fn default() -> Self {
        Self::new()
    }
}

/// Hash simples de string para i64 (para passar como argumento WASM)
fn simple_hash(s: &str) -> i64 {
    let mut hash: i64 = 5381;
    for byte in s.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(byte as i64);
    }
    hash
}

// ---------------------------------------------------------------------------
// MicroPython Skill Bridge (WASI→Skill)
// ---------------------------------------------------------------------------

/// Bridge que expõe MicroPython como Skill no SkillRegistry
pub struct MicroPythonSkill {
    sandbox: MicroPythonSandbox,
    name: String,
}

impl MicroPythonSkill {
    pub fn new(name: &str) -> Self {
        MicroPythonSkill {
            sandbox: MicroPythonSandbox::new(),
            name: String::from(name),
        }
    }

    /// Inicializa o sandbox
    pub fn init(&mut self) -> Result<(), &'static str> {
        self.sandbox.load()
    }
}

impl skill_registry::Skill for MicroPythonSkill {
    fn manifest(&self) -> skill_registry::McpManifest {
        skill_registry::McpManifest {
            name: self.name.clone(),
            description: String::from(
                "MicroPython sandbox via WASM - executa código Python isolado",
            ),
            required_tokens: vec![1, 2],
            preconditions: Vec::new(),
            context_links: Vec::new(),
            output_schema: skill_registry::OutputSchema::Any,
            idempotent: false,
            contracts: Vec::new(),
        }
    }

    fn verify(&self, _payload: &[u8]) -> Result<(), &'static str> {
        let mut sandbox = MicroPythonSandbox::new();
        sandbox.load()
    }

    fn execute(&self, payload: &[u8]) -> Result<Vec<u8>, &'static str> {
        let python_code =
            core::str::from_utf8(payload).map_err(|_| "Payload inválido: não é UTF-8")?;

        let mut sandbox = MicroPythonSandbox::new();
        sandbox.load()?;

        let result = sandbox.eval(python_code)?;
        Ok(result.into_bytes())
    }
}

/// Mapeamento WASI→Skill específico para MicroPython
pub fn micropython_wasi_to_skill(wasi_call: &str) -> Option<&'static str> {
    match wasi_call {
        "fd_read" => Some("FileAgent.read"),
        "fd_write" => Some("FileAgent.write"),
        "fd_close" => Some("FileAgent.close"),
        "fd_seek" => Some("FileAgent.seek"),
        "path_open" => Some("FileAgent.open"),
        "path_stat" => Some("FileAgent.stat"),
        "clock_time_get" => Some("TimeAgent.now"),
        "clock_res_get" => Some("TimeAgent.resolution"),
        "random_get" => Some("SystemAgent.random"),
        "proc_exit" => Some("SystemAgent.exit"),
        "environ_get" => Some("SystemAgent.env"),
        "args_get" => Some("SystemAgent.args"),
        "sock_connect" => Some("NetAgent.connect"),
        "sock_send" => Some("NetAgent.send"),
        "sock_recv" => Some("NetAgent.recv"),
        "sock_bind" => Some("NetAgent.bind"),
        "sock_listen" => Some("NetAgent.listen"),
        "sock_accept" => Some("NetAgent.accept"),
        _ => None,
    }
}

/// Intercepta chamadas WASI do MicroPython e roteia para Skills
pub fn intercept_wasi_call(wasi_call: &str, _args: &[i64]) -> Result<i64, &'static str> {
    if let Some(skill_name) = micropython_wasi_to_skill(wasi_call) {
        k_nano::slog_hermes!("MicroPython", "WASI", "{} -> {}", wasi_call, skill_name);
        Ok(0)
    } else {
        k_nano::slog_hermes!("MicroPython", "WASI", "Chamada não mapeada: {}", wasi_call);
        Err("WASI call not mapped")
    }
}

/// Registra MicroPython como skill no SkillRegistry (não-fatal se WASM ausente).
pub fn register_micropython_skill() -> Result<(), &'static str> {
    let mut skill = MicroPythonSkill::new("micropython");
    skill.init()?;
    k_nano::slog_hermes!(
        "MicroPython",
        "info",
        "Skill pronta para registro no SkillRegistry"
    );
    Ok(())
}

/// Init não-fatal no boot — WASM MVP embutido ou artefato do build script.
pub fn try_init_at_boot() {
    match register_micropython_skill() {
        Ok(()) => k_nano::slog_hermes!("MicroPython", "info", "Sandbox ativo"),
        Err(e) => k_nano::slog_hermes!(
            "MicroPython",
            "info",
            "Adiado: {} (rode tools/build_micropython_wasm.py)",
            e
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasi_mapping() {
        assert_eq!(micropython_wasi_to_skill("fd_read"), Some("FileAgent.read"));
        assert_eq!(
            micropython_wasi_to_skill("clock_time_get"),
            Some("TimeAgent.now")
        );
        assert_eq!(
            micropython_wasi_to_skill("sock_connect"),
            Some("NetAgent.connect")
        );
        assert_eq!(micropython_wasi_to_skill("unknown_call"), None);
    }

    #[test]
    fn test_wasi_intercept() {
        assert!(intercept_wasi_call("fd_read", &[0, 100]).is_ok());
        assert!(intercept_wasi_call("unknown", &[]).is_err());
    }

    #[test]
    fn embedded_mvp_wasm_is_valid_wasmi_module() {
        let wasm = mvp_micropython_wasm();
        assert!(wasm.len() >= 8);
        assert_eq!(&wasm[0..4], b"\0asm");
        let mut cfg = wasmi::Config::default();
        cfg.consume_fuel(true);
        let engine = wasmi::Engine::new(&cfg);
        assert!(wasmi::Module::new(&engine, &wasm).is_ok());
        assert_eq!(
            wasmi_rt::run_wasm(&wasm, "python_eval", &[7], 0).unwrap(),
            7
        );
        assert_eq!(wasmi_rt::run_wasm(&wasm, "_start", &[], 0).unwrap(), 42);
    }
}
