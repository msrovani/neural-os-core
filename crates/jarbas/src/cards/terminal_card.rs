//! ADR-0090 Tier 3 - Terminal Card (VT100 subset)
//!
//! Card que integra o vconsole existente no desktop do Jarbas.

use alloc::string::String;
use alloc::vec::Vec;
use crate::display::card::{UiDeclaration, Widget};

pub const TERMINAL_CARD_ID: u32 = 8200;

pub struct TerminalState {
    pub output_lines: Vec<TerminalLine>,
    pub input_buffer: String,
    pub input_cursor: usize,
    pub scroll_offset: usize,
    pub active: bool,
}

#[derive(Clone)]
pub struct TerminalLine {
    pub text: String,
    pub fg: (u8, u8, u8),
    pub bg: (u8, u8, u8),
}

impl TerminalState {
    pub fn new() -> Self {
        let mut lines = Vec::new();
        lines.push(TerminalLine { text: String::from("Neural OS Terminal v1.0"), fg: (0, 200, 255), bg: (10, 12, 18) });
        lines.push(TerminalLine { text: String::from("Digite comandos ou use /help"), fg: (150, 175, 200), bg: (10, 12, 18) });
        Self { output_lines: lines, input_buffer: String::new(), input_cursor: 0, scroll_offset: 0, active: true }
    }

    pub fn push_line(&mut self, text: &str, fg: (u8, u8, u8)) {
        self.output_lines.push(TerminalLine { text: String::from(text), fg, bg: (10, 12, 18) });
        let visible = 15;
        if self.output_lines.len() > visible {
            self.scroll_offset = self.output_lines.len() - visible;
        }
    }

    pub fn push_error(&mut self, text: &str) { self.push_line(text, (255, 80, 80)); }
    pub fn push_success(&mut self, text: &str) { self.push_line(text, (80, 220, 120)); }

    pub fn process_input(&mut self) -> String {
        let cmd = core::mem::take(&mut self.input_buffer);
        self.input_cursor = 0;
        if cmd.is_empty() { return String::new(); }
        self.push_line(&alloc::format!("> {}", cmd), (200, 220, 255));
        match cmd.as_str() {
            "/help" => {
                self.push_line("Comandos:", (0, 200, 255));
                self.push_line("  /help    - esta ajuda", (150, 175, 200));
                self.push_line("  /clear   - limpa terminal", (150, 175, 200));
                self.push_line("  /ls      - lista arquivos", (150, 175, 200));
                self.push_line("  /ps      - lista agents", (150, 175, 200));
                self.push_line("  /mem     - uso de memoria", (150, 175, 200));
                self.push_line("  /net     - status rede", (150, 175, 200));
                self.push_line("  /mesh    - status mesh P2P", (150, 175, 200));
            }
            "/clear" => { self.output_lines.clear(); self.scroll_offset = 0; }
            "/ls" => {
                self.push_line("/", (0, 200, 255));
                self.push_line("  models/", (200, 200, 200));
                self.push_line("  firmware/", (200, 200, 200));
                self.push_line("  config/", (200, 200, 200));
                self.push_line("  BOOT.LOG", (200, 200, 200));
                self.push_line("  UPDATE.CFG", (200, 200, 200));
            }
            "/ps" => {
                self.push_line("Agents ativos:", (0, 200, 255));
                self.push_line("  display     running", (80, 220, 120));
                self.push_line("  hermes      running", (80, 220, 120));
                self.push_line("  cortex      idle", (200, 200, 100));
                self.push_line("  net         running", (80, 220, 120));
                self.push_line("  security    running", (80, 220, 120));
            }
            "/mem" => {
                let mem = k_nano::memory::TOTAL_RAM_MB.load(core::sync::atomic::Ordering::Relaxed);
                self.push_line(&alloc::format!("RAM: {}MB total", mem), (0, 200, 255));
            }
            "/net" => {
                let online = k_nano::env::is_online();
                self.push_line(&alloc::format!("Status: {}", if online { "ONLINE" } else { "OFFLINE" }),
                    if online { (80, 220, 120) } else { (200, 100, 100) });
            }
            "/mesh" => {
                self.push_line("Mesh P2P:", (0, 200, 255));
                self.push_line("  Nodes: 1 (self)", (200, 200, 200));
            }
            _ => { self.push_error(&alloc::format!("Comando desconhecido: {}", cmd)); }
        }
        cmd
    }

    pub fn input_char(&mut self, c: char) { self.input_buffer.push(c); self.input_cursor = self.input_buffer.len(); }
    pub fn input_backspace(&mut self) { self.input_buffer.pop(); self.input_cursor = self.input_buffer.len(); }
    pub fn scroll(&mut self, delta: i32) {
        let max = self.output_lines.len().saturating_sub(15);
        self.scroll_offset = (self.scroll_offset as i32 + delta).max(0).min(max as i32) as usize;
    }
}

pub fn terminal_card(state: &TerminalState) -> UiDeclaration {
    let mut decl = UiDeclaration::new(TERMINAL_CARD_ID, "Terminal", 120, 80, 520, 340);
    let visible_count = 14;
    let start = state.scroll_offset.min(state.output_lines.len());
    let end = (start + visible_count).min(state.output_lines.len());
    let items: Vec<String> = state.output_lines[start..end].iter().map(|l| l.text.clone()).collect();
    if !items.is_empty() { decl = decl.push(Widget::List(items)); }
    decl = decl.push(Widget::Divider);
    decl = decl.push(Widget::KeyValue(String::from(">"), state.input_buffer.clone()));
    decl = decl.push(Widget::Button(String::from("Clear")));
    decl = decl.push(Widget::Button(String::from("Scroll Up")));
    decl = decl.push(Widget::Button(String::from("Scroll Down")));
    decl
}

pub fn handle_terminal_button(card_id: u32, btn_idx: usize, state: &mut TerminalState) -> &'static str {
    if card_id != TERMINAL_CARD_ID { return "wrong_card"; }
    match btn_idx {
        0 => { state.output_lines.clear(); state.scroll_offset = 0; "clear" }
        1 => { state.scroll(-5); "scroll_up" }
        2 => { state.scroll(5); "scroll_down" }
        _ => "unknown"
    }
}

pub fn self_test() -> bool {
    let mut state = TerminalState::new();
    state.push_line("test output", (200, 200, 200));
    let decl = terminal_card(&state);
    decl.body.len() >= 2 && decl.title == "Terminal"
}
