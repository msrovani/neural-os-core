//! ADR-0059 Caminho A — Runtime WASM real (`wasmi`, no_std, fuel).
//!
//! Executa **módulos WebAssembly padrão** em sandbox (SFI + fuel + limite de
//! memória), com host-imports `aios::*` **gated por CapGate + PermissionGate**.
//! Host net/fs: Cap + `net_bridge`/`VFS` → I/O real; sem bridge → trap.
//! GPU → trap até KernelPack. WASI Preview1 não ligado (`wasi_host` orphan).
//!
//! Substitui a VM `Op` custom (`wasm_exec.rs`) e o interpretador parcial
//! (`wasm.rs`) — aposentados pela ADR-0059.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use wasmi::{Config, Engine, Linker, Module, Store};

// ─── Capability bitmask constants ───
// Usado pelo check_cap() para gate de host functions.
pub const CAP_LOG: u32     = 1 << 0;
pub const CAP_NET: u32     = 1 << 1;
pub const CAP_FS: u32      = 1 << 2;
pub const CAP_DISPLAY: u32 = 1 << 3;
pub const CAP_AUDIO: u32   = 1 << 4;
pub const CAP_CRYPTO: u32  = 1 << 5;
pub const CAP_IO: u32      = 1 << 6;
pub const CAP_DMA: u32     = 1 << 7;
pub const CAP_SYS: u32     = 1 << 8;
pub const CAP_GPU: u32      = 1 << 9;
pub const CAP_ALL: u32     = 0xFFFF_FFFF;
pub const CAP_NONE: u32    = 0;

/// Maximum allocation size for WASM-allocated buffers (1MB cap).
const MAX_WASM_ALLOC: usize = 1024 * 1024;

/// Estado do host visível às funções importadas (capabilities concedidas).
pub struct HostState {
    pub caps: u32,
    pub out: Vec<u8>,
}

impl HostState {
    pub fn new(caps: u32) -> Self {
        Self { caps, out: Vec::new() }
    }
}

/// Fuel default por execução.
pub const DEFAULT_FUEL: u64 = 5_000_000;

/// Verifica cap bitmask e roteia Escalate para PermissionGate.
/// Returns `Err(wasmi::Error)` (trap) on denial.
/// Honesty SESSION_379: nunca forçar `Verdict::Allow` — RiskLevel classifica.
fn check_cap(caller: &wasmi::Caller<'_, HostState>, required: u32, namespace: &str, name: &str) -> Result<(), wasmi::Error> {
    let held = caller.data().caps;
    // 1. Bitmask check
    if held & required == 0 {
        k_nano::telemetry::TELEMETRY.push(4, 0, &required.to_ne_bytes());
        return Err(wasmi::Error::new("capability denied (bitmask)"));
    }
    // 2. Membrane-like verdict from RiskLevel (não hardcode Allow).
    let risk = crate::permission_gate::RiskLevel::classify(namespace, name);
    let membrane = match risk {
        crate::permission_gate::RiskLevel::Auto => crate::membrane::Verdict::Allow,
        crate::permission_gate::RiskLevel::Deny => crate::membrane::Verdict::Deny,
        crate::permission_gate::RiskLevel::Confirm
        | crate::permission_gate::RiskLevel::Escalate => crate::membrane::Verdict::Escalate,
    };
    let verdict = crate::permission_gate::PermissionGate::check(namespace, name, membrane);
    match verdict {
        crate::permission_gate::PermissionVerdict::Allow => Ok(()),
        crate::permission_gate::PermissionVerdict::Deny => {
            k_nano::telemetry::TELEMETRY.push(4, 0, &required.to_ne_bytes());
            Err(wasmi::Error::new("permission denied (gate)"))
        }
        crate::permission_gate::PermissionVerdict::Pending { id } => {
            k_nano::slog_hermes!("WASMI", "warn", "Pending HITL #{} — trap", id);
            Err(wasmi::Error::new("HITL pending"))
        }
    }
}

fn read_guest_bytes(
    caller: &wasmi::Caller<'_, HostState>,
    ptr: i32,
    len: i32,
) -> Result<Vec<u8>, wasmi::Error> {
    let Some(wasmi::Extern::Memory(mem)) = caller.get_export("memory") else {
        return Err(wasmi::Error::new("wasm memory missing"));
    };
    let data = mem.data(caller);
    let (p, l) = (ptr as usize, (len as usize).min(MAX_WASM_ALLOC));
    if p.saturating_add(l) > data.len() {
        return Err(wasmi::Error::new("guest ptr OOB"));
    }
    Ok(data[p..p + l].to_vec())
}

fn read_guest_str(
    caller: &wasmi::Caller<'_, HostState>,
    ptr: i32,
    len: i32,
) -> Result<String, wasmi::Error> {
    let bytes = read_guest_bytes(caller, ptr, len)?;
    core::str::from_utf8(&bytes)
        .map(|s| String::from(s))
        .map_err(|_| wasmi::Error::new("guest str utf8"))
}

