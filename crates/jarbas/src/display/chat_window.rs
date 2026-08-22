//! ChatWindow — janela de chat Onyx-style renderizada no framebuffer.
//!
//! Substitui o overlay Hermes texto-plano por uma UI com:
//! - Timeline de tools (expansível/colapsável)
//! - Mensagens com streaming typewriter
//! - Input buffer no rodapé
//! - Histórico por sessão (árvore)
//!
//! Renderização direta pixel-a-pixel (sem embedded-graphics), mesma
//! abordagem do console.rs, para compatibilidade máxima.

use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;
use event_bus;
use hermes::stream_packet::{self, StreamPacket, ToolKind};
use crate::display::fb::DoubleBuffer;
use crate::display::font;
use crate::display::compositor::draw_text;

/// Flag: botão de microfone ativo (ChatWindow pediu gravação).
/// Lida pelo JarbasVoiceAgent em voice.rs.
pub static MIC_ACTIVE: AtomicBool = AtomicBool::new(false);

// ── Constantes visuais ──────────────────────────────────────────────

/// Altura da barra de título
const TITLE_H: usize = 28;
/// Altura do input bar
const INPUT_H: usize = 28;
/// Padding horizontal
const PAD: usize = 8;
/// Altura de linha
const LH: usize = 16;
/// Ícones de tool (caractere)
// Ícones ASCII — a fonte bitmap (8x16) só suporta 0x20..0x7E
const ICON_REASON: char = '*';
const ICON_SEARCH: char = '?';
const ICON_FETCH:  char = '>';
const ICON_CODE:   char = '$';
const ICON_MEMORY: char = '&';
const ICON_DONE:   char = '+';
const ICON_USER:   char = '<';
const ICON_BOT:    char = '@';

// ── Tipos de dados ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TimelineStep {
    pub id: u32,
    pub kind: ToolKind,
    pub label: String,
    pub content: String,
    pub done: bool,
    pub expanded: bool,
}

#[derive(Debug, Clone)]
pub enum DisplayMsg {
    User { content: String },
    Assistant {
        content: String,
        reasoning: Option<String>,
        citations: Vec<(u32, String, Option<String>)>,
        /// Tool calls que precederam esta mensagem
        tool_steps: Vec<usize>,  // índices no timeline_steps global
    },
    System { content: String },
    Error { content: String },
}

// ── Estado global do ChatWindow ──────────────────────────────────────

pub static CHAT_WINDOW: Mutex<Option<ChatWindow>> = Mutex::new(None);

pub struct ChatWindow {
    /// ID da sessão atual
    pub session_id: u32,
    /// Mensagens já completadas (display)
    pub messages: VecDeque<DisplayMsg>,
    /// Steps da timeline ativa (tool calls em andamento ou completos)
    pub timeline_steps: Vec<TimelineStep>,
    /// Conteúdo streaming atual (MessageDelta sendo recebido)
    pub streaming_msg: String,
    /// Reasoning streaming atual
    pub reasoning_msg: String,
    /// Se está mostrando reasoning no momento
    pub showing_reasoning: bool,
    /// Se há streaming ativo
    pub streaming_active: bool,
    /// Buffer de input do usuário
    pub input_buffer: String,
    /// Cursor no input
    pub input_cursor: usize,
    /// Scroll offset (linhas)
    pub scroll: usize,
    /// Pré-processamento (tool execution time)
    pub pre_answer_seconds: Option<f32>,
    /// Flag p/ forçar re-render
    pub dirty: bool,
    /// Último tool id emitido
    next_tool_id: u32,
}

impl ChatWindow {
    pub fn new(session_id: u32) -> Self {
        Self {
            session_id,
            messages: VecDeque::new(),
            timeline_steps: Vec::new(),
            streaming_msg: String::new(),
            reasoning_msg: String::new(),
            showing_reasoning: false,
            streaming_active: false,
            input_buffer: String::new(),
            input_cursor: 0,
            scroll: 0,
            pre_answer_seconds: None,
            dirty: true,
            next_tool_id: 1,
        }
    }


