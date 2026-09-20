//! Gate de evidência LLM (lab/QEMU) — funções puras, testáveis no host.
//!
//! Contrato (SESSION clima mesh): **submit ≠ resposta**.
//! Aceite de geração = `prefill_done` + (`decode_tok/s=` | `done id=`) e/ou
//! stream `MSG_DELTA` / tópico `LLM_RESPONSE` no serial.
//! `InferQueue submit` sozinho = PASS falso (já observado).

#![allow(dead_code)]

/// Classificação de uma linha de serial/boot log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmLogHit {
    IntentLinj,
    IntentTexto,
    InferSubmit,
    PrefillBegin,
    PrefillDone,
    DecodeToks,
    JobDone,
    StreamDelta,
    HermesResponse,
    TtsOut,
}

/// Extrai hits relevantes de uma linha de log (ASCII/UTF-8).
pub fn classify_line(line: &str) -> Option<LlmLogHit> {
    let l = line;
    if l.contains("inject USER_INTENT (LINJ)") {
        return Some(LlmLogHit::IntentLinj);
    }
    if l.contains("CORTEX") && l.contains("Texto:") {
        return Some(LlmLogHit::IntentTexto);
    }
    if l.contains("InferQueue submit") || (l.contains("InferQ") && l.contains("submit id=")) {
        return Some(LlmLogHit::InferSubmit);
    }
    if l.contains("prefill_begin") {
        return Some(LlmLogHit::PrefillBegin);
    }
    if l.contains("prefill_done") {
        return Some(LlmLogHit::PrefillDone);
    }
    if l.contains("decode_tok/s=") {
        return Some(LlmLogHit::DecodeToks);
    }
    // finish_job: "done id=N len=M in_flight=..."
    if l.contains("InferQ") && l.contains("done id=") {
        return Some(LlmLogHit::JobDone);
    }
    if l.contains("MSG_DELTA|") || l.contains("LLM_STREAM") {
        return Some(LlmLogHit::StreamDelta);
    }
    if l.contains("HERMES_RESPONSE") {
        return Some(LlmLogHit::HermesResponse);
    }
    if l.contains("TTS Piper") || l.contains("TTS Formant") {
        return Some(LlmLogHit::TtsOut);
    }
    None
}

/// Acumulado de evidências ao longo de um log.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LlmEvidence {
    pub intent: bool,
    pub submit: bool,
    pub prefill_begin: bool,
    pub prefill_done: bool,
    pub decode: bool,
    pub job_done: bool,
    pub stream: bool,
    pub hermes_response: bool,
    pub tts: bool,
}

impl LlmEvidence {
    pub fn ingest_line(&mut self, line: &str) {
        match classify_line(line) {
            Some(LlmLogHit::IntentLinj) | Some(LlmLogHit::IntentTexto) => self.intent = true,
            Some(LlmLogHit::InferSubmit) => self.submit = true,
            Some(LlmLogHit::PrefillBegin) => self.prefill_begin = true,
            Some(LlmLogHit::PrefillDone) => self.prefill_done = true,
            Some(LlmLogHit::DecodeToks) => self.decode = true,
            Some(LlmLogHit::JobDone) => self.job_done = true,
            Some(LlmLogHit::StreamDelta) => self.stream = true,
            Some(LlmLogHit::HermesResponse) => self.hermes_response = true,
            Some(LlmLogHit::TtsOut) => self.tts = true,
            None => {}
        }
    }

    pub fn ingest_log(&mut self, text: &str) {
        for line in text.lines() {
            self.ingest_line(line);
        }
    }

    /// Resposta gerada de verdade (não só enqueue).
    pub fn has_response(&self) -> bool {
        self.job_done || self.decode || self.stream || self.hermes_response
    }

    /// Prefill chegou ao fim (necessário antes do decode no InferQ atual).
    pub fn has_prefill_complete(&self) -> bool {
        self.prefill_done
    }

    /// Verdict de lab: FAIL / PASS_WEAK / PASS_PARTIAL / PASS.
    pub fn verdict(&self) -> &'static str {
        if !self.intent && !self.submit {
            return "FAIL: sem intent nem InferQueue submit";
        }
        if self.submit && !self.prefill_begin && !self.has_response() {
            return "FAIL: submit sem prefill (fila parada)";
        }
        if self.prefill_begin && !self.prefill_done && !self.has_response() {
            return "FAIL: prefill incompleto — sem resposta";
        }
        if self.prefill_done && !self.has_response() {
            return "FAIL: prefill_done sem decode/done (resposta ausente)";
        }
        if self.has_response() && self.tts {
            return "PASS: resposta LLM + TTS";
        }
        if self.has_response() && self.prefill_done {
            return "PASS: resposta LLM (decode/done/stream)";
        }
        if self.has_response() {
            return "PASS_PARTIAL: resposta sem prefill_done observavel";
        }
        if self.submit {
            return "FAIL: submit sem resposta (PASS falso evitavel)";
        }
        "FAIL: sem evidencia LLM"
    }
}

