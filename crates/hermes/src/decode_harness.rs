//! ADR-0059 F4 — Structured Decode Harness (PONYTAIL).
//!
//! Ponte entre o `StructuredDecoder` (FSM de saída) e o gerador de módulos
//! WASM. Em vez de um assembler WAT→wasm completo (~800 LOC), este módulo
//! implementa um **reconhecedor de padrões** mínimo:
//!
//! - "add <a> <b>" → gera módulo WASM com `(func (export "main") (param i32 i32) (result i32) ...)`
//! - "echo <msg>"  → gera módulo com `_start` que retorna 42
//! - default        → gera módulo dummy (_start → i32(42))
//!
//! ## Upgrade path
//! Quando o crate `wat` suportar no_std, substituir por `wat::parse_str()` real.

use alloc::vec::Vec;
use crate::wasmi_rt;
use crate::structured_decode::{StructuredDecoder, DecodeMode};

/// Padrão reconhecido pelo harness.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SkillPattern {
    /// Soma dois números: "add 3 5" → WASM com add(i32,i32)→i32
    Add,
    /// Ecoa mensagem: "echo hello" → WASM _start retorna 42
    Echo,
    /// Qualquer outra descrição → WASM dummy _start retorna 42
    Default,
}

/// Analisa uma descrição textual e retorna o padrão reconhecido.
// ponytail: reconhecimento por prefixo simples — sem parser CFG.
pub fn recognize(description: &str) -> SkillPattern {
    let trimmed = description.trim().to_lowercase();
    if trimmed.starts_with("add") || trimmed.starts_with("sum") || trimmed.starts_with("+") {
        SkillPattern::Add
    } else if trimmed.starts_with("echo") || trimmed.starts_with("print") || trimmed.starts_with("say") {
        SkillPattern::Echo
    } else {
        SkillPattern::Default
    }
}

/// Gera bytecode WASM a partir de um padrão reconhecido.
pub fn generate_from_pattern(pattern: SkillPattern, _description: &str) -> Vec<u8> {
    match pattern {
        SkillPattern::Add => generate_add_wasm(),
        SkillPattern::Echo | SkillPattern::Default => wasmi_rt::generate_wasm_module(),
    }
}

/// Gera módulo WASM com `(func (export "main") (param i32 i32) (result i32) local.get 0; local.get 1; i32.add; end)`.
fn generate_add_wasm() -> Vec<u8> {
    let mut wasm = Vec::with_capacity(48);
    // magic + version
    wasm.extend_from_slice(&[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);
    // type section: (i32,i32)->i32
    wasm.push(0x01); wasm.push(0x07); wasm.push(0x01);
    wasm.push(0x60); wasm.push(0x02); wasm.push(0x7f); wasm.push(0x7f);
    wasm.push(0x01); wasm.push(0x7f);
    // func section: 1 func, type 0
    wasm.push(0x03); wasm.push(0x02); wasm.push(0x01); wasm.push(0x00);
    // export section: "main" func 0
    wasm.push(0x07); wasm.push(0x09); wasm.push(0x01);
    wasm.push(0x04); // name length = 4
    wasm.extend_from_slice(b"main");
    wasm.push(0x00); // kind = func
    wasm.push(0x00); // func_idx = 0
    // code section: local.get 0; local.get 1; i32.add; end
    wasm.push(0x0a); wasm.push(0x09); wasm.push(0x01);
    wasm.push(0x07); wasm.push(0x00); // locals: 0
    wasm.push(0x20); wasm.push(0x00); // local.get 0
    wasm.push(0x20); wasm.push(0x01); // local.get 1
    wasm.push(0x6a);                  // i32.add
    wasm.push(0x0b);                  // end
    wasm
}

/// Pipeline completo: reconhece descrição → gera WASM → valida no wasmi.
pub fn decode_and_generate(description: &str, _decoder: &mut StructuredDecoder) -> Result<Vec<u8>, &'static str> {
    let pattern = recognize(description);
    let wasm = generate_from_pattern(pattern, description);
    // Valida no wasmi (tenta _start, depois main)
    wasmi_rt::run_wasm(&wasm, "_start", &[], 0)
        .or_else(|_| wasmi_rt::run_wasm(&wasm, "main", &[], 0))
        .map_err(|_| "harness: wasm inválido")?;
    Ok(wasm)
}

/// Self-test: reconhece "add 3 5" → gera WASM → executa no wasmi → 8.
pub fn self_test() -> bool {
    let mut decoder = StructuredDecoder::new(DecodeMode::Alpha);
    let desc = "add 3 5";
    match decode_and_generate(desc, &mut decoder) {
        Ok(wasm) => match wasmi_rt::run_wasm(&wasm, "main", &[3, 5], 0) {
            Ok(8) => {
                k_nano::slog_hermes!("DECODE_HARNESS", "info", "F4 self-test PASS (add 3 5 = 8) — ADR-0059");
                true
            }
            Ok(v) => {
                k_nano::slog_hermes!("DECODE_HARNESS", "info", "F4 self-test: add(3,5) = {} (esperado 8)", v);
                false
            }
            Err(e) => {
                k_nano::slog_hermes!("DECODE_HARNESS", "info", "F4 self-test FAIL: {}", e);
                false
            }
        },
        Err(e) => {
            k_nano::slog_hermes!("DECODE_HARNESS", "info", "F4 self-test FAIL: {}", e);
            false
        }
    }
}






