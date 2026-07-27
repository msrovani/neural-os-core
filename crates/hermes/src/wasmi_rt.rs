//! ADR-0059 Caminho A — Runtime WASM real (`wasmi`, no_std, fuel).
//!
//! Executa **módulos WebAssembly padrão** em sandbox (SFI + fuel + limite de
//! memória), com host-imports `aios::*` **gated por CapGate** e
//! `wasi_snapshot_preview1` stubs. É o backend **seguro por default** para
//! apps/skills geradas por IA (código não-confiável): nada de MMIO/DMA, tudo
//! mediado por capabilities, execução determinística com fuel (evita loop
//! infinito) — padrão MCP-SandboxScan / SelfEvolve.
//!
//! Substitui a VM `Op` custom (`wasm_exec.rs`) e o interpretador parcial
//! (`wasm.rs`) — aposentados pela ADR-0059.

use alloc::vec::Vec;
use core::sync::atomic::Ordering;
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
pub const CAP_ALL: u32     = 0xFFFF_FFFF;
pub const CAP_NONE: u32    = 0;

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
fn check_cap(caller: &wasmi::Caller<'_, HostState>, required: u32, namespace: &str, name: &str) -> Result<(), wasmi::core::Trap> {
    let held = caller.data().caps;
    // 1. Bitmask check
    if held & required == 0 {
        k_nano::telemetry::TELEMETRY.push(4, 0, &required.to_ne_bytes());
        return Err(wasmi::core::Trap::new("capability denied (bitmask)"));
    }
    // 2. PermissionGate escalate check
    // Membrane::check é chamado pelo PermissionGate internamente
    let verdict = crate::permission_gate::PermissionGate::check(namespace, name, crate::membrane::Verdict::Allow);
    match verdict {
        crate::permission_gate::PermissionVerdict::Allow => Ok(()),
        crate::permission_gate::PermissionVerdict::Deny => {
            k_nano::telemetry::TELEMETRY.push(4, 0, &required.to_ne_bytes());
            Err(wasmi::core::Trap::new("permission denied (gate)"))
        }
        crate::permission_gate::PermissionVerdict::Pending { id } => {
            // O PermissionGate já fez spin-wait; se chegou aqui é Allow ou Deny
            k_nano::slog_hermes!("WASMI", "warn", "Pending HITL #{} — should not reach here", id);
            Err(wasmi::core::Trap::new("HITL pending"))
        }
    }
}