/// Prompts canônicos de lab (ASCII) — espelham LINJ / sendkey.
pub mod prompts {
    pub const CLIMA_PASSO_FUNDO: &str =
        "jarbas, que temperatura esta agora em passo fundo?";
    pub const APP_CLIMA_TEMPO: &str = "crie uma app de clima e tempo";
    pub const PING_CURTO: &str = "diga ok";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_intent_and_submit() {
        assert_eq!(
            classify_line(
                r#"[T+228] [R3] [hermes] [LAB] [ok] - inject USER_INTENT (LINJ): "diga ok""#
            ),
            Some(LlmLogHit::IntentLinj)
        );
        assert_eq!(
            classify_line(
                r#"[T+233] [R3] [hermes] [CORTEX] [ok] - Texto: "diga ok""#
            ),
            Some(LlmLogHit::IntentTexto)
        );
        assert_eq!(
            classify_line(
                "[T+251] [R2] [cortex] [LLM] [ok] - InferQueue submit id=1 (Falcon3 off BSP)"
            ),
            Some(LlmLogHit::InferSubmit)
        );
    }

    #[test]
    fn classify_prefill_decode_done() {
        assert_eq!(
            classify_line(
                "[T+253] [R2] [cortex] [InferQ] [ok] - prefill_begin id=1 tokens=8 layers=18"
            ),
            Some(LlmLogHit::PrefillBegin)
        );
        assert_eq!(
            classify_line(
                "[T+3544] [R2] [cortex] [InferQ] [ok] - prefill_done id=1 tokens=8 layers=18 slices=10 us=55717302"
            ),
            Some(LlmLogHit::PrefillDone)
        );
        assert_eq!(
            classify_line(
                "[T+3600] [R2] [cortex] [InferQ] [ok] - decode_tok/s=2 toks=4 us=2000000"
            ),
            Some(LlmLogHit::DecodeToks)
        );
        assert_eq!(
            classify_line(
                "[T+3700] [R2] [cortex] [InferQ] [ok] - done id=1 len=12 in_flight=0"
            ),
            Some(LlmLogHit::JobDone)
        );
    }

    #[test]
    fn submit_alone_is_fail() {
        let mut e = LlmEvidence::default();
        e.ingest_line(
            "[T+251] [R2] [cortex] [LLM] [ok] - InferQueue submit id=1 (Falcon3 off BSP)",
        );
        assert!(!e.has_response());
        assert!(e.verdict().starts_with("FAIL: submit sem"));
    }

    #[test]
    fn prefill_done_without_decode_is_fail() {
        // Caso real lab mesh2 app-clima: prefill_done e fila engasgou.
        let mut e = LlmEvidence::default();
        e.ingest_log(
            r#"
[T+228] inject USER_INTENT (LINJ): "crie uma app de clima e tempo"
[T+251] InferQueue submit id=1 (Falcon3 off BSP)
[T+253] prefill_begin id=1 tokens=8 layers=18
[T+3544] prefill_done id=1 tokens=8 layers=18 slices=10 us=55717302
"#,
        );
        assert!(e.intent);
        assert!(e.submit);
        assert!(e.prefill_done);
        assert!(!e.has_response());
        assert_eq!(
            e.verdict(),
            "FAIL: prefill_done sem decode/done (resposta ausente)"
        );
    }

    #[test]
    fn full_pipeline_pass() {
        let mut e = LlmEvidence::default();
        e.ingest_log(
            r#"[T+1] inject USER_INTENT (LINJ): "diga ok"
[T+2] [cortex] [LLM] InferQueue submit id=1
[T+3] [cortex] [InferQ] [ok] - prefill_begin id=1
[T+4] [cortex] [InferQ] [ok] - prefill_done id=1 tokens=3 layers=18
[T+5] [cortex] [InferQ] [ok] - decode_tok/s=1 toks=2 us=2000000
[T+6] [cortex] [InferQ] [ok] - done id=1 len=5 in_flight=0
[T+7] MSG_DELTA|ok
"#,
        );
        assert!(e.intent);
        assert!(e.submit);
        assert!(e.prefill_done);
        assert!(e.decode);
        assert!(e.job_done);
        assert!(e.stream);
        assert!(e.has_response());
        assert!(e.verdict().starts_with("PASS"));
    }

    #[test]
    fn prompts_are_ascii_printable() {
        for p in [prompts::CLIMA_PASSO_FUNDO, prompts::APP_CLIMA_TEMPO, prompts::PING_CURTO] {
            assert!(p.is_ascii());
            assert!(p.len() <= 240);
            assert!(!p.is_empty());
        }
    }

    #[test]
    fn stream_delta_counts_as_response() {
        let mut e = LlmEvidence::default();
        e.submit = true;
        e.prefill_done = true;
        e.ingest_line("MSG_DELTA|ola mundo");
        assert!(e.has_response());
        assert!(e.verdict().contains("PASS"));
    }
}
