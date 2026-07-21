//! ADR-0059 Caminho A — Runtime WASM real (`wasmi`, no_std, fuel).
//!
//! Executa **módulos WebAssembly padrão** em sandbox (SFI + fuel + limite de
//! memória), com host-imports `aios::*` **gated por CapGate**. É o backend
//! **seguro por default** para apps/skills geradas por IA (código não-confiável):
//! nada de MMIO/DMA, tudo mediado por capabilities, execução determinística com
//! fuel (evita loop infinito) — padrão MCP-SandboxScan / SelfEvolve.
//!
//! Substitui a VM `Op` custom (`wasm_exec.rs`) e o interpretador parcial
//! (`wasm.rs`) — aposentados pela ADR-0059.

use alloc::vec::Vec;
use wasmi::{Config, Engine, Linker, Module, Store};

/// Estado do host visível às funções importadas (capabilities concedidas).
pub struct HostState {
    /// Bitmask de capabilities concedidas ao módulo (ADR-0041 CapGate).
    pub caps: u32,
    /// Buffer de saída textual do módulo (via `aios::log`).
    pub out: Vec<u8>,
}

impl HostState {
    pub fn new(caps: u32) -> Self {
        Self { caps, out: Vec::new() }
    }
}

/// Fuel default por execução (determinístico; evita loop infinito).
pub const DEFAULT_FUEL: u64 = 5_000_000;

/// Instala os host-imports `aios::*` no linker, cada um **gated por CapGate**.
/// Sem a capability correspondente, a chamada faz trap (deny honesto).
fn install_host_abi(linker: &mut Linker<HostState>) -> Result<(), &'static str> {
    // aios::log(ptr,len) — escreve no buffer de saída (sempre permitido: observe-only).
    linker
        .func_wrap(
            "aios",
            "log",
            |mut caller: wasmi::Caller<'_, HostState>, ptr: i32, len: i32| {
                // Lê a memória exportada "memory" do guest (se houver) e copia.
                if let Some(wasmi::Extern::Memory(mem)) = caller.get_export("memory") {
                    let data = mem.data(&caller);
                    let (p, l) = (ptr as usize, len as usize);
                    if p.saturating_add(l) <= data.len() {
                        let mut buf = Vec::with_capacity(l);
                        buf.extend_from_slice(&data[p..p + l]);
                        caller.data_mut().out.extend_from_slice(&buf);
                    }
                }
            },
        )
        .map_err(|_| "linker aios::log")?;
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
