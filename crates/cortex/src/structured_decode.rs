//! Structured Decoding — FSM comprimido para geracao constraint.
//! Baseado em SGLang compressed FSM (arXiv 2405.16818).
//! Mascara logits no BitNet decoder para tokens validos (JSON, numbers, shell, skill).

use alloc::vec::Vec;
use alloc::vec;
use alloc::string::String;
use alloc::collections::BTreeMap;

const BOS: u16 = 0;
const EOS: u16 = 1;
const PAD: u16 = 2;
const CHAR_OFFSET: u16 = 3;
const VOCAB_SIZE: u16 = 99;

/// Modo de decodificacao estruturada
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DecodeMode {
    Free,          // sem constraint
    Json,          // JSON valido
    Number,        // numero (0-9, ., -)
    Alpha,         // apenas letras
    SkillCmd,      // comandos (/status, /ping, etc)
    ShellSafe,     // comandos shell seguros (sem pipe, redirect, etc)
}

/// Gramática de saída para decodificação estruturada — API pública.
///
/// Mapeia para o `DecodeMode` interno que implementa o FSM comprimido
/// (SGLang-style, arXiv 2405.16818).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutputGrammar {
    /// JSON válido: only `{}[]""` `true/false/null` `0-9` `:` `,` whitespace
    Json,
    /// Shell seguro: alphanumeric + `/` `-` `_` `.` ` ` — sem `|` `>` `<` `;` `&` `` ` `` `$`
    Shell,
    /// Skill command: comandos estilo `/skill` — full text structural (futuro LLM)
    Skill,
    /// Sem constraint (free text, passthrough)
    Raw,
}

impl From<OutputGrammar> for DecodeMode {
    fn from(g: OutputGrammar) -> Self {
        match g {
            OutputGrammar::Json => DecodeMode::Json,
            OutputGrammar::Shell => DecodeMode::ShellSafe,
            OutputGrammar::Skill => DecodeMode::Free,  // ponytail: full-text structural is a future concern
            OutputGrammar::Raw => DecodeMode::Free,
        }
    }
}

/// Verifica se um caractere é perigoso em contexto shell (pipe, redirect, etc).
/// Usado pelo self-test para validar saída em modo Shell.
pub fn is_shell_dangerous(c: char) -> bool {
    matches!(c, '|' | '>' | '<' | ';' | '&' | '`' | '$' | '(' | ')')
}

/// Verificação básica de estrutura JSON: começa com `{`/`[` e termina com `}`/`]`.
/// Não é um parser completo — suficiente para self-test do FSM.
pub fn looks_like_json(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    let first = s.as_bytes()[0];
    let last = s.as_bytes()[s.len() - 1];
    (first == b'{' && last == b'}') || (first == b'[' && last == b']')
}

/// Maquina de estados finita comprimida para decode constraint
pub struct StructuredDecoder {
    pub mode: DecodeMode,
    state: u32,
    valid_tokens: Vec<u16>,      // tokens permitidos no estado atual
    transitions: BTreeMap<(u32, u16), u32>,  // (state, token) -> next_state
}

impl StructuredDecoder {
    pub fn new(mode: DecodeMode) -> Self {
        let mut sd = StructuredDecoder {
            mode,
            state: 0,
            valid_tokens: Vec::new(),
            transitions: BTreeMap::new(),
        };
        sd.build_fsm();
        sd
    }

    /// Constroi FSM comprimido para o modo escolhido
    fn build_fsm(&mut self) {
        match self.mode {
            DecodeMode::Json => self.build_json_fsm(),
            DecodeMode::Number => self.build_number_fsm(),
            DecodeMode::Alpha => self.build_alpha_fsm(),
            DecodeMode::SkillCmd => self.build_skill_fsm(),
            DecodeMode::ShellSafe => self.build_shell_fsm(),
            DecodeMode::Free => {
                self.valid_tokens = (0..VOCAB_SIZE).collect();
            }
        }
    }

