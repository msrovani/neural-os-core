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
    // s429-lab: ALLOC-FREE — `to_lowercase()` aloca String; sob pressão de
    // memória (HeapAIOS pressure=1) o alloc pode devolver NULL → deref → #PF
    // no AP (cr2 baixo, ip em conversões unicode). Heurística ASCII-fold no
    // stack: copia o prefixo (256B) baixando ASCII; UTF-8 multibyte passa
    // intacto (comparação de keywords ASCII continua correta).
    let src = description.as_bytes();
    let n = src.len().min(256);
    let mut buf = [0u8; 256];
    for (i, b) in src[..n].iter().enumerate() {
        buf[i] = b.to_ascii_lowercase();
    }
    // trim manually (leading) — leading whitespace não afeta starts_with após fold
    let trimmed = &buf[..n];
    let trimmed = &trimmed[trimmed.iter().take_while(|b| **b == b' ' || **b == b'\t' || **b == b'\n' || **b == b'\r').count()..];
    if trimmed.starts_with(b"add") || trimmed.starts_with(b"sum") || trimmed.starts_with(b"+") {
        SkillPattern::Add
    } else if trimmed.starts_with(b"echo") || trimmed.starts_with(b"print") || trimmed.starts_with(b"say") {
        SkillPattern::Echo
    } else if contains_sub(trimmed, b"card")
        || contains_sub(trimmed, b"display")
        || contains_sub(trimmed, b"render")
        || contains_sub(trimmed, b"mostrar")
        || contains_sub(trimmed, b"show")
    {
        SkillPattern::Card
    } else {
        SkillPattern::Default
    }
}

/// subsequência byte-a-byte (slice::contains só aceita &u8).
fn contains_sub(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
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

/// Lane B: texto bruto do modelo → op-IR (expression/DSL, sem nova gramática)
/// → wasm → sandbox wasmi. Dummy recusa; fora-da-gramática recusa; tudo com
/// `Err` honesto, sem panic/unwrap. O registro/persistência é do `evolve`
/// (`promote_model_text_to_wasm`) — aqui só bytes validados.
pub fn decode_model_text_to_wasm(model_text: &str) -> Result<Vec<u8>, &'static str> {
    let (n_params, ops) = match crate::wasm_build::model_text_to_ops(model_text) {
        Ok(parsed) => {
            wasmi_rt::note_model_text_parse(true);
            parsed
        }
        Err(_) => {
            wasmi_rt::note_model_text_parse(false);
            return Err("harness: model-text fora da gramática");
        }
    };
    if crate::wasm_build::is_dummy_ops(&ops) {
        return Err("harness: dummy nunca é skill");
    }
    let wasm = crate::wasm_build::build_run_module(n_params, &ops)
        .map_err(|_| "harness: build-fail")?;
    if !wasmi_rt::sandbox_validate_and_run(&wasm) {
        return Err("harness: sandbox-fail");
    }
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

    #[test]
    fn model_text_to_wasm_round_trip() {
        let wasm = decode_model_text_to_wasm("a*b+7").expect("decode");
        assert_eq!(wasmi_rt::run_wasm(&wasm, "run", &[6, 7], 0).unwrap(), 49);
    }

    #[test]
    fn model_text_refuses_dummy_and_garbage() {
        assert!(decode_model_text_to_wasm("0").is_err());
        assert!(decode_model_text_to_wasm("42").is_err());
        assert!(decode_model_text_to_wasm("").is_err());
        assert!(decode_model_text_to_wasm("def foo():").is_err());
    }
}