/// Instala os host-imports `aios::*`, `aios_net::*`, `aios_fs::*`
/// e `wasi_snapshot_preview1` no linker, **gated por CapGate + PermissionGate**.
fn install_host_abi(linker: &mut Linker<HostState>) -> Result<(), &'static str> {
    // ── aios::log(ptr,len) ────────────────────────────────────────────────
    linker.func_wrap("aios", "log",
        |mut caller: wasmi::Caller<'_, HostState>, ptr: i32, len: i32| -> Result<(), wasmi::Error> {
            check_cap(&caller, CAP_LOG, "aios", "log")?;
            if let Some(wasmi::Extern::Memory(mem)) = caller.get_export("memory") {
                let data = mem.data(&caller);
                let (p, l) = (ptr as usize, len as usize);
                let l = l.min(MAX_WASM_ALLOC);
                if p.saturating_add(l) <= data.len() {
                    let mut buf = Vec::with_capacity(l);
                    buf.extend_from_slice(&data[p..p + l]);
                    caller.data_mut().out.extend_from_slice(&buf);
                }
            }
            k_nano::telemetry::TELEMETRY.push(3, 0, &[0; 32]); // EV_WASM_CALL
            Ok(())
        },
    ).map_err(|_| "linker aios::log")?;

    // ── aios::debug(i32) -> i32 ─────────────────────────────────────────────
    linker.func_wrap("aios", "debug",
        |caller: wasmi::Caller<'_, HostState>, val: i32| -> Result<i32, wasmi::Error> {
            check_cap(&caller, CAP_LOG, "aios", "debug")?;
            Ok(val)
        },
    ).map_err(|_| "linker aios::debug")?;

    // ── aios::get_tick() -> i64 ─────────────────────────────────────────────
    linker.func_wrap("aios", "get_tick",
        |caller: wasmi::Caller<'_, HostState>| -> Result<i64, wasmi::Error> {
            check_cap(&caller, CAP_LOG, "aios", "get_tick")?;
            Ok(k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as i64)
        },
    ).map_err(|_| "linker aios::get_tick")?;

    // ── aios_net::http_get(ptr,len) -> i32 ──────────────────────────────────
    // Cap + net_bridge → body em HostState.out, retorna len; senão trap.
    linker.func_wrap("aios_net", "http_get",
        |mut caller: wasmi::Caller<'_, HostState>, ptr: i32, len: i32| -> Result<i32, wasmi::Error> {
            check_cap(&caller, CAP_NET, "aios_net", "http_get")?;
            if !crate::net_bridge::http_ready() {
                return Err(wasmi::Error::new("aios_net::http_get bridge absent"));
            }
            let url = read_guest_str(&caller, ptr, len)?;
            let body = crate::net_bridge::http_get_url(&url)
                .or_else(|_| crate::net_bridge::resolve_and_http_get_safe(&url))
                .map_err(|e| wasmi::Error::new(e))?;
            let n = body.len().min(MAX_WASM_ALLOC) as i32;
            caller.data_mut().out = body;
            Ok(n)
        },
    ).map_err(|_| "linker aios_net::http_get")?;

    // ── aios_fs::fs_read(ptr,len,max) -> i32 ────────────────────────────────
    linker.func_wrap("aios_fs", "fs_read",
        |mut caller: wasmi::Caller<'_, HostState>, ptr: i32, len: i32, max: i32| -> Result<i32, wasmi::Error> {
            check_cap(&caller, CAP_FS, "aios_fs", "fs_read")?;
            if !crate::fs::vfs_ready_for_wasm() {
                return Err(wasmi::Error::new("aios_fs::fs_read VFS absent"));
            }
            let path = read_guest_str(&caller, ptr, len)?;
            let data = crate::fs::read_vfs(&path).map_err(|e| wasmi::Error::new(e))?;
            let cap = (max as usize).min(MAX_WASM_ALLOC).min(data.len());
            caller.data_mut().out = data[..cap].to_vec();
            Ok(cap as i32)
        },
    ).map_err(|_| "linker aios_fs::fs_read")?;

    // ── aios_fs::fs_write(ptr,len) -> i32 ───────────────────────────────────
    // Guest layout: path\0payload (path NUL-terminated); retorna bytes escritos.
    linker.func_wrap("aios_fs", "fs_write",
        |mut caller: wasmi::Caller<'_, HostState>, ptr: i32, len: i32| -> Result<i32, wasmi::Error> {
            check_cap(&caller, CAP_FS, "aios_fs", "fs_write")?;
            if !crate::fs::vfs_ready_for_wasm() {
                return Err(wasmi::Error::new("aios_fs::fs_write VFS absent"));
            }
            let blob = read_guest_bytes(&caller, ptr, len)?;
            let nul = blob.iter().position(|&b| b == 0).ok_or_else(|| {
                wasmi::Error::new("aios_fs::fs_write need path\\0payload")
            })?;
            let path = core::str::from_utf8(&blob[..nul])
                .map_err(|_| wasmi::Error::new("aios_fs::fs_write path utf8"))?;
            let payload = &blob[nul + 1..];
            crate::fs::write_vfs(path, payload).map_err(|e| wasmi::Error::new(e))?;
            Ok(payload.len() as i32)
        },
    ).map_err(|_| "linker aios_fs::fs_write")?;

    // ── aios_gpu::submit(op,flags) -> i32 ────────────────────────────────
    // Sem KernelPack → trap (Ok(0) fingia job id / CPU fallback).
    linker.func_wrap("aios_gpu", "submit",
        |caller: wasmi::Caller<'_, HostState>, op: i32, _flags: i32| -> Result<i32, wasmi::Error> {
            check_cap(&caller, CAP_GPU, "aios_gpu", "submit")?;
            k_nano::slog_hermes!(
                "WASM",
                "warn",
                "aios_gpu::submit not wired (no KernelPack) op={}",
                op
            );
            Err(wasmi::Error::new("aios_gpu::submit no KernelPack/backend"))
        },
    ).map_err(|_| "linker aios_gpu::submit")?;

    // wasi_snapshot_preview1: orphan wasi_host.rs — não wire stub.
    Ok(())
}

