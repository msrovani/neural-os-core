//! Structured Decoding — FSM comprimido para geracao constraint.
//! Baseado em SGLang compressed FSM (arXiv 2405.16818).
//! Mascara logits no BitNet decoder para tokens validos (JSON, SKILL.md, shell).

use alloc::vec::Vec;
use alloc::vec;
use alloc::string::String;
use alloc::collections::BTreeMap;
use k_nano::kjson;

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
    SkillCmd,      // comandos Hermes (/status, /ping, etc)
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
        // Estado 0: inicio (espera {, [, ", true, false, null, number)
        // Estado 1: dentro de objeto (espera }, string)
        // Estado 2: apos chave (espera :)
        // Estado 3: apos valor em objeto (espera , ou })
        // Estado 4: dentro de array (espera ], valor)
        // Estado 5: dentro de string
        // Estado 6: apos numero
        // Estado 7: dentro de true
        // Estado 8: dentro de false
        // Estado 9: dentro de null

        // Transicoes simplificadas para o espaco de tokens CHAR_OFFSET+char
        let co = CHAR_OFFSET;

        // Estado 0: inicio
        self.add_trans(0, b'{' as u16 + co, 1);
        self.add_trans(0, b'[' as u16 + co, 4);
        self.add_trans(0, b'"' as u16 + co, 5);
        self.add_trans(0, b't' as u16 + co, 7);
        self.add_trans(0, b'f' as u16 + co, 8);
        self.add_trans(0, b'n' as u16 + co, 9);
        for d in 0..10 { self.add_trans(0, b'0' as u16 + d as u16 + co, 6); }
        self.add_trans(0, b'-' as u16 + co, 6);

        // Estado 1: dentro de objeto
        self.add_trans(1, b'"' as u16 + co, 5);
        self.add_trans(1, b'}' as u16 + co, 0);

        // Estado 2: apos chave
        self.add_trans(2, b':' as u16 + co, 0);

        // Estado 3: apos valor
        self.add_trans(3, b',' as u16 + co, 1);
        self.add_trans(3, b'}' as u16 + co, 0);

        // Estado 4: dentro de array
        self.add_trans(4, b']' as u16 + co, 0);
        // No array pode ter qualquer valor
        self.add_trans(4, b'"' as u16 + co, 5);
        for d in 0..10 { self.add_trans(4, b'0' as u16 + d as u16 + co, 6); }

        // Estado 5: dentro de string (qualquer char exceto ")
        for c in 32..127u16 {
            if c != b'"' as u16 {
                self.add_trans(5, c + co, 5);
            }
        }
        self.add_trans(5, b'"' as u16 + co, 0);

        // Estado 6: apos numero
        self.add_trans(6, b',' as u16 + co, 3);
        self.add_trans(6, b'}' as u16 + co, 0);
        self.add_trans(6, b']' as u16 + co, 0);
        for d in 0..10 { self.add_trans(6, b'0' as u16 + d as u16 + co, 6); }
        self.add_trans(6, b'.' as u16 + co, 6);

        // Estado 7: true
        self.add_trans(7, b'r' as u16 + co, 10);
        self.add_trans(10, b'u' as u16 + co, 11);
        self.add_trans(11, b'e' as u16 + co, 0);

        // Estado 8: false
        self.add_trans(8, b'a' as u16 + co, 12);
        self.add_trans(12, b'l' as u16 + co, 13);
        self.add_trans(13, b's' as u16 + co, 14);
        self.add_trans(14, b'e' as u16 + co, 0);

        // Estado 9: null
        self.add_trans(9, b'u' as u16 + co, 15);
        self.add_trans(15, b'l' as u16 + co, 16);
        self.add_trans(16, b'l' as u16 + co, 0);

        self.update_valid();
    }

    /// FSM numero: [0-9.-]
    fn build_number_fsm(&mut self) {
        let co = CHAR_OFFSET as u16;
        for d in 0..10 { self.add_trans(0, b'0' as u16 + d as u16 + co, 0); }
        self.add_trans(0, b'.' as u16 + co, 1);
        self.add_trans(0, b'-' as u16 + co, 0);
        self.add_trans(1, b'0' as u16 + co, 1);
        self.update_valid();
    }

    /// FSM alpha: apenas A-Za-z
    fn build_alpha_fsm(&mut self) {
        let co = CHAR_OFFSET as u16;
        for c in b'A'..=b'Z' { self.add_trans(0, c as u16 + co, 0); }
        for c in b'a'..=b'z' { self.add_trans(0, c as u16 + co, 0); }
        self.add_trans(0, b' ' as u16 + co, 0);
        self.update_valid();
    }

    /// FSM skill: /status, /help, /ping, etc
    fn build_skill_fsm(&mut self) {
        let co = CHAR_OFFSET as u16;
        self.add_trans(0, b'/' as u16 + co, 1);
        for c in b'a'..=b'z' { self.add_trans(1, c as u16 + co, 1); }
        self.add_trans(1, b' ' as u16 + co, 0);
        self.update_valid();
    }

    /// FSM shell safe: sem pipe, redirect, ;, `, $, (, ), {, }, <, >
    fn build_shell_fsm(&mut self) {
        let co = CHAR_OFFSET as u16;
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
            // Fallback: permite BOS/EOS/PAD em estado invalido
            self.valid_tokens = vec![BOS, EOS, PAD];
        }
    }

    /// Avanca o FSM com o token gerado, retorna tokens validos para o proximo passo
    pub fn step(&mut self, token: u16) -> &[u16] {
        if self.mode == DecodeMode::Free {
            return &[];
        }
        if let Some(&next) = self.transitions.get(&(self.state, token)) {
            self.state = next;
        }
        // Se token EOS, mantem estado atual
        if token == EOS {
            return &[];
        }
        self.update_valid();
        &self.valid_tokens
    }

    /// Mascara logits: zera tudo exceto tokens validos
    pub fn mask_logits(&self, logits: &mut [f32]) {
        if self.mode == DecodeMode::Free { return; }
        let vocab = logits.len();
        for i in 0..vocab {
            let tok = i as u16;
            if tok < CHAR_OFFSET {
                // BOS/EOS/PAD sempre permitidos
                continue;
            }
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

/// SkillOpt — otimiza skills baseado em metricas de execucao
pub struct SkillOptimizer {
    pub min_calls: u32,
    pub min_success: f32,
    pub optimized: Vec<String>,
}

impl SkillOptimizer {
    pub fn new() -> Self {
        SkillOptimizer { min_calls: 3, min_success: 0.7, optimized: Vec::new() }
    }

    /// Analisa metricas do SkillMarket e sugere otimizacoes
    pub fn analyze(&mut self, market: &hermes::wasm_rt::SkillMarket) -> Vec<String> {
        let mut suggestions = Vec::new();
        for s in market.top(10) {
            if s.calls >= self.min_calls && s.success_rate < self.min_success {
                let suggestion = alloc::format!(
                    "Skill '{}': success_rate={:.1}% ({} calls) — needs review",
                    s.skill, s.success_rate * 100.0, s.calls
                );
                suggestions.push(suggestion);
                if !self.optimized.contains(&s.skill) {
                    self.optimized.push(s.skill.clone());
                }
            }
        }
        kjson!("SKILLOPT", "ANALYZE", "done", "suggestions", suggestions.len() as u32);
        suggestions
    }

    /// Gera nova versao de skill com parametros ajustados
    pub fn optimize_skill(&self, _name: &str, old_ticks: u64, success_rate: f32) -> (u64, f32) {
        // Otimizacao heuristica: reduz fuel se taxa alta, aumenta se baixa
        let new_fuel = if success_rate > 0.9 {
            (old_ticks as f32 * 0.8) as u64
        } else if success_rate < 0.5 {
            (old_ticks as f32 * 1.5) as u64
        } else {
            old_ticks
        };
        let new_rate = success_rate.min(1.0);
        kjson!("SKILLOPT", "SKILL", "optimize", "fuel", new_fuel, "rate", new_rate);
        (new_fuel, new_rate)
    }

    pub fn status(&self) -> String {
        alloc::format!("[SKILLOPT] {} optimized, threshold={}% success/{} calls",
            self.optimized.len(), (self.min_success * 100.0) as u8, self.min_calls)
    }
}