    /// FASE 4.2: Scroll via mouse wheel. delta > 0 = scroll up.
    pub fn handle_scroll(&mut self, delta: i32) {
        let total_lines: usize = self.messages.iter().map(|m| {
            match m {
                DisplayMsg::User { content } => self.wrap_text(content, 400).len() + 1,
                DisplayMsg::Assistant { content, .. } => self.wrap_text(content, 400).len() + 1,
                DisplayMsg::Error { content } => self.wrap_text(content, 400).len() + 1,
                DisplayMsg::System { .. } => 1,
            }
        }).sum();
        let max_scroll = total_lines.saturating_sub(15);
        self.scroll = (self.scroll as i32 + delta).max(0).min(max_scroll as i32) as usize;
        self.dirty = true;
    }

    /// Processa um StreamPacket e atualiza estado interno
    pub fn process_packet(&mut self, pkt: StreamPacket) {
        self.dirty = true;
        match pkt {
            StreamPacket::SessionStart { session_id } => {
                // Reseta tudo
                self.session_id = session_id;
                self.streaming_msg.clear();
                self.reasoning_msg.clear();
                self.showing_reasoning = false;
                self.streaming_active = false;
                self.timeline_steps.clear();
                self.messages.clear();
                self.pre_answer_seconds = None;
            }
            StreamPacket::ReasoningStart => {
                self.showing_reasoning = true;
                self.reasoning_msg.clear();
                // Adiciona step de reasoning na timeline
                let id = self.next_tool_id;
                self.next_tool_id += 1;
                self.timeline_steps.push(TimelineStep {
                    id,
                    kind: ToolKind::Reasoning,
                    label: String::from("Raciocinando"),
                    content: String::new(),
                    done: false,
                    expanded: false,
                });
            }
            StreamPacket::ReasoningDelta { content } => {
                self.reasoning_msg.push_str(&content);
                // Atualiza último step de reasoning
                if let Some(step) = self.timeline_steps.iter_mut().rev().find(|s| s.kind == ToolKind::Reasoning && !s.done) {
                    step.content.push_str(&content);
                }
            }
            StreamPacket::ReasoningDone => {
                self.showing_reasoning = false;
                if let Some(step) = self.timeline_steps.iter_mut().rev().find(|s| s.kind == ToolKind::Reasoning && !s.done) {
                    step.done = true;
                }
            }
            StreamPacket::ToolStart { id, kind, label } => {
                self.next_tool_id = self.next_tool_id.max(id + 1);
                self.timeline_steps.push(TimelineStep {
                    id,
                    kind,
                    label,
                    content: String::new(),
                    done: false,
                    expanded: true,  // auto-expand tool steps
                });
            }
            StreamPacket::ToolDelta { id, content } => {
                if let Some(step) = self.timeline_steps.iter_mut().find(|s| s.id == id) {
                    step.content.push_str(&content);
                }
            }
            StreamPacket::ToolDone { id, result_summary } => {
                if let Some(step) = self.timeline_steps.iter_mut().find(|s| s.id == id) {
                    step.done = true;
                    if let Some(summary) = result_summary {
                        step.content = summary;
                    }
                    // Auto-collapse após concluído
                    step.expanded = false;
                }
            }
            StreamPacket::MessageStart { pre_answer_seconds } => {
                self.pre_answer_seconds = pre_answer_seconds;
                self.streaming_msg.clear();
                self.streaming_active = true;
            }
            StreamPacket::MessageDelta { content } => {
                self.streaming_msg.push_str(&content);
            }
            StreamPacket::Citation { doc_id: _, text: _, url: _ } => {
                // Será anexado à mensagem quando finalizar
            }
            StreamPacket::UserMessage { content } => {
                // Mensagem do usuário — vira entrada no histórico
                self.messages.push_back(DisplayMsg::User { content: content.clone() });
                self.streaming_active = true;
            }
            StreamPacket::Error { message } => {
                self.messages.push_back(DisplayMsg::Error { content: message });
                self.streaming_active = false;
            }
            StreamPacket::Stop => {
                // Finaliza a mensagem atual e registra no histórico
                self.streaming_active = false;
                if !self.streaming_msg.is_empty() {
                    // Coleta os índices dos tool steps ativos
                    let step_indices: Vec<usize> = (0..self.timeline_steps.len()).collect();
                    self.messages.push_back(DisplayMsg::Assistant {
                        content: core::mem::take(&mut self.streaming_msg),
                        reasoning: if self.reasoning_msg.is_empty() { None } else { Some(core::mem::take(&mut self.reasoning_msg)) },
                        citations: Vec::new(),
                        tool_steps: step_indices,
                    });
                }
            }
        }
    }