/// Executa uma função exportada `func_name(i32,i32)->i32` de um módulo WASM.
/// `caps` = capabilities concedidas (CapGate). Fuel limita o tempo.
pub fn run_i32_2(
    wasm: &[u8],
    func_name: &str,
    a: i32,
    b: i32,
    caps: u32,
) -> Result<i32, &'static str> {
    let mut config = Config::default();
    config.consume_fuel(true);
    let engine = Engine::new(&config);
    // ponytail: verificar integridade basica antes de chamar parser (evita #PF em wasmparser)
    if wasm.len() < 8 || wasm[0..4] != [0x00, 0x61, 0x73, 0x6D] {
        return Err("wasm: bytes inválidos (sem magic)");
    }
    let module = Module::new(&engine, wasm).map_err(|_| "wasm: módulo inválido")?;
    let mut store = Store::new(&engine, HostState::new(caps));
    store.set_fuel(DEFAULT_FUEL).map_err(|_| "wasm: set_fuel")?;

    let mut linker = <Linker<HostState>>::new(&engine);
    install_host_abi(&mut linker)?;

    let instance = linker
        .instantiate_and_start(&mut store, &module)
        .map_err(|_| "wasm: instantiate (import negado/ausente?)")?;

    let func = instance
        .get_typed_func::<(i32, i32), i32>(&store, func_name)
        .map_err(|_| "wasm: export não encontrado")?;

    func.call(&mut store, (a, b)).map_err(|_| "wasm: trap/out-of-fuel")
}

/// Executa uma funcao exportada 'func_name()->i32' (zero params).
pub fn run_i32_0(
    wasm: &[u8],
    func_name: &str,
    caps: u32,
) -> Result<i32, &'static str> {
    let mut config = Config::default();
    config.consume_fuel(true);
    let engine = Engine::new(&config);
    if wasm.len() < 8 || wasm[0..4] != [0x00, 0x61, 0x73, 0x6D] {
        return Err("wasm: bytes invalidos");
    }
    let module = Module::new(&engine, wasm).map_err(|_| "wasm: modulo invalido")?;
    let mut store = Store::new(&engine, HostState::new(caps));
    store.set_fuel(DEFAULT_FUEL).map_err(|_| "wasm: set_fuel")?;
    let mut linker = <Linker<HostState>>::new(&engine);
    install_host_abi(&mut linker)?;
    let instance = linker
        .instantiate_and_start(&mut store, &module)
        .map_err(|_| "wasm: instantiate")?;
    let func = instance
        .get_typed_func::<(), i32>(&store, func_name)
        .map_err(|_| "wasm: export nao encontrado")?;
    func.call(&mut store, ()).map_err(|_| "wasm: trap/out-of-fuel")
}

/// Executa uma funcao exportada 'func_name(i32,i32,i32)->i32' (3 params).
pub fn run_i32_3(
    wasm: &[u8],
    func_name: &str,
    a: i32,
    b: i32,
    c: i32,
    caps: u32,
) -> Result<i32, &'static str> {
    let mut config = Config::default();
    config.consume_fuel(true);
    let engine = Engine::new(&config);
    if wasm.len() < 8 || wasm[0..4] != [0x00, 0x61, 0x73, 0x6D] {
        return Err("wasm: bytes invalidos");
    }
    let module = Module::new(&engine, wasm).map_err(|_| "wasm: modulo invalido")?;
    let mut store = Store::new(&engine, HostState::new(caps));
    store.set_fuel(DEFAULT_FUEL).map_err(|_| "wasm: set_fuel")?;
    let mut linker = <Linker<HostState>>::new(&engine);
    install_host_abi(&mut linker)?;
    let instance = linker
        .instantiate_and_start(&mut store, &module)
        .map_err(|_| "wasm: instantiate")?;
    let func = instance
        .get_typed_func::<(i32, i32, i32), i32>(&store, func_name)
        .map_err(|_| "wasm: export nao encontrado")?;
    func.call(&mut store, (a, b, c)).map_err(|_| "wasm: trap/out-of-fuel")
}

