//! ADR-0059 F4 — Montador WASM a partir de uma IR de ops (op-IR).
//!
//! Este é o "assembler" seguro do pipeline "app feita por IA em runtime": a IA
//! (Cortex/Trinity/LLM) emite uma **lista de `Op`** (op-IR) — o alvo constrangido
//! pela gramática (ADR-0057 #412) — e este módulo **monta um módulo WebAssembly
//! válido** (bytes) que roda no sandbox `wasmi` (Caminho A).
//!
//! Por que op-IR em vez de WAT livre: o conjunto de `Op` é pequeno e fechado, e
//! o builder **garante wasm válido por construção** (índices/tipos checados) —
//! a IA não consegue emitir wasm inválido. É a "gramática" prática: o LLM só
//! escolhe entre `Op`s permitidos; nada de bytes arbitrários.
//!
//! Escopo (subset seguro, expansível): função exportada `run` com N params i32
//! e 1 resultado i32; ops de pilha i32 (const/local/add/sub/mul). Sem memória,
//! sem imports, sem loops → determinístico e trivialmente seguro (+ fuel wasmi).

use alloc::vec::Vec;

/// Instrução da op-IR (alvo constrangido da geração por IA).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// Empilha o parâmetro local `idx` (0..n_params).
    LocalGet(u32),
    /// Empilha uma constante i32.
    I32Const(i32),
    I32Add,
    I32Sub,
    I32Mul,
}

fn uleb(mut n: u64, out: &mut Vec<u8>) {
    loop {
        let mut b = (n & 0x7f) as u8;
        n >>= 7;
        if n != 0 {
            b |= 0x80;
        }
        out.push(b);
        if n == 0 {
            break;
        }
    }
}

fn sleb(mut v: i64, out: &mut Vec<u8>) {
    loop {
        let mut b = (v & 0x7f) as u8;
        v >>= 7;
        let sign = b & 0x40;
        let more = !((v == 0 && sign == 0) || (v == -1 && sign != 0));
        if more {
            b |= 0x80;
        }
        out.push(b);
        if !more {
            break;
        }
    }
}

fn section(id: u8, content: &[u8], out: &mut Vec<u8>) {
    out.push(id);
    uleb(content.len() as u64, out);
    out.extend_from_slice(content);
}

/// Valida que a op-IR não subfluxa a pilha e usa apenas locais válidos.
/// (Segurança extra além do fuel/SFI do wasmi.)
pub fn validate(n_params: u32, ops: &[Op]) -> Result<(), &'static str> {
    let mut depth: i32 = 0;
    for op in ops {
        match op {
            Op::LocalGet(i) => {
                if *i >= n_params {
                    return Err("op-IR: local fora de faixa");
                }
                depth += 1;
            }
            Op::I32Const(_) => depth += 1,
            Op::I32Add | Op::I32Sub | Op::I32Mul => {
                if depth < 2 {
                    return Err("op-IR: stack underflow em binop");
                }
                depth -= 1;
            }
        }
    }
    if depth != 1 {
        return Err("op-IR: deve sobrar exatamente 1 valor (result i32)");
    }
    Ok(())
}

/// Monta um módulo WASM: `(func (export "run")(param i32 x N)(result i32) <ops>)`.
/// Retorna os bytes `.wasm` prontos para o `wasmi`.
pub fn build_run_module(n_params: u32, ops: &[Op]) -> Result<Vec<u8>, &'static str> {
    validate(n_params, ops)?;

    let mut out = Vec::new();
    out.extend_from_slice(&[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]); // magic+version

    // Type section: 1 tipo (N i32) -> (i32)
    let mut ty = Vec::new();
    uleb(1, &mut ty); // 1 tipo
    ty.push(0x60);
    uleb(n_params as u64, &mut ty);
    for _ in 0..n_params {
        ty.push(0x7f); // i32
    }
    uleb(1, &mut ty); // 1 result
    ty.push(0x7f);
    section(0x01, &ty, &mut out);

    // Function section: 1 func, tipo 0
    let mut fun = Vec::new();
    uleb(1, &mut fun);
    uleb(0, &mut fun);
    section(0x03, &fun, &mut out);

    // Export section: "run" -> func 0
    let mut exp = Vec::new();
    uleb(1, &mut exp);
    let name = b"run";
    uleb(name.len() as u64, &mut exp);
    exp.extend_from_slice(name);
    exp.push(0x00); // kind func
    uleb(0, &mut exp);
    section(0x07, &exp, &mut out);

    // Code section: 1 body (0 locals + ops + end)
    let mut body = Vec::new();
    uleb(0, &mut body); // 0 grupos de locais
    for op in ops {
        match op {
            Op::LocalGet(i) => {
                body.push(0x20);
                uleb(*i as u64, &mut body);
            }
            Op::I32Const(v) => {
                body.push(0x41);
                sleb(*v as i64, &mut body);
            }
            Op::I32Add => body.push(0x6a),
            Op::I32Sub => body.push(0x6b),
            Op::I32Mul => body.push(0x6c),
        }
    }
    body.push(0x0b); // end

    let mut code = Vec::new();
    uleb(1, &mut code); // 1 body
    uleb(body.len() as u64, &mut code);
    code.extend_from_slice(&body);
    section(0x0a, &code, &mut out);

    Ok(out)
}

/// F3 bridge: monta o módulo a partir da op-IR e **roda no sandbox wasmi**
/// (2 params i32). Fim-a-fim "gera → monta → executa" (Caminho A).
pub fn build_and_run_2(ops: &[Op], a: i32, b: i32) -> Result<i32, &'static str> {
    let wasm = build_run_module(2, ops)?;
    crate::wasmi_rt::run_i32_2(&wasm, "run", a, b, 0)
}

/// Dica de schema da op-IR para o LLM (ADR-0057 #412 constrange a isto).
pub fn op_ir_schema_hint() -> &'static str {
    concat!(
        "Gere só uma lista de ops i32 (op-IR): ",
        "LocalGet(idx) | I32Const(n) | I32Add | I32Sub | I32Mul. ",
        "Função run(param i32 x N) -> i32; a pilha deve terminar com 1 valor."
    )
}

/// Self-test (sem modelo): op-IR de `a*b + 7` → monta wasm → roda no wasmi.
/// Prova o pipeline gera(op-IR)→monta(wasm)→sandbox(wasmi)→resultado.
pub fn self_test() -> bool {
    // run(a,b) = a*b + 7 : [LocalGet0, LocalGet1, I32Mul, I32Const7, I32Add]
    let ops = [
        Op::LocalGet(0),
        Op::LocalGet(1),
        Op::I32Mul,
        Op::I32Const(7),
        Op::I32Add,
    ];
    match build_and_run_2(&ops, 6, 7) {
        Ok(v) if v == 49 => {
            // 6*7 + 7 = 49
            k_nano::slog_hermes!(
                "WASM-BUILD",
                "info",
                "op-IR→wasm→wasmi self-test PASS (a*b+7: 6,7 -> {}) — ADR-0059 F4",
                v
            );
            true
        }
        Ok(v) => {
            k_nano::slog_hermes!("WASM-BUILD", "warn", "self-test resultado inesperado: {}", v);
            false
        }
        Err(e) => {
            k_nano::slog_hermes!("WASM-BUILD", "warn", "self-test FAIL: {}", e);
            false
        }
    }
}