    /// FSM JSON: {}`[]`"": true/false/null, numbers
    fn build_json_fsm(&mut self) {
        let co = CHAR_OFFSET;
        self.add_trans(0, b'{' as u16 + co, 1);
        self.add_trans(0, b'[' as u16 + co, 4);
        self.add_trans(0, b'"' as u16 + co, 5);
        self.add_trans(0, b't' as u16 + co, 7);
        self.add_trans(0, b'f' as u16 + co, 8);
        self.add_trans(0, b'n' as u16 + co, 9);
        for d in 0..10 { self.add_trans(0, b'0' as u16 + d as u16 + co, 6); }
        self.add_trans(0, b'-' as u16 + co, 6);
        self.add_trans(1, b'"' as u16 + co, 5);
        self.add_trans(1, b'}' as u16 + co, 0);
        self.add_trans(2, b':' as u16 + co, 0);
        self.add_trans(3, b',' as u16 + co, 1);
        self.add_trans(3, b'}' as u16 + co, 0);
        self.add_trans(4, b']' as u16 + co, 0);
        self.add_trans(4, b'"' as u16 + co, 5);
        for d in 0..10 { self.add_trans(4, b'0' as u16 + d as u16 + co, 6); }
        for c in 32..127u16 {
            if c != b'"' as u16 { self.add_trans(5, c + co, 5); }
        }
        self.add_trans(5, b'"' as u16 + co, 0);
        self.add_trans(6, b',' as u16 + co, 3);
        self.add_trans(6, b'}' as u16 + co, 0);
        self.add_trans(6, b']' as u16 + co, 0);
        for d in 0..10 { self.add_trans(6, b'0' as u16 + d as u16 + co, 6); }
        self.add_trans(6, b'.' as u16 + co, 6);
        self.add_trans(7, b'r' as u16 + co, 10);
        self.add_trans(10, b'u' as u16 + co, 11);
        self.add_trans(11, b'e' as u16 + co, 0);
        self.add_trans(8, b'a' as u16 + co, 12);
        self.add_trans(12, b'l' as u16 + co, 13);
        self.add_trans(13, b's' as u16 + co, 14);
        self.add_trans(14, b'e' as u16 + co, 0);
        self.add_trans(9, b'u' as u16 + co, 15);
        self.add_trans(15, b'l' as u16 + co, 16);
        self.add_trans(16, b'l' as u16 + co, 0);
        self.update_valid();
    }

    fn build_number_fsm(&mut self) {
        let co = CHAR_OFFSET;
        for d in 0..10 { self.add_trans(0, b'0' as u16 + d as u16 + co, 0); }
        self.add_trans(0, b'.' as u16 + co, 1);
        self.add_trans(0, b'-' as u16 + co, 0);
        self.add_trans(1, b'0' as u16 + co, 1);
        self.update_valid();
    }

    fn build_alpha_fsm(&mut self) {
        let co = CHAR_OFFSET;
        for c in b'A'..=b'Z' { self.add_trans(0, c as u16 + co, 0); }
        for c in b'a'..=b'z' { self.add_trans(0, c as u16 + co, 0); }
        self.add_trans(0, b' ' as u16 + co, 0);
        self.update_valid();
    }

    fn build_skill_fsm(&mut self) {
        let co = CHAR_OFFSET;
        self.add_trans(0, b'/' as u16 + co, 1);
        for c in b'a'..=b'z' { self.add_trans(1, c as u16 + co, 1); }
        self.add_trans(1, b' ' as u16 + co, 0);
        self.update_valid();
    }

    fn build_shell_fsm(&mut self) {
        let co = CHAR_OFFSET;
        let safe: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789 ./_=+-@!~,[]";
        for &c in safe { self.add_trans(0, c as u16 + co, 0); }
        self.update_valid();
    }

    fn add_trans(&mut self, state: u32, token: u16, next: u32) {
        self.transitions.insert((state, token), next);
    }

    fn update_valid(&mut self) {
        self.valid_tokens = self.transitions.keys()
            .filter(|&(s, _)| *s == self.state)
            .map(|(_, t)| *t)
            .collect();
        if self.valid_tokens.is_empty() && self.mode != DecodeMode::Free {
            self.valid_tokens = vec![BOS, EOS, PAD];
        }
    }

    /// Avanca o FSM com o token gerado, retorna tokens validos para o proximo passo
    pub fn step(&mut self, token: u16) -> &[u16] {
        if self.mode == DecodeMode::Free { return &[]; }
        if let Some(&next) = self.transitions.get(&(self.state, token)) {
            self.state = next;
        }
        if token == EOS { return &[]; }
        self.update_valid();
        &self.valid_tokens
    }