    /// Renderiza o chat window dentro do retângulo dado.
    /// x,y = posição, w,h = dimensões, fb = framebuffer, scr_w = largura total (p/ draw_text)
    pub fn render(&self, fb: &mut DoubleBuffer, x: usize, y: usize, w: usize, h: usize, scr_w: usize) {
        let theme = crate::display::theme::current_theme();
        let fg = theme.fg;
        let bg = theme.bg_alt;
        let accent = theme.accent;
        let muted = theme.fg_muted;

        // ── Fundo da janela ──
        fb.fill_rect(x, y, w, h, bg.0, bg.1, bg.2);

        // ── Área de conteúdo (entre título e input) ──
        let content_top = y + TITLE_H + 4;
        let content_bot = y + h - INPUT_H;
        let _content_h = content_bot.saturating_sub(content_top);
        let content_x = x + PAD;

        let mut line_y = content_bot.saturating_sub(LH);  // começa de baixo p/ cima

        // ── Input bar ──
        let input_y = y + h - INPUT_H;
        fb.fill_rect(x, input_y, w, INPUT_H, theme.bg.0, theme.bg.0, theme.bg.0);
        fb.fill_rect(x, input_y, w, 1, accent.0, accent.1, accent.2); // linha separadora

        // Botão microfone (toggle gravação)
        let mic_active = MIC_ACTIVE.load(Ordering::Relaxed);
        let mic_x = content_x;
        let mic_label = if mic_active { "[REC]" } else { "[MIC]" };
        let mic_color = if mic_active { (255u8, 60u8, 60u8) } else { muted };
        draw_text(fb, mic_x, input_y + 6, mic_label, scr_w, mic_color.0, mic_color.1, mic_color.2);
        // Indicador de gravação pulsante (alterna a cada ~30 ticks baseado no tick global)
        if mic_active {
            let tick = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            if (tick / 15) % 2 == 0 {
                // Bolinha vermelha pulsante ao lado do [REC]
                fb.fill_rect(mic_x + 5 * font::CHAR_W, input_y + 10, 6, 6, 255, 40, 40);
            }
        }
        let prompt_x = mic_x + 5 * font::CHAR_W + 10; // espaço p/ mic label + bolinha
        let prompt = alloc::format!("> {}", self.input_buffer);
        draw_text(fb, prompt_x, input_y + 6, &prompt, scr_w, fg.0, fg.1, fg.2);

        // ── Streaming ativo: mostra mensagem streaming ──
        if self.streaming_active && !self.streaming_msg.is_empty() {
            let lines = self.wrap_text(&self.streaming_msg, w.saturating_sub(PAD * 2));
            for text in lines.iter().rev() {
                if line_y < content_top { break; }
                draw_text(fb, content_x, line_y, text, scr_w, fg.0, fg.1, fg.2);
                line_y = line_y.saturating_sub(LH);
            }
        }

        // ── Timeline steps (tool calls) ──
        for step in self.timeline_steps.iter().rev() {
            let icon = match step.kind {
                ToolKind::Reasoning => ICON_REASON,
                ToolKind::Search => ICON_SEARCH,
                ToolKind::UrlFetch => ICON_FETCH,
                ToolKind::CodeExec => ICON_CODE,
                ToolKind::Memory => ICON_MEMORY,
                _ => ICON_DONE,
            };
            let status_ch = if step.done { ICON_DONE } else { '~' };

            // Cabeçalho do step
            let header = alloc::format!("[{}] {} {} — {}", status_ch, icon, step.label, if step.done { "ok" } else { "..." });
            if line_y < content_top { break; }
            let color = if step.done { muted } else { accent };
            draw_text(fb, content_x, line_y, &header, scr_w, color.0, color.1, color.2);
            line_y = line_y.saturating_sub(LH);

            // Conteúdo expandido
            if step.expanded && !step.content.is_empty() {
                let content_lines = self.wrap_text(&step.content, w.saturating_sub(PAD * 2 + 16));
                for text in content_lines.iter().rev() {
                    if line_y < content_top { break; }
                    draw_text(fb, content_x + 16, line_y, text, scr_w, fg.0, fg.1, fg.2);
                    line_y = line_y.saturating_sub(LH);
                }
            }
        }

        // ── Mensagens do histórico ──
        for msg in self.messages.iter().rev() {
            match msg {
                DisplayMsg::User { content } => {
                    let lines = self.wrap_text(content, w.saturating_sub(PAD * 2));
                    for text in lines.iter().rev() {
                        if line_y < content_top { break; }
                        draw_text(fb, content_x, line_y, text, scr_w, 0, 200, 255); // cyan p/ user
                        line_y = line_y.saturating_sub(LH);
                    }
                    if line_y >= content_top {
                        draw_text(fb, content_x, line_y, "─── você ───", scr_w, 0, 200, 255);
                        line_y = line_y.saturating_sub(LH);
                    }
                }
                DisplayMsg::Assistant { content, .. } => {
                    let lines = self.wrap_text(content, w.saturating_sub(PAD * 2));
                    for text in lines.iter().rev() {
                        if line_y < content_top { break; }
                        draw_text(fb, content_x, line_y, text, scr_w, fg.0, fg.1, fg.2);
                        line_y = line_y.saturating_sub(LH);
                    }
                    if line_y >= content_top {
                        draw_text(fb, content_x, line_y, "─── jarbas ───", scr_w, 0, 255, 100);
                        line_y = line_y.saturating_sub(LH);
                    }
                }
                DisplayMsg::Error { content } => {
                    let lines = self.wrap_text(content, w.saturating_sub(PAD * 2));
                    for text in lines.iter().rev() {
                        if line_y < content_top { break; }
                        draw_text(fb, content_x, line_y, text, scr_w, 255, 80, 80);
                        line_y = line_y.saturating_sub(LH);
                    }
                }
                DisplayMsg::System { content } => {
                    if line_y >= content_top {
                        draw_text(fb, content_x, line_y, content, scr_w, muted.0, muted.1, muted.2);
                        line_y = line_y.saturating_sub(LH);
                    }
                }
            }
        }

        // ── Placeholder se vazio ──
        if self.messages.is_empty() && !self.streaming_active {
            draw_text(fb, content_x, content_top + 40, "[Jarbas] Como posso ajudar?", scr_w, accent.0, accent.1, accent.2);
            draw_text(fb, content_x, content_top + 60, "Digite algo no campo abaixo.", scr_w, muted.0, muted.1, muted.2);
        }
    }

