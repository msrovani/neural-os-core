//! ADR-0059 — Runtime App Factory: seletor de caminho por IA + política de
//! execução (isolamento / CapGate / HW-gate / HITL forte).
//!
//! Fluxo: Hermes pede → Cortex/Trinity/LLM geram → **a IA analisa e recomenda**
//! um dos 3 backends; o **usuário/HITL decide**; a execução é **mediada por
//! CapGate + HW-gate**. Backends:
//!
//! - **A `WasmInterp`** (wasmi): sandbox seguro por default; código não-confiável.
//! - **B `WasmJit`** (Cranelift wasm→nativo): mais rápido, mantém semântica wasm;
//!   feature `jit-cranelift`. Exige ring de isolamento p/ o código nativo.
//! - **C `NativeRustSubset`** (Cranelift + front Rust-subset, à la rustc-lite):
//!   compila Rust-subset a nativo; **sem sandbox wasm** → exige ring de
//!   isolamento + HITL forte. Trilha self-hosting (self-update/self-improve).
//!
//! Segurança: B e C geram **código nativo**; até existir um ring de isolamento
//! dedicado (paging/Ring3 — ADR-0041), a execução nativa fica **gated**
//! (AWAITING + requires-HITL). O Caminho A executa já, com segurança total.

use alloc::string::String;
use core::sync::atomic::{AtomicUsize, Ordering};

/// ADR-0060 seam — ring de isolamento nativo (Ring3). O `neural-kernel`
/// registra aqui um executor nativo isolado QUANDO o F6 estiver validado
/// (§6 da ADR-0060). Enquanto não registrado, B/C nativo fica gated.
pub type NativeRingFn = fn(code: &[u8], caps: u32) -> Result<i64, &'static str>;
static NATIVE_RING: AtomicUsize = AtomicUsize::new(0);

/// Registrado por `neural-kernel::isolation_ring` só após o ring passar o gate.
pub fn register_native_ring(f: NativeRingFn) {
    NATIVE_RING.store(f as usize, Ordering::Release);
    k_nano::slog_hermes!("APPFACTORY", "info", "native isolation ring REGISTRADO (ADR-0060) — B/C liberável sob HITL");
}