/// Módulo WASM mínimo válido: `(func (export "add")(param i32 i32)(result i32)
/// local.get 0; local.get 1; i32.add)`. Usado no self-test (sem imports).
const ADD_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // magic + version
    0x01, 0x07, 0x01, 0x60, 0x02, 0x7f, 0x7f, 0x01, 0x7f, // type: (i32,i32)->i32
    0x03, 0x02, 0x01, 0x00, // func: 1 func, type 0
    0x07, 0x07, 0x01, 0x03, 0x61, 0x64, 0x64, 0x00, 0x00, // export "add" func 0
    0x0a, 0x09, 0x01, 0x07, 0x00, 0x20, 0x00, 0x20, 0x01, 0x6a, 0x0b, // code: get0 get1 i32.add end
];

/// Módulo WASM enlatado para TESTES (`_start` → i32(42)). H8 (canvas onda 1):
/// em produção NÃO há gerador de bytes — código produtivo sem WASM recebe
/// `Err("no-wasm-bytes")`. A geração real vem do op-IR (#412 / wasm_build).
///
/// Host ABI no runtime (CapGate): `aios::{log,debug,get_tick}` wired;
/// Cap+bridge/VFS → I/O; GPU → **trap** até KernelPack (SESSION_379 residual).
/// WASI Preview1 **não** ligado (`wasi_host` orphan).
#[cfg(test)]
pub fn canned_test_module() -> Vec<u8> {
    let mut wasm = Vec::with_capacity(64);
    // magic + version
    wasm.extend_from_slice(&[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);
    // type section: () -> i32
    wasm.push(0x01); wasm.push(0x05); wasm.push(0x01); // section 1, 5 bytes, 1 type
    wasm.push(0x60); wasm.push(0x00); wasm.push(0x01); wasm.push(0x7f); // ()->i32
    // func section: 1 func, type 0
    wasm.push(0x03); wasm.push(0x02); wasm.push(0x01); wasm.push(0x00);
    // export section: "_start" func 0
    wasm.push(0x07); wasm.push(0x0a); wasm.push(0x01);
    wasm.push(0x06); // name length = 6 ("_start")
    wasm.extend_from_slice(b"_start"); // name
    wasm.push(0x00); // kind = func
    wasm.push(0x00); // func_idx = 0
    // code section: body = i32.const 42; end
    wasm.push(0x0a); wasm.push(0x06); wasm.push(0x01);
    wasm.push(0x04); wasm.push(0x00); // body size 4, 0 locals
    wasm.push(0x41); wasm.push(42); // i32.const 42
    wasm.push(0x0b); // end
    wasm
}

/// Executa uma função exportada de um módulo WASM com argumentos `&[i32]`.
/// Tenta resolver por assinatura (0..4 args i32 → i32).

pub fn run_wasm(
    wasm: &[u8],
    func_name: &str,
    args: &[i32],
    caps: u32,
) -> Result<i32, &'static str> {
    let mut config = Config::default();
    config.consume_fuel(true);
    let engine = Engine::new(&config);
    if wasm.len() < 8 || wasm[0..4] != [0x00, 0x61, 0x73, 0x6D] {
        return Err("wasm: bytes inválidos (sem magic)");
    }
    let module = Module::new(&engine, wasm).map_err(|_| "wasm: módulo inválido")?;
    let mut store = Store::new(&engine, HostState::new(caps));
    store.set_fuel(DEFAULT_FUEL).map_err(|_| "wasm: set_fuel")?;
    let mut linker = <Linker<HostState>>::new(&engine);
    install_host_abi(&mut linker)?;
    let instance = linker
        .instantiate_and_start(&mut store, &module)
        .map_err(|_| "wasm: instantiate")?;
    if args.len() > 4 {
        return Err("wasm: muitos argumentos (max 4)");
    }
    // Resolve por assinatura: tenta a aridade fornecida primeiro, depois as
    // demais 0..=4. Necessário porque `sandbox_validate_and_run` chama com
    // `args` vazio módulos cujo export tem aridade > 0 (ex.: DSL `run(a,b)`);
    // a assinatura errada falha no `get_typed_func` sem executar.
    let a = |i: usize| args.get(i).copied().unwrap_or(0);
    for n_params in [args.len(), 0, 1, 2, 3, 4] {
        let r: Result<i32, _> = match n_params {
            0 => instance.get_typed_func::<(), i32>(&store, func_name)
                .and_then(|f| f.call(&mut store, ()).map_err(|e| e.into())),
            1 => instance.get_typed_func::<(i32,), i32>(&store, func_name)
                .and_then(|f| f.call(&mut store, (a(0),)).map_err(|e| e.into())),
            2 => instance.get_typed_func::<(i32, i32), i32>(&store, func_name)
                .and_then(|f| f.call(&mut store, (a(0), a(1))).map_err(|e| e.into())),
            3 => instance.get_typed_func::<(i32, i32, i32), i32>(&store, func_name)
                .and_then(|f| f.call(&mut store, (a(0), a(1), a(2))).map_err(|e| e.into())),
            4 => instance.get_typed_func::<(i32, i32, i32, i32), i32>(&store, func_name)
                .and_then(|f| f.call(&mut store, (a(0), a(1), a(2), a(3))).map_err(|e| e.into())),
            _ => continue,
        };
        if let Ok(val) = r {
            return Ok(val);
        }
    }
    Err("wasm: export não encontrado ou assinatura incompatível")
}
/// Valida e executa um modulo WASM no sandbox (fuel limitado, sem imports perigosos).
/// Retorna true se executou sem trap.
pub fn sandbox_validate_and_run(wasm: &[u8]) -> bool {
    run_wasm(wasm, "run", &[], CAP_ALL).is_ok()
}


