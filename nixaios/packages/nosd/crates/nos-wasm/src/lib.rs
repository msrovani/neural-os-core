//! wasmi Frugal — heap 256 KiB, fuel 100_000 (política ADR-0118).

use nos_core::{CapBits, CapGate};
use thiserror::Error;
use wasmi::{Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

/// 256 KiB = 4 × 64 KiB WASM pages.
pub const FRUGAL_HEAP_BYTES: usize = 256 * 1024;
pub const FRUGAL_FUEL: u64 = 100_000;
pub const FRUGAL_MAX_PAGES: u32 = 4;

#[derive(Debug, Error)]
pub enum WasmError {
    #[error("cap: {0}")]
    Cap(&'static str),
    #[error("wasmi: {0}")]
    Wasmi(String),
    #[error("fuel exhausted")]
    Fuel,
}

pub struct HostState {
    pub gate: CapGate,
    pub current_gen: u64,
    pub out: Vec<u8>,
    pub limits: StoreLimits,
}

impl HostState {
    pub fn frugal(caps: CapBits, gen: u64) -> Self {
        let limits = StoreLimitsBuilder::new()
            .memory_size(FRUGAL_HEAP_BYTES)
            .memories(1)
            .tables(1)
            .instances(1)
            .build();
        Self {
            gate: CapGate::new(caps, gen),
            current_gen: gen,
            out: Vec::new(),
            limits,
        }
    }
}

pub struct FrugalRuntime {
    engine: Engine,
}

impl Default for FrugalRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl FrugalRuntime {
    pub fn new() -> Self {
        let mut config = Config::default();
        config.consume_fuel(true);
        Self {
            engine: Engine::new(&config),
        }
    }

    /// Executa export `run` (i32) → i32 com fuel/heap Frugal.
    pub fn run_i32(
        &self,
        wasm: &[u8],
        caps: CapBits,
        gen: u64,
        arg: i32,
    ) -> Result<i32, WasmError> {
        let module =
            Module::new(&self.engine, wasm).map_err(|e| WasmError::Wasmi(e.to_string()))?;
        let mut store = Store::new(&self.engine, HostState::frugal(caps, gen));
        store.limiter(|s| &mut s.limits);
        store
            .set_fuel(FRUGAL_FUEL)
            .map_err(|e| WasmError::Wasmi(e.to_string()))?;

        let linker = Linker::<HostState>::new(&self.engine);
        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| WasmError::Wasmi(e.to_string()))?
            .start(&mut store)
            .map_err(|e| WasmError::Wasmi(e.to_string()))?;

        let run = instance
            .get_typed_func::<i32, i32>(&store, "run")
            .map_err(|e| WasmError::Wasmi(e.to_string()))?;

        match run.call(&mut store, arg) {
            Ok(v) => Ok(v),
            Err(e) => {
                let msg = e.to_string();
                if msg.to_ascii_lowercase().contains("fuel") {
                    Err(WasmError::Fuel)
                } else {
                    Err(WasmError::Wasmi(msg))
                }
            }
        }
    }
}

/// WASM: (module (func (export "run") (param i32) (result i32) (local.get 0)))
pub fn tiny_identity_wasm() -> Vec<u8> {
    wat::parse_str(
        r#"(module
            (func (export "run") (param i32) (result i32)
                local.get 0))
        "#,
    )
    .expect("valid wat")
}

/// Loop que consome fuel: `arg` iterações de add (trap se fuel estourar).
pub fn fuel_burn_wasm() -> Vec<u8> {
    wat::parse_str(
        r#"(module
            (func (export "run") (param $n i32) (result i32)
                (local $i i32)
                (local $acc i32)
                (local.set $i (i32.const 0))
                (local.set $acc (i32.const 0))
                (block $done
                    (loop $body
                        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
                        (local.set $acc (i32.add (local.get $acc) (i32.const 1)))
                        (local.set $i (i32.add (local.get $i) (i32.const 1)))
                        (br $body)))
                (local.get $acc)))
        "#,
    )
    .expect("valid wat")
}

/// Resultado de frota frugal (medição host — ADR-0113).
#[derive(Debug, Clone)]
pub struct FleetReport {
    pub n: usize,
    pub ok: usize,
    pub fuel_traps: usize,
    pub other_err: usize,
    pub elapsed_ms: u128,
    pub heap_budget_kib: usize,
}

/// Corre `n` instâncias sequenciais do identity WASM (1 store por agente).
pub fn run_fleet_identity(n: usize) -> FleetReport {
    let rt = FrugalRuntime::new();
    let code = tiny_identity_wasm();
    let t0 = std::time::Instant::now();
    let mut ok = 0usize;
    let mut fuel_traps = 0usize;
    let mut other_err = 0usize;
    for i in 0..n {
        match rt.run_i32(&code, CapBits::ALL, 1, i as i32) {
            Ok(v) if v == i as i32 => ok += 1,
            Ok(_) => other_err += 1,
            Err(WasmError::Fuel) => fuel_traps += 1,
            Err(_) => other_err += 1,
        }
    }
    FleetReport {
        n,
        ok,
        fuel_traps,
        other_err,
        elapsed_ms: t0.elapsed().as_millis(),
        heap_budget_kib: (FRUGAL_HEAP_BYTES / 1024) * n,
    }
}

/// Provoca trap de fuel com loop grande.
pub fn expect_fuel_trap() -> bool {
    let rt = FrugalRuntime::new();
    matches!(
        rt.run_i32(&fuel_burn_wasm(), CapBits::ALL, 1, 5_000_000),
        Err(WasmError::Fuel)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frugal_identity() {
        let rt = FrugalRuntime::new();
        let v = rt
            .run_i32(&tiny_identity_wasm(), CapBits::ALL, 1, 42)
            .unwrap();
        assert_eq!(v, 42);
    }

    #[test]
    fn constants() {
        assert_eq!(FRUGAL_HEAP_BYTES, 256 * 1024);
        assert_eq!(FRUGAL_FUEL, 100_000);
        assert_eq!(FRUGAL_MAX_PAGES, 4);
    }

    #[test]
    fn fuel_trap_on_burn() {
        assert!(expect_fuel_trap(), "burn loop must exhaust Frugal fuel");
    }

    #[test]
    fn fleet_32_identity() {
        let r = run_fleet_identity(32);
        assert_eq!(r.ok, 32, "{r:?}");
        assert_eq!(r.fuel_traps, 0);
        assert_eq!(r.heap_budget_kib, 256 * 32);
        // Bound frouxo: host Windows/WSL; só evita regressão absurda
        assert!(r.elapsed_ms < 30_000, "fleet too slow: {r:?}");
    }

    #[test]
    fn fleet_128_identity() {
        let r = run_fleet_identity(128);
        assert_eq!(r.ok, 128, "{r:?}");
        assert_eq!(r.heap_budget_kib, 256 * 128);
    }
}