pub fn native_ring_registered() -> bool {
    NATIVE_RING.load(Ordering::Acquire) != 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppBackend {
    /// A — interpretador wasmi (seguro; default p/ IA não-confiável).
    WasmInterp,
    /// B — Cranelift JIT de wasm→nativo (rápido; sandbox wasm mantido).
    WasmJit,
    /// C — Cranelift + Rust-subset → nativo (self-hosting; sem sandbox wasm).
    NativeRustSubset,
}

/// Pedido de app/skill gerada.
#[derive(Clone)]
pub struct AppRequest {
    pub desc: String,
    /// Fonte confiável (assinada/verificada) vs. gerada por IA não-confiável.
    pub trusted: bool,
    /// Precisa de throughput alto (matmul, loop pesado)?
    pub perf_critical: bool,
    /// A IA quer emitir Rust (subset) em vez de WAT/DSL?
    pub wants_rust: bool,
}

/// Recomendação da IA (backend + justificativa + exigências de segurança).
#[derive(Clone)]
pub struct Recommendation {
    pub backend: AppBackend,
    pub rationale: String,
    /// Requer aprovação humana antes de executar (HITL forte).
    pub requires_hitl: bool,
    /// Requer ring de isolamento p/ código nativo (HW-gate).
    pub requires_isolation_ring: bool,
    /// Capabilities mínimas (CapGate) a conceder.
    pub caps_needed: u32,
}

/// A IA analisa o pedido e **recomenda** um caminho (o usuário decide).
///
/// Política honesta:
/// - IA não-confiável (default) → **A** (sandbox máximo).
/// - confiável + perf-critical → **B** (JIT, ainda sandbox wasm) — HITL leve.
/// - confiável + quer Rust/self-hosting → **C** (nativo) — HITL forte + ring.
pub fn analyze_and_recommend(req: &AppRequest) -> Recommendation {
    if !req.trusted {
        return Recommendation {
            backend: AppBackend::WasmInterp,
            rationale: String::from("código gerado por IA (não-confiável) → sandbox wasmi seguro"),
            requires_hitl: false,
            requires_isolation_ring: false,
            caps_needed: 0,
        };
    }
    if req.wants_rust {
        return Recommendation {
            backend: AppBackend::NativeRustSubset,
            rationale: String::from("Rust-subset self-hosting → Cranelift nativo (exige HITL forte + ring de isolamento)"),
            requires_hitl: true,
            requires_isolation_ring: true,
            caps_needed: 0,
        };
    }
    if req.perf_critical {
        return Recommendation {
            backend: AppBackend::WasmJit,
            rationale: String::from("perf-critical + confiável → Cranelift JIT de wasm (sandbox mantido)"),
            requires_hitl: true,
            requires_isolation_ring: true,
            caps_needed: 0,
        };
    }
    Recommendation {
        backend: AppBackend::WasmInterp,
        rationale: String::from("default seguro → wasmi"),
        requires_hitl: false,
        requires_isolation_ring: false,
        caps_needed: 0,
    }
}

/// Resultado da fábrica.
pub enum FactoryOutcome {
    /// Executado no sandbox wasmi (Caminho A). Valor de retorno i32.
    RanWasm(i32),
    /// Executado no ring de isolamento nativo (B/C) — só após ADR-0060.
    RanNative(i64),
    /// Backend nativo (B/C) pendente de ring de isolamento + HITL — honesto.
    AwaitingIsolation(AppBackend),
    /// Negado por CapGate/HITL.
    Denied(&'static str),
}

/// Executa um artefato conforme a recomendação, **aplicando as barreiras**:
/// CapGate (imports), HW-gate (ring nativo) e HITL. Caminho A roda de verdade;
/// B/C ficam gated (AWAITING) até o ring de isolamento (ADR-0041) + aprovação.
pub fn execute(
    rec: &Recommendation,
    wasm_or_src: &[u8],
    entry: &str,
    a: i32,
    b: i32,
) -> FactoryOutcome {
    // HW-gate: código nativo (B/C) só com ring de isolamento habilitado.
    if rec.requires_isolation_ring && !isolation_ring_available() {
        k_nano::slog_hermes!(
            "APPFACTORY",
            "info",
            "VERDICT=AWAITING_ISOLATION backend={:?} reason=native_needs_ring (HITL={})",
            rec.backend,
            rec.requires_hitl
        );
        return FactoryOutcome::AwaitingIsolation(rec.backend);
    }
    match rec.backend {
        AppBackend::WasmInterp => match crate::wasmi_rt::run_i32_2(wasm_or_src, entry, a, b, rec.caps_needed) {
            Ok(v) => {
                k_nano::slog_hermes!("APPFACTORY", "info", "RanWasm backend=A entry={} ret={}", entry, v);
                FactoryOutcome::RanWasm(v)
            }
            Err(e) => FactoryOutcome::Denied(e),
        },
        // B/C: se o ring nativo (ADR-0060) foi registrado, executa nele; senão
        // AWAITING (residual F6). Chegar aqui já passou pelo HW-gate acima.
        AppBackend::WasmJit | AppBackend::NativeRustSubset => {
            let slot = NATIVE_RING.load(Ordering::Acquire);
            if slot != 0 {
                let f: NativeRingFn = unsafe { core::mem::transmute::<usize, NativeRingFn>(slot) };
                match f(wasm_or_src, rec.caps_needed) {
                    Ok(v) => FactoryOutcome::RanNative(v),
                    Err(e) => FactoryOutcome::Denied(e),
                }
            } else {
                k_nano::slog_hermes!(
                    "APPFACTORY",
                    "info",
                    "VERDICT=AWAITING_ISOLATION backend={:?} reason=ring3_pending (ADR-0060)",
                    rec.backend
                );
                FactoryOutcome::AwaitingIsolation(rec.backend)
            }
        }
    }
}

/// ADR-0059 F3 — fim-a-fim "IA gera → monta → executa": recebe a **op-IR**
/// (gerada por Cortex/Trinity/LLM, constrangida por #412), monta o wasm e roda
/// pelo caminho recomendado. Código de IA = não-confiável → **A (wasmi)**.
pub fn generate_and_run(ops: &[crate::wasm_build::Op], a: i32, b: i32) -> FactoryOutcome {
    let req = AppRequest {
        desc: String::from("skill gerada por IA (op-IR)"),
        trusted: false,
        perf_critical: false,
        wants_rust: false,
    };
    let rec = analyze_and_recommend(&req);
    let wasm = match crate::wasm_build::build_run_module(2, ops) {
        Ok(w) => w,
        Err(e) => return FactoryOutcome::Denied(e),
    };
    execute(&rec, &wasm, "run", a, b)
}

/// HW-gate: ring de isolamento para código nativo (ADR-0060 / Ring3). Reflete
/// se um ring nativo **validado** foi registrado (`register_native_ring`).
/// Hoje `false` até o F6 (ADR-0060) passar o gate — B/C nativo fica gated.
pub fn isolation_ring_available() -> bool {
    native_ring_registered()
}

/// Self-test (sem modelo): valida o seletor + execução A end-to-end.
pub fn self_test() -> bool {
    // IA não-confiável → recomenda A; executa o add.wasm no sandbox.
    let req = AppRequest {
        desc: String::from("soma dois numeros"),
        trusted: false,
        perf_critical: false,
        wants_rust: false,
    };
    let rec = analyze_and_recommend(&req);
    let ok_rec = rec.backend == AppBackend::WasmInterp && !rec.requires_hitl;
    // Reusa o módulo add do wasmi_rt via run (Caminho A real).
    let ran = matches!(crate::wasmi_rt::self_test(), true);
    // C exige HITL + ring (gated).
    let req_c = AppRequest { desc: String::from("x"), trusted: true, perf_critical: false, wants_rust: true };
    let rec_c = analyze_and_recommend(&req_c);
    let ok_c = rec_c.backend == AppBackend::NativeRustSubset && rec_c.requires_hitl && rec_c.requires_isolation_ring;
    // F3/F4: pipeline fim-a-fim gera(op-IR)→monta(wasm)→sandbox(wasmi).
    // op-IR de (a+b)*2 : [LocalGet0, LocalGet1, I32Add, I32Const2, I32Mul]
    use crate::wasm_build::Op;
    let ops = [Op::LocalGet(0), Op::LocalGet(1), Op::I32Add, Op::I32Const(2), Op::I32Mul];
    let ok_gen = matches!(generate_and_run(&ops, 3, 4), FactoryOutcome::RanWasm(14));
    if ok_gen {
        k_nano::slog_hermes!("APPFACTORY", "info", "gera→monta→sandbox PASS ((3+4)*2=14) — ADR-0059 F3/F4");
    }
    let ok = ok_rec && ran && ok_c && ok_gen;
    if ok {
        k_nano::slog_hermes!("APPFACTORY", "info", "path-selector self-test PASS (A=run, C=HITL+ring gated) — ADR-0059");
    } else {
        k_nano::slog_hermes!("APPFACTORY", "warn", "path-selector self-test FAIL");
    }
    ok
}