/// Self-test de boot (sem modelo): roda um `.wasm` real (`add(2,3)==5`) no
/// wasmi. Prova que o runtime WASM funciona em bare-metal. Retorna true = PASS.
pub fn self_test() -> bool {
    match run_i32_2(ADD_WASM, "add", 2, 3, 0) {
        Ok(5) => {
            k_nano::slog_hermes!("WASMI", "info", "runtime WASM real self-test PASS (add(2,3)=5) — ADR-0059 A");
            true
        }
        Ok(other) => {
            k_nano::slog_hermes!("WASMI", "warn", "self-test resultado inesperado: {}", other);
            false
        }
        Err(e) => {
            k_nano::slog_hermes!("WASMI", "warn", "self-test FAIL: {}", e);
            false
        }
    }
}

// ─── WASM Bridge — ADR-0059 F3: `register_wasm_skill` → wasmi_rt ─────────────

const WASM_MAGIC: [u8; 4] = [0x00, 0x61, 0x73, 0x6D]; // \0asm
const WASM_VERSION: [u8; 4] = [0x01, 0x00, 0x00, 0x00];

#[derive(Debug, Clone)]
pub struct WasmExport {
    pub name: String,
    pub kind: u8, // 0=func, 1=table, 2=mem, 3=global
    pub index: u32,
}

#[derive(Debug, Clone)]
struct WasmModule {
    pub functions: u32,
    pub exports: Vec<WasmExport>,
}

/// Parseia cabeçalho WASM e tabela de exports (LEB128 real).
/// O parser anterior lia `section_len` como u32 LE fixo — em módulos válidos
/// o 1º byte de conteúdo virava parte do len, `section_end` estourava e o
/// loop quebrava com `exports` vazio: TODA WasmSkill registrada ficava muda
/// (`execute` → "nenhuma função exportada"). Lane B conserta a leitura.
fn read_uleb32(data: &[u8], off: &mut usize) -> Option<u32> {
    let mut res: u32 = 0;
    let mut shift = 0;
    for _ in 0..5 {
        if *off >= data.len() {
            return None;
        }
        let b = data[*off];
        *off += 1;
        res |= ((b & 0x7F) as u32).checked_shl(shift)?;
        shift += 7;
        if b & 0x80 == 0 {
            return Some(res);
        }
    }
    None
}