/// Instala os host-imports `aios::*`, `aios_net::*`, `aios_fs::*`
/// e `wasi_snapshot_preview1` no linker, **gated por CapGate + PermissionGate**.
fn install_host_abi(linker: &mut Linker<HostState>) -> Result<(), &'static str> {
    // ── aios::log(ptr,len) ────────────────────────────────────────────────
    linker.func_wrap("aios", "log",
        |mut caller: wasmi::Caller<'_, HostState>, ptr: i32, len: i32| {
            check_cap(&caller, CAP_LOG, "aios", "log").unwrap();
            if let Some(wasmi::Extern::Memory(mem)) = caller.get_export("memory") {
                let data = mem.data(&caller);
                let (p, l) = (ptr as usize, len as usize);
                if p.saturating_add(l) <= data.len() {
                    let mut buf = Vec::with_capacity(l);
                    buf.extend_from_slice(&data[p..p + l]);
                    caller.data_mut().out.extend_from_slice(&buf);
                }
            }
            k_nano::telemetry::TELEMETRY.push(3, 0, &[0; 32]); // EV_WASM_CALL
        },
    ).map_err(|_| "linker aios::log")?;

    // ── aios::debug(i32) -> i32 ─────────────────────────────────────────────
    linker.func_wrap("aios", "debug",
        |caller: wasmi::Caller<'_, HostState>, val: i32| -> i32 {
            check_cap(&caller, CAP_LOG, "aios", "debug").unwrap();
            val
        },
    ).map_err(|_| "linker aios::debug")?;

    // ── aios::get_tick() -> i64 ─────────────────────────────────────────────
    linker.func_wrap("aios", "get_tick",
        |caller: wasmi::Caller<'_, HostState>| -> i64 {
            check_cap(&caller, CAP_LOG, "aios", "get_tick").unwrap();
            k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as i64
        },
    ).map_err(|_| "linker aios::get_tick")?;

    // ── aios_net::http_get(ptr,len) -> i32 ──────────────────────────────────
    linker.func_wrap("aios_net", "http_get",
        |caller: wasmi::Caller<'_, HostState>, _ptr: i32, _len: i32| -> i32 {
            check_cap(&caller, CAP_NET, "aios_net", "http_get").unwrap();
            -1 // ponytail: stub — sem HTTP real
        },
    ).map_err(|_| "linker aios_net::http_get")?;

    // ── aios_fs::fs_read(ptr,len,max) -> i32 ────────────────────────────────
    linker.func_wrap("aios_fs", "fs_read",
        |caller: wasmi::Caller<'_, HostState>, _ptr: i32, _len: i32, _max: i32| -> i32 {
            check_cap(&caller, CAP_FS, "aios_fs", "fs_read").unwrap();
            0 // ponytail: stub
        },
    ).map_err(|_| "linker aios_fs::fs_read")?;

    // ── aios_fs::fs_write(ptr,len) -> i32 ───────────────────────────────────
    linker.func_wrap("aios_fs", "fs_write",
        |caller: wasmi::Caller<'_, HostState>, _ptr: i32, _len: i32| -> i32 {
            check_cap(&caller, CAP_FS, "aios_fs", "fs_write").unwrap();
            0 // ponytail: stub
        },
    ).map_err(|_| "linker aios_fs::fs_write")?;

    // ── wasi_snapshot_preview1 ──────────────────────────────────────────────
    super::wasi_host::register_wasi_host_functions(linker)
        .map_err(|_| "linker wasi_snapshot_preview1")?;

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
    let module = Module::new(&engine, wasm).map_err(|_| "wasm: módulo inválido")?;
    let mut store = Store::new(&engine, HostState::new(caps));
    store.set_fuel(DEFAULT_FUEL).map_err(|_| "wasm: set_fuel")?;

    let mut linker = <Linker<HostState>>::new(&engine);
    install_host_abi(&mut linker)?;

    let instance = linker
        .instantiate(&mut store, &module)
        .map_err(|_| "wasm: instantiate (import negado/ausente?)")?
        .start(&mut store)
        .map_err(|_| "wasm: start")?;

    let func = instance
        .get_typed_func::<(i32, i32), i32>(&store, func_name)
        .map_err(|_| "wasm: export não encontrado")?;

    func.call(&mut store, (a, b)).map_err(|_| "wasm: trap/out-of-fuel")
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

/// Gera um módulo WASM mínimo com uma função exportada que retorna i32(42).
/// Usado pelo bridge ADR-0059 F3/F5 para criar bytecode dummy para evolução/hot-swap.
///
/// **Imports disponíveis no runtime:**
/// - `aios::log`, `aios::debug`, `aios::get_tick` (CAP_LOG)
/// - `aios_net::http_get` (CAP_NET)
/// - `aios_fs::fs_read`, `aios_fs::fs_write` (CAP_FS)
/// - `wasi_snapshot_preview1` (15 stubs: fd_write, clock_time_get, random_get,
///   path_open, proc_exit, fd_read, fd_fdstat_get, environ/args, etc.)
///
/// ponytail: módulo minimalista — só `_start` exportado, sem imports.
pub fn generate_wasm_module() -> Vec<u8> {
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
/// Fallback: tenta com 0 params se a assinatura exata falhar.
pub fn run_wasm(
    wasm: &[u8],
    func_name: &str,
    args: &[i32],
    caps: u32,
) -> Result<i32, &'static str> {
    let mut config = Config::default();
    config.consume_fuel(true);
    let engine = Engine::new(&config);
    let module = Module::new(&engine, wasm).map_err(|_| "wasm: módulo inválido")?;
    let mut store = Store::new(&engine, HostState::new(caps));
    store.set_fuel(DEFAULT_FUEL).map_err(|_| "wasm: set_fuel")?;
    let mut linker = <Linker<HostState>>::new(&engine);
    install_host_abi(&mut linker)?;
    let instance = linker
        .instantiate(&mut store, &module)
        .map_err(|_| "wasm: instantiate")?
        .start(&mut store)
        .map_err(|_| "wasm: start")?;
    for n_params in &[args.len(), 0] {
        let a = |i: usize| args.get(i).copied().unwrap_or(0);
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
            _ => return Err("wasm: muitos argumentos (max 4)"),
        };
        match r {
            Ok(val) => return Ok(val),
            Err(_) if *n_params == args.len() => continue,
            Err(_) => return Err("wasm: export não encontrado/assinatura"),
        }
    }
    Err("wasm: export não encontrado ou assinatura incompatível")
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