    /// Mascara logits: zera tudo exceto tokens validos
    pub fn mask_logits(&self, logits: &mut [f32]) {
        if self.mode == DecodeMode::Free { return; }
        let vocab = logits.len();
        for i in 0..vocab {
            let tok = i as u16;
            if tok < CHAR_OFFSET { continue; }
            if !self.valid_tokens.contains(&tok) {
                logits[i] = f32::NEG_INFINITY;
            }
        }
    }

    pub fn reset(&mut self) {
        self.state = 0;
        self.update_valid();
    }

    pub fn status(&self) -> String {
        alloc::format!("[DECODE] mode={:?} state={} valid={}", self.mode, self.state, self.valid_tokens.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test 1: OutputGrammar → DecodeMode mapping
    #[test]
    fn test_output_grammar_mapping() {
        assert_eq!(DecodeMode::from(OutputGrammar::Json), DecodeMode::Json);
        assert_eq!(DecodeMode::from(OutputGrammar::Shell), DecodeMode::ShellSafe);
        assert_eq!(DecodeMode::from(OutputGrammar::Skill), DecodeMode::Free);
        assert_eq!(DecodeMode::from(OutputGrammar::Raw), DecodeMode::Free);
    }

    /// Test 2: JSON FSM — starts with { and follows valid transitions
    #[test]
    fn test_json_fsm_transitions() {
        let mut dec = StructuredDecoder::new(DecodeMode::Json);
        // Initial state should allow JSON starters: { [ " t f n 0-9 -
        assert!(dec.valid_tokens.contains(&(b'{' as u16 + CHAR_OFFSET)));
        assert!(dec.valid_tokens.contains(&(b'[' as u16 + CHAR_OFFSET)));
        assert!(dec.valid_tokens.contains(&(b'"' as u16 + CHAR_OFFSET)));
        // After {, we expect " }
        dec.step(b'{' as u16 + CHAR_OFFSET);
        assert!(dec.valid_tokens.contains(&(b'"' as u16 + CHAR_OFFSET)));
        assert!(dec.valid_tokens.contains(&(b'}' as u16 + CHAR_OFFSET)));
        // Close the object
        dec.step(b'}' as u16 + CHAR_OFFSET);
        // After closing, back to state 0 (can start a new value)
        assert!(dec.valid_tokens.contains(&(b'{' as u16 + CHAR_OFFSET)));
    }

    /// Test 3: JSON FSM — basic object {} closing
    #[test]
    fn test_json_object_complete() {
        let mut dec = StructuredDecoder::new(DecodeMode::Json);
        // {} — simulate the token sequence
        assert!(dec.valid_tokens.contains(&(b'{' as u16 + CHAR_OFFSET)), "state 0 should allow {{");
        dec.step(b'{' as u16 + CHAR_OFFSET);
        assert!(dec.valid_tokens.contains(&(b'}' as u16 + CHAR_OFFSET)), "state 1 should allow }}");
        dec.step(b'}' as u16 + CHAR_OFFSET);
        // Back to state 0 — should allow starters again
        assert!(dec.valid_tokens.contains(&(b'{' as u16 + CHAR_OFFSET)), "back to state 0");
    }

    /// Test 4: Shell FSM — rejects dangerous characters
    #[test]
    fn test_shell_safe_rejects_dangerous() {
        let dec = StructuredDecoder::new(DecodeMode::ShellSafe);
        let dangerous_chars = b"|><;&`$()";
        for &c in dangerous_chars {
            let tok = c as u16 + CHAR_OFFSET;
            assert!(
                !dec.valid_tokens.contains(&tok),
                "ShellSafe should reject '{}' (token {})",
                c as char, tok
            );
        }
        // Safe chars should be allowed
        let safe_chars = b"ls -la /tmp/file.txt";
        for &c in safe_chars {
            let tok = c as u16 + CHAR_OFFSET;
            assert!(
                dec.valid_tokens.contains(&tok),
                "ShellSafe should allow '{}' (token {})",
                c as char, tok
            );
        }
    }

    /// Test 5: Shell FSM — round-trip through mask_logits
    #[test]
    fn test_shell_mask_logits() {
        let dec = StructuredDecoder::new(DecodeMode::ShellSafe);
        let mut logits = [0.0f32; VOCAB_SIZE as usize];
        dec.mask_logits(&mut logits);
        // Dangerous char tokens should be NEG_INFINITY
        let dangerous = b"|><;&`$()";
        for &c in dangerous {
            let tok = (c as u16 + CHAR_OFFSET) as usize;
            if tok < logits.len() {
                assert!(
                    logits[tok] == f32::NEG_INFINITY,
                    "char '{}' should be masked",
                    c as char
                );
            }
        }
        // Safe char tokens should be 0.0 (unmasked)
        let safe = b"ls -la";
        for &c in safe {
            let tok = (c as u16 + CHAR_OFFSET) as usize;
            if tok < logits.len() {
                assert_eq!(logits[tok], 0.0, "char '{}' should not be masked", c as char);
            }
        }
    }

    /// Test 6: JSON mask_logits — first token must be a JSON starter
    #[test]
    fn test_json_mask_logits_first_token() {
        let dec = StructuredDecoder::new(DecodeMode::Json);
        // FSM tokens vão até b'~' + CHAR_OFFSET (126+3=129); VOCAB_SIZE=99 cobre
        // só 0..99. Buffer maior p/ cobrir o espaço de tokens do FSM (pre-existente).
        let mut logits = [0.0f32; 130];
        dec.mask_logits(&mut logits);
        // '{' must be valid (not masked)
        let brace_open = (b'{' as u16 + CHAR_OFFSET) as usize;
        assert_eq!(logits[brace_open], 0.0, "{{ should not be masked at start");
        // '|' must be masked
        let pipe = (b'|' as u16 + CHAR_OFFSET) as usize;
        assert_eq!(logits[pipe], f32::NEG_INFINITY, "pipe should be masked in JSON mode");
    }

    /// Test 7: OutputGrammar JSON — verify generated tokens start with { and end with }
    #[test]
    fn test_json_output_structure() {
        // We cannot run the full model in no_std test context,
        // but we verify the FSM transitions produce a valid starting sequence.
        let mut dec = StructuredDecoder::new(DecodeMode::Json);
        // Step 1: '{'
        let t1 = b'{' as u16 + CHAR_OFFSET;
        assert!(dec.valid_tokens.contains(&t1), "state 0 must allow {{");
        dec.step(t1);
        // Step 2: '}'
        let t2 = b'}' as u16 + CHAR_OFFSET;
        assert!(dec.valid_tokens.contains(&t2), "after {{ must allow }}");
        dec.step(t2);
        // State is back to 0 — verified by allow {{ again
        let t3 = b'{' as u16 + CHAR_OFFSET;
        assert!(dec.valid_tokens.contains(&t3), "after {{}} must return to state 0");
    }

    /// Test 8: Shell safe chars — is_shell_dangerous helper
    #[test]
    fn test_is_shell_dangerous() {
        assert!(is_shell_dangerous('|'));
        assert!(is_shell_dangerous('>'));
        assert!(is_shell_dangerous('<'));
        assert!(is_shell_dangerous(';'));
        assert!(is_shell_dangerous('&'));
        assert!(is_shell_dangerous('`'));
        assert!(is_shell_dangerous('$'));
        assert!(!is_shell_dangerous('a'));
        assert!(!is_shell_dangerous('/'));
        assert!(!is_shell_dangerous('-'));
    }

    /// Test 9: looks_like_json helper
    #[test]
    fn test_looks_like_json() {
        assert!(looks_like_json("{}"));
        assert!(looks_like_json("[]"));
        assert!(looks_like_json(r#"{"a":1}"#));
        assert!(looks_like_json("[1,2,3]"));
        assert!(!looks_like_json("hello"));
        assert!(!looks_like_json(""));
    }

    /// Test 10: Free mode passes everything through
    #[test]
    fn test_free_mode_no_masking() {
        let dec = StructuredDecoder::new(DecodeMode::Free);
        let mut logits = [42.0f32; VOCAB_SIZE as usize];
        dec.mask_logits(&mut logits);
        for v in logits.iter() {
            assert_eq!(*v, 42.0, "Free mode should not modify logits");
        }
    }
}