fn parse_wasm(bytecode: &[u8]) -> Result<WasmModule, &'static str> {
    if bytecode.len() < 8 {
        return Err("Wasm too short");
    }
    if bytecode[0..4] != WASM_MAGIC {
        return Err("Invalid WASM magic");
    }
    if bytecode[4..8] != WASM_VERSION {
        return Err("Unsupported WASM version");
    }

    let mut off = 8usize;
    let mut functions = 0u32;
    let mut exports = Vec::new();

    while off < bytecode.len() {
        let section_id = bytecode[off];
        off += 1;
        let section_len = match read_uleb32(bytecode, &mut off) {
            Some(n) => n as usize,
            None => break, // fail-soft: registra parcial (verify valida depois)
        };
        let section_end = match off.checked_add(section_len) {
            Some(e) if e <= bytecode.len() => e,
            _ => break, // fail-soft (idem)
        };

        match section_id {
            3 => {
                // Function section: count + type indices
                let mut p = off;
                if let Some(count) = read_uleb32(bytecode, &mut p) {
                    functions = count;
                }
            }
            7 => {
                // Export section: count + (name_len, name, kind, index)
                let mut p = off;
                let count = match read_uleb32(bytecode, &mut p) {
                    Some(n) => n,
                    None => {
                        off = section_end;
                        continue;
                    }
                };
                for _ in 0..count {
                    let name_len = match read_uleb32(bytecode, &mut p) {
                        Some(n) => n as usize,
                        None => break,
                    };
                    if p + name_len > section_end {
                        break;
                    }
                    let name = core::str::from_utf8(&bytecode[p..p + name_len])
                        .unwrap_or("?")
                        .to_string();
                    p += name_len;
                    if p + 1 > section_end {
                        break;
                    }
                    let kind = bytecode[p];
                    p += 1;
                    let index = match read_uleb32(bytecode, &mut p) {
                        Some(n) => n,
                        None => break,
                    };
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

// ─── WASI→Skill Bridge ───────────────────────────────────────────────────────

struct WasmSkillBridge {
    skill_name: String,
    registered: bool,
}

static WASM_SKILL_BRIDGE: spin::Mutex<WasmSkillBridge> = spin::Mutex::new(WasmSkillBridge {
    skill_name: String::new(),
    registered: false,
});

/// Registra uma skill WASM no SkillRegistry.
/// Proveniência default = `Template` (chamadores legados não declaram origem;
/// o wire model-born usa `register_wasm_skill_with_provenance`).
pub fn register_wasm_skill(bytecode: &[u8], name: &str, desc: &str) -> Result<(), &'static str> {
    register_wasm_skill_inner(bytecode, name, desc, None)
}

/// Lane B: registra skill WASM com proveniência carimbada + log serial.
/// `model-born` = op-IR veio de texto do modelo (via `model_text_to_ops`);
/// `template`/`dummy` = bytes de teste/placeholders — nunca contam como
/// model-born; `reloaded` = sidecar `.prov` ausente no reload (skill antiga);
/// `imported` = mesh SkillSync. Sidecar gravado pelo promote (evolve).
pub fn register_wasm_skill_with_provenance(
    bytecode: &[u8],
    name: &str,
    desc: &str,
    provenance: SkillProvenance,
) -> Result<(), &'static str> {
    register_wasm_skill_inner(bytecode, name, desc, Some(provenance))
}

/// Proveniência da skill WASM (Lane B — carimbo no registro, log no serial).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillProvenance {
    /// op-IR convertida de texto bruto do modelo (`model_text_to_ops`).
    ModelBorn,
    /// Bytes de template/placeholders conhecidos (ex.: `generate_add_wasm`).
    Template,
    /// Sentinelas `I32Const(0)`/`I32Const(42)` — nunca contam como model-born.
    Dummy,
    /// Sidecar `.prov` ausente no reload (skill antiga, origem desconhecida).
    Reloaded,
    /// Importada via mesh SkillSync (`skill_sync.rs`, outro nó).
    Imported,
}

impl SkillProvenance {
    pub fn as_str(&self) -> &'static str {
        match self {
            SkillProvenance::ModelBorn => "model-born",
            SkillProvenance::Template => "template",
            SkillProvenance::Dummy => "dummy",
            SkillProvenance::Reloaded => "reloaded",
            SkillProvenance::Imported => "imported",
        }
    }
}

static SKILL_PROVENANCE: spin::Mutex<BTreeMap<String, SkillProvenance>> =
    spin::Mutex::new(BTreeMap::new());

/// Runtime hygiene (s410d): provenance map é 1:1 com o registro de skills
/// (re-register sobrescreve), mas skills unregistered deixavam entrada órfã.
/// Cap espelha o cap do registro de skills (32 no skill_marketplace, s410).
const SKILL_PROVENANCE_CAP: usize = 64;

/// Carimba proveniência no registro (sobrescreve em re-registro).
pub fn record_skill_provenance(name: &str, provenance: SkillProvenance) {
    let mut map = SKILL_PROVENANCE.lock();
    if !map.contains_key(name) && map.len() >= SKILL_PROVENANCE_CAP {
        if let Some(first) = map.keys().next().cloned() {
            map.remove(&first);
        }
    }
    map.insert(String::from(name), provenance);
}

/// Lê o carimbo de proveniência (`None` = registrada pelo caminho legado).
pub fn skill_provenance(name: &str) -> Option<SkillProvenance> {
    SKILL_PROVENANCE.lock().get(name).copied()
}

// ─── Lane B3+C: métricas mínimas (AtomicU64, sem tópico/evento) ─────────────
// Parse/promote/reload contam aqui; leitura via getters (futuro HUD lê sem
// EventBus — sem consumidor novo, sem tópico novo).
static METER_MODEL_TEXT_OK: AtomicU64 = AtomicU64::new(0);
static METER_MODEL_TEXT_FAIL: AtomicU64 = AtomicU64::new(0);
static METER_PROMOTE_OK: AtomicU64 = AtomicU64::new(0);
static METER_PROMOTE_DENY: AtomicU64 = AtomicU64::new(0);
static METER_RELOAD_OK: AtomicU64 = AtomicU64::new(0);

/// `model_text_to_ops` converteu (evolve + decode_harness anotam).
pub fn note_model_text_parse(ok: bool) {
    if ok {
        METER_MODEL_TEXT_OK.fetch_add(1, Ordering::Relaxed);
    } else {
        METER_MODEL_TEXT_FAIL.fetch_add(1, Ordering::Relaxed);
    }
}