    /// Renderiza texto com markup inline: **bold** e `code`
    fn render_styled_text(fb: &mut DoubleBuffer, x: usize, y: usize, text: &str, scr_w: usize, base_r: u8, base_g: u8, base_b: u8) {
        let theme = crate::display::theme::current_theme();
        let mut cx = x;
        let mut chars = text.char_indices().peekable();
        let mut in_bold = false;
        let mut in_code = false;
        let mut buf = [0u8; 4];
        while let Some((i, c)) = chars.next() {
            if c == '*' && chars.peek().map(|&(_, nc)| nc) == Some('*') {
                if in_bold { in_bold = false; chars.next(); continue; }
                else if text[i+2..].contains("**") { in_bold = true; chars.next(); continue; }
            }
            if c == 96u8 as char { in_code = !in_code; continue; }
            if c == 10u8 as char { cx = x; continue; }
            let (r, g, b) = if in_bold { (theme.accent.0, theme.accent.1, theme.accent.2) }
                else if in_code { (220, 180, 60) }
                else { (base_r, base_g, base_b) };
            let s = c.encode_utf8(&mut buf);
            crate::display::font::draw_text_scaled(fb, cx, y, s, 1, scr_w, r, g, b);
            cx += crate::display::font::CHAR_W;
        }
    }
    /// Wrap texto simples em linhas (quebra no espaço mais próximo)
    fn wrap_text(&self, text: &str, max_px: usize) -> Vec<String> {
        let char_w = font::CHAR_W;
        let max_chars = (max_px / char_w).max(8);
        let mut lines = Vec::new();
        let mut current = String::new();

        for word in text.split(' ') {
            // +1 for the space that will be added
            let test_len = if current.is_empty() { word.len() } else { current.len() + 1 + word.len() };
            if test_len > max_chars && !current.is_empty() {
                lines.push(core::mem::take(&mut current));
            }
            if !current.is_empty() { current.push(' '); }
            current.push_str(word);
        }
        if !current.is_empty() { lines.push(current); }
        if lines.is_empty() { lines.push(String::from(text)); }
        lines
    }

