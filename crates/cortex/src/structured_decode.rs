//! Structured Decoding — FSM comprimido para geracao constraint.
//! Baseado em SGLang compressed FSM (arXiv 2405.16818).
//! Mascara logits no BitNet decoder para tokens validos (JSON, SKILL.md, shell).

//! Structured Decoding — FSM comprimido para geracao constraint.
//! Baseado em SGLang compressed FSM (arXiv 2405.16818).
//! Mascara logits no BitNet decoder para tokens validos (JSON, numbers, alpha, shell).

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