/// Promote terminou registrado (ok) ou recusado em qualquer gate (deny).
pub fn note_promote(ok: bool) {
    if ok {
        METER_PROMOTE_OK.fetch_add(1, Ordering::Relaxed);
    } else {
        METER_PROMOTE_DENY.fetch_add(1, Ordering::Relaxed);
    }
}

/// Reload re-registrou 1 skill do VFS.
pub fn note_reload_ok() {
    METER_RELOAD_OK.fetch_add(1, Ordering::Relaxed);
}

/// (parse_ok, parse_fail) do model-text.
pub fn metrics_model_text() -> (u64, u64) {
    (
        METER_MODEL_TEXT_OK.load(Ordering::Relaxed),
        METER_MODEL_TEXT_FAIL.load(Ordering::Relaxed),
    )
}

/// (promote_ok, promote_deny).
pub fn metrics_promote() -> (u64, u64) {
    (
        METER_PROMOTE_OK.load(Ordering::Relaxed),
        METER_PROMOTE_DENY.load(Ordering::Relaxed),
    )
}

/// reload_ok acumulado.
pub fn metrics_reload_ok() -> u64 {
    METER_RELOAD_OK.load(Ordering::Relaxed)
}

// ─── Lane B3: sidecar de proveniência `/skills/{name}.prov` ─────────────────
// Formato: ASCII exato `model-born|template|dummy|reloaded|imported`.
// Escolhido sobre seção custom WASM: `build_run_module` vive em wasm_build.rs
// (fora do escopo B3) e o sidecar é legível pelo reload sem re-parse do
// módulo; skills antigas (sem sidecar) caem em `Reloaded` + warn.
pub fn provenance_sidecar_path(name: &str) -> String {
    alloc::format!("/skills/{}.prov", name)
}

/// Parse estrito dos bytes do sidecar (`None` = ausente/corrompido).
pub fn parse_provenance_bytes(bytes: &[u8]) -> Option<SkillProvenance> {
    match bytes {
        b"model-born" => Some(SkillProvenance::ModelBorn),
        b"template" => Some(SkillProvenance::Template),
        b"dummy" => Some(SkillProvenance::Dummy),
        b"reloaded" => Some(SkillProvenance::Reloaded),
        b"imported" => Some(SkillProvenance::Imported),
        _ => None,
    }
}

/// Grava o sidecar (best-effort pelo caller; VFS ausente = `Err`).
pub fn write_provenance_sidecar(name: &str, prov: SkillProvenance) -> Result<(), &'static str> {
    let path = provenance_sidecar_path(name);
    crate::fs::write_vfs(&path, prov.as_str().as_bytes())
}

/// Lê o sidecar (`None` = ausente/ilegível → caller usa `Reloaded` + warn).
pub fn read_provenance_sidecar(name: &str) -> Option<SkillProvenance> {
    let path = provenance_sidecar_path(name);
    let bytes = crate::fs::read_vfs(&path).ok()?;
    parse_provenance_bytes(&bytes)
}

fn register_wasm_skill_inner(
    bytecode: &[u8],
    name: &str,
    desc: &str,
    provenance: Option<SkillProvenance>,
) -> Result<(), &'static str> {
    let module = parse_wasm(bytecode)?;
    // Lane B: todo registro WASM instala o executor no DynamicSkill —
    // unifica execução (mesh/promote/reload executam no wasmi; sem bridge
    // o DynamicSkill segue fail-closed).
    ensure_dynskill_bridge();
    let skill = WasmSkill::new(bytecode, name, desc, module.exports.clone());
    crate::globals::SKILL_REGISTRY.lock().register(Box::new(skill));
    crate::self_evolve::publish_change("wasm", name);
    {
        let mut bridge = WASM_SKILL_BRIDGE.lock();
        bridge.skill_name = String::from(name);
        bridge.registered = true;
    }
    if let Some(prov) = provenance {
        record_skill_provenance(name, prov);
        k_nano::slog_hermes!("Wasm", "ok", "Skill '{}' registrada ({} exports, prov={}).", name, module.exports.len(), prov.as_str());
    } else {
        k_nano::slog_hermes!("Wasm", "info", "Skill '{}' registrada com {} exports.", name, module.exports.len());
    }
    Ok(())
}

/// Executor real do DynamicSkill com `wasm` (ponte hermes→skill-registry).
/// Mesma política do `WasmSkill::execute`: `main`/`_start`/1º export + args
/// derivados do payload; refuse honesto, sem panic/unwrap.
pub fn dynskill_wasm_exec(wasm: &[u8], payload: &[u8]) -> Result<Vec<u8>, &'static str> {
    let module = parse_wasm(wasm)?;
    let func_name = module
        .exports
        .iter()
        .find(|e| e.name == "main" || e.name == "_start")
        .or_else(|| module.exports.first())
        .map(|e| e.name.clone())
        .unwrap_or_default();
    if func_name.is_empty() {
        return Err("WASM: nenhuma função exportada");
    }
    run_wasm_export(wasm, &func_name, payload)
}