    /// Handle click dentro do chat window
    /// Retorna true se o clique foi consumido
    pub fn handle_click(&mut self, cx: usize, cy: usize, win_x: usize, win_y: usize, _w: usize, h: usize) -> bool {
        let input_y = win_y + h - INPUT_H;
        if cy >= input_y {
            // Mic button click (primeiros 5 chars do input bar)
            let mic_x = win_x + PAD;
            let mic_w = 5 * font::CHAR_W + 4; // "[MIC]" ou "[REC]"
            if cy < input_y + INPUT_H && cx >= mic_x && cx < mic_x + mic_w {
                let new_val = !MIC_ACTIVE.load(Ordering::Relaxed);
                MIC_ACTIVE.store(new_val, Ordering::Relaxed);
                k_nano::slog_jarbas!("CHAT", "mic", "microfone {}", if new_val { "LIGADO" } else { "DESLIGADO" });
                self.dirty = true;
                return true;
            }
            // Input area click (fora do mic) → foca input
            self.dirty = true;
            return true;
        }
        false
    }

    /// Adiciona caractere ao input buffer
    pub fn input_char(&mut self, c: char) {
        self.input_buffer.push(c);
        self.input_cursor = self.input_buffer.len();
        self.dirty = true;
    }

    /// Backspace no input
    pub fn input_backspace(&mut self) {
        self.input_buffer.pop();
        self.input_cursor = self.input_buffer.len();
        self.dirty = true;
    }

    /// Enter — publica USER_INTENT e limpa input
    pub fn input_enter(&mut self) -> String {
        let msg = core::mem::take(&mut self.input_buffer);
        self.input_cursor = 0;
        self.dirty = true;
        msg
    }
}

// ── Helpers para o DisplayAgent ──

/// Publica um StreamPacket no EventBus
pub fn publish_packet(pkt: StreamPacket) {
    let payload = pkt.encode();
    let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
        id: 0,
        topic: alloc::string::String::from(stream_packet::TOPIC_LLM_STREAM),
        payload,
        token: event_bus::CapabilityToken::Legacy(1),
    });
}
