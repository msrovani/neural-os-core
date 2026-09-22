//! ADR-0059 F4 — Structured Decode Harness (PONYTAIL).
//!
//! Ponte entre o `StructuredDecoder` (FSM de saída) e o gerador de módulos
//! WASM. Reconhecedor de padrões mínimo:
//!
//! - "add <a> <b>" → WASM `main(i32,i32)->i32` add real
//! - "echo …" / default → dummy `_start→42` (não ecoa payload — honesty)
//! - "card …" → **Err(card_ir_pending)** — não vende dummy como card UI

use alloc::vec::Vec;
use crate::wasmi_rt;
use crate::structured_decode::{StructuredDecoder, DecodeMode};

/// Padrão reconhecido pelo harness.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SkillPattern {
    /// Soma dois números: "add 3 5" → WASM com add(i32,i32)→i32
    Add,
    /// Dummy `_start→42` (não ecoa a mensagem)
    Echo,
    /// Card UI — exige grammar/UiDeclaration; sem bytecode fake
    Card,
    /// Dummy `_start→42`
    Default,
}

/// Analisa uma descrição textual e retorna o padrão reconhecido.
pub fn recognize(description: &str) -> SkillPattern {
    let trimmed = description.trim().to_lowercase();
    if trimmed.starts_with("add") || trimmed.starts_with("sum") || trimmed.starts_with("+") {
        SkillPattern::Add
    } else if trimmed.starts_with("echo") || trimmed.starts_with("print") || trimmed.starts_with("say") {
        SkillPattern::Echo
    } else if trimmed.contains("card")
        || trimmed.contains("display")
        || trimmed.contains("render")
        || trimmed.contains("mostrar")
        || trimmed.contains("show")
    {
        SkillPattern::Card
    } else {
        SkillPattern::Default
    }
}

/// Gera bytecode WASM a partir de um padrão — Card falha (sem IR).
pub fn generate_from_pattern(
    pattern: SkillPattern,
    _description: &str,
) -> Result<Vec<u8>, &'static str> {
    match pattern {
        SkillPattern::Add => Ok(generate_add_wasm()),
        // H8 (canvas onda 1): sem gerador de bytes honesto — errar, não dummy.
        SkillPattern::Echo | SkillPattern::Default => Err("no-wasm-bytes"),
        SkillPattern::Card => Err("card_ir_pending"),
    }
}

fn generate_add_wasm() -> Vec<u8> {
    let mut wasm = Vec::with_capacity(48);
    wasm.extend_from_slice(&[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);
    wasm.push(0x01); wasm.push(0x07); wasm.push(0x01);
    wasm.push(0x60); wasm.push(0x02); wasm.push(0x7f); wasm.push(0x7f);
    wasm.push(0x01); wasm.push(0x7f);
    wasm.push(0x03); wasm.push(0x02); wasm.push(0x01); wasm.push(0x00);
    wasm.push(0x07); wasm.push(0x09); wasm.push(0x01);
    wasm.push(0x04);
    wasm.extend_from_slice(b"main");
    wasm.push(0x00);
    wasm.push(0x00);
    wasm.push(0x0a); wasm.push(0x09); wasm.push(0x01);
    wasm.push(0x07); wasm.push(0x00);
    wasm.push(0x20); wasm.push(0x00);
    wasm.push(0x20); wasm.push(0x01);
    wasm.push(0x6a);
    wasm.push(0x0b);
    wasm
}

/// Pipeline: reconhece → gera WASM → valida no wasmi.
pub fn decode_and_generate(
    description: &str,
    _decoder: &mut StructuredDecoder,
) -> Result<Vec<u8>, &'static str> {
    let pattern = recognize(description);
    let wasm = generate_from_pattern(pattern, description)?;
    wasmi_rt::run_wasm(&wasm, "_start", &[], 0)
        .or_else(|_| wasmi_rt::run_wasm(&wasm, "main", &[], 0))
        .map_err(|_| "harness: wasm inválido")?;
    Ok(wasm)
}

/// Self-test: reconhece "add 3 5" → gera WASM → executa → 8.
pub fn self_test() -> bool {
    let mut decoder = StructuredDecoder::new(DecodeMode::Alpha);
    let desc = "add 3 5";
    match decode_and_generate(desc, &mut decoder) {
        Ok(wasm) => match wasmi_rt::run_wasm(&wasm, "main", &[3, 5], 0) {
            Ok(8) => {
                k_nano::slog_hermes!(
                    "DECODE_HARNESS",
                    "ok",
                    "F4 self-test PASS (add 3 5 = 8) — ADR-0059"
                );
                true
            }
            Ok(v) => {
                k_nano::slog_hermes!(
                    "DECODE_HARNESS",
                    "warn",
                    "F4 self-test: add(3,5) = {} (esperado 8)",
                    v
                );
                false
            }
            Err(e) => {
                k_nano::slog_hermes!("DECODE_HARNESS", "fail", "F4 self-test FAIL: {}", e);
                false
            }
        },
        Err(e) => {
            k_nano::slog_hermes!("DECODE_HARNESS", "fail", "F4 self-test FAIL: {}", e);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_pattern_does_not_emit_dummy_wasm() {
        assert_eq!(recognize("show card clima"), SkillPattern::Card);
        assert_eq!(
            generate_from_pattern(SkillPattern::Card, "card"),
            Err("card_ir_pending")
        );
    }
}