/// Instala o executor wasmi no DynamicSkill (idempotente).
fn ensure_dynskill_bridge() {
    skill_registry::dynskill::install_wasm_exec_bridge(dynskill_wasm_exec);
}

fn run_wasm_export(
    bytecode: &[u8],
    func_name: &str,
    payload: &[u8],
) -> Result<Vec<u8>, &'static str> {
    let args = payload_to_args(payload);
    match run_wasm(bytecode, func_name, &args, CAP_LOG) {
        Ok(result) => Ok(alloc::format!("[WASM] {} → {}", func_name, result).into_bytes()),
        Err(e) => {
            if func_name == "main" || func_name == "_start" {
                run_wasm(bytecode, func_name, &[], CAP_LOG)
                    .map(|r| alloc::format!("[WASM] {} → {}", func_name, r).into_bytes())
                    .map_err(|_| e)
            } else {
                Err(e)
            }
        }
    }
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
            .map(|e| e.name.clone())
            .unwrap_or_default();

        if func_name.is_empty() {
            return Err("WASM: nenhuma função exportada");
        }
        run_wasm_export(&self.bytecode, &func_name, payload)
    }
}

#[cfg(test)]
mod lane_b_tests {
    use super::*;

    fn build_test_module() -> Vec<u8> {
        let ops = [
            crate::wasm_build::Op::LocalGet(0),
            crate::wasm_build::Op::I32Const(2),
            crate::wasm_build::Op::I32Mul,
            crate::wasm_build::Op::I32Const(1),
            crate::wasm_build::Op::I32Add,
        ];
        crate::wasm_build::build_run_module(1, &ops).expect("build")
    }

    #[test]
    fn parse_finds_run_export_of_built_module() {
        let wasm = build_test_module();
        let module = parse_wasm(&wasm).expect("parse");
        assert!(module.exports.iter().any(|e| e.name == "run" && e.kind == 0));
        assert_eq!(run_wasm(&wasm, "run", &[6], CAP_NONE).expect("run"), 13);
    }

    #[test]
    fn register_records_model_born_provenance() {
        let wasm = build_test_module();
        register_wasm_skill_with_provenance(&wasm, "lb_prov_a", "test", SkillProvenance::ModelBorn)
            .expect("register");
        assert_eq!(skill_provenance("lb_prov_a"), Some(SkillProvenance::ModelBorn));
        assert!(crate::globals::SKILL_REGISTRY.lock().has_skill("lb_prov_a"));
        crate::globals::SKILL_REGISTRY.lock().unregister("lb_prov_a");
    }

    #[test]
    fn dynskill_with_wasm_executes_via_bridge_after_register() {
        // O registro instala o bridge; DynamicSkill com wasm passa a executar.
        let wasm = build_test_module();
        register_wasm_skill_with_provenance(&wasm, "lb_bridge_a", "test", SkillProvenance::Template)
            .expect("register");
        let dyn_skill =
            skill_registry::DynamicSkill::with_wasm("lb_bridge_a", "d", "i", wasm);
        let out = skill_registry::Skill::execute(&dyn_skill, b"6").expect("bridge exec");
        let text = core::str::from_utf8(&out).expect("utf8");
        assert!(text.contains("13"), "esperava a*2+1 com a=6 → 13, veio {}", text);
        crate::globals::SKILL_REGISTRY.lock().unregister("lb_bridge_a");
    }

    #[test]
    fn sidecar_parse_covers_all_variants() {
        assert_eq!(parse_provenance_bytes(b"model-born"), Some(SkillProvenance::ModelBorn));
        assert_eq!(parse_provenance_bytes(b"template"), Some(SkillProvenance::Template));
        assert_eq!(parse_provenance_bytes(b"dummy"), Some(SkillProvenance::Dummy));
        assert_eq!(parse_provenance_bytes(b"reloaded"), Some(SkillProvenance::Reloaded));
        assert_eq!(parse_provenance_bytes(b"imported"), Some(SkillProvenance::Imported));
        assert_eq!(parse_provenance_bytes(b""), None);
        assert_eq!(parse_provenance_bytes(b"MODEL-BORN"), None);
        assert_eq!(parse_provenance_bytes(b"model-born\n"), None);
        assert_eq!(SkillProvenance::Imported.as_str(), "imported");
    }

    #[test]
    fn metrics_counters_are_monotonic() {
        let (ok0, fail0) = metrics_model_text();
        let (pok0, pden0) = metrics_promote();
        note_model_text_parse(true);
        note_model_text_parse(false);
        note_promote(true);
        note_promote(false);
        note_reload_ok();
        let (ok1, fail1) = metrics_model_text();
        let (pok1, pden1) = metrics_promote();
        assert!(ok1 >= ok0 + 1 && fail1 >= fail0 + 1);
        assert!(pok1 >= pok0 + 1 && pden1 >= pden0 + 1);
        assert!(metrics_reload_ok() >= 1);
    }
}




