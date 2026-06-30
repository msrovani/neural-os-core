//! Neural Console — layout multi-região sobre o framebuffer.
//!
//! Divide a tela em:
//! - Status bar (topo): tick, agentes, memória, LLM, rede
//! - Conversation area (meio): histórico Hermes
//! - Tensor strip (fundo): faixa colorida refletindo hardware context

use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;
use crate::display::fb::DoubleBuffer;
use crate::display::font;
use crate::profile::ProfileManager;

const COL_BG: (u8, u8, u8) = (10, 10, 20);
const COL_FG: (u8, u8, u8) = (200, 200, 220);
const COL_STATUS_BG: (u8, u8, u8) = (20, 20, 40);
const COL_GREEN: (u8, u8, u8) = (0, 200, 100);
const COL_YELLOW: (u8, u8, u8) = (220, 200, 0);
const COL_RED: (u8, u8, u8) = (220, 50, 50);
const COL_CYAN: (u8, u8, u8) = (0, 200, 200);
const COL_BLUE: (u8, u8, u8) = (80, 130, 220);

/// Console neural multi-região com double buffering
pub struct NeuralConsole {
    pub fb: DoubleBuffer,
    conv_lines: VecDeque<String>,
    _tensor_value: f32,
    input_buffer: String,
    show_prompt: bool,
}

impl NeuralConsole {
    pub fn new(fb: DoubleBuffer) -> Self {
        NeuralConsole { fb, conv_lines: VecDeque::new(), _tensor_value: 0.0, input_buffer: String::new(), show_prompt: false }
    }

    /// Renderiza um frame completo no back buffer, depois faz swap
    pub fn render(&mut self, tick: u64, agent_count: usize, mem_pct: f32, llm_busy: bool, net_online: bool) {
        let w = self.fb.info.width;
        let h = self.fb.info.height;

        let profile = ProfileManager::get();
        let (bg, accent, fg_user) = profile.theme_colors();

        self.fb.fill_rect(0, 0, w, h, bg.0, bg.1, bg.2);

        // Tensor strip (topo 4px)
        for x in 0..w {
            let t = (x as f32 / w as f32);
            let r = (30.0 + t * mem_pct * 200.0) as u8;
            let g = (10.0 + (1.0 - t) * 60.0) as u8;
            self.fb.set_pixel(x, 0, r, g, 30 + (t * 150.0) as u8);
            self.fb.set_pixel(x, 1, r/2, g/2, 15);
        }

        // Status bar — fundo ocupa apenas a altura do texto + padding 2px
        let status_y = 4;
        let ch = font::CHAR_H;
        let cw = font::CHAR_W;
        let sb_h = ch + 3; // altura real da status bar

        self.fb.fill_rect(0, status_y - 1, w, sb_h, COL_STATUS_BG.0, COL_STATUS_BG.1, COL_STATUS_BG.2);

        let llm_str = if llm_busy { "LLM:gen" } else { "LLM:idle" };
        let net_str = if net_online { "NET:on" } else { "NET:off" };
        let mem_str = alloc::format!("{:.0}%", mem_pct * 100.0);
        let profile_icon = profile.icon();
        let profile_name = profile.name();
        let status = alloc::format!("{} {} t:{} ag:{} mem:{} {} {}",
            profile_icon, profile_name, tick, agent_count, mem_str, llm_str, net_str);
        self.draw_text(2, status_y, &status, accent);

        // Conversation area — comeca logo apos a status bar
        let conv_y = status_y + sb_h;
        let max_lines = (h.saturating_sub(conv_y + ch + 2)) / ch;
        let lines: alloc::vec::Vec<(String, (u8,u8,u8))> = self.conv_lines.iter()
            .skip(self.conv_lines.len().saturating_sub(max_lines))
            .map(|line| (line.clone(), self.color_for_line(line)))
            .collect();
        for (i, (line, color)) in lines.iter().enumerate() {
            let y = conv_y + i * ch;
            self.draw_text(2, y, line, *color);
        }

        // Prompt area (ultima linha)
        if self.show_prompt {
            let prompt_y = h - ch - 2;
            let prompt_text = alloc::format!("> {}", self.input_buffer);
            self.fb.fill_rect(0, prompt_y - 1, w, ch + 3, COL_STATUS_BG.0, COL_STATUS_BG.1, COL_STATUS_BG.2);
            self.draw_text(2, prompt_y, &prompt_text, fg_user);
        }

        // Bottom separator
        if h > 6 {
            self.fb.set_pixel(0, h - 3, accent.0/2, accent.1/2, accent.2/2);
        }

        // Swap buffers — elimina cintilacao
        self.fb.swap();
    }

    pub fn push_line(&mut self, line: &str) {
        if line.len() > 250 {
            let truncated: String = line.chars().take(247).collect();
            self.conv_lines.push_back(truncated + "...");
        } else {
            self.conv_lines.push_back(String::from(line));
        }
        if self.conv_lines.len() > 2000 {
            self.conv_lines.pop_front();
        }
    }

    pub fn set_input_buffer(&mut self, buf: &str) {
        self.input_buffer = String::from(buf);
    }

    pub fn set_prompt_visible(&mut self, visible: bool) {
        self.show_prompt = visible;
    }

    pub fn clear_input(&mut self) {
        self.input_buffer.clear();
    }

    fn color_for_line(&self, line: &str) -> (u8, u8, u8) {
        if line.starts_with("[Hermes]") { COL_GREEN }
        else if line.starts_with("[CORTEX]") || line.starts_with("[CORTEX-LLM]") { COL_YELLOW }
        else if line.starts_with("[NET]") { COL_CYAN }
        else if line.starts_with("[SKILL]") { COL_BLUE }
        else if line.starts_with("[ERROR]") || line.starts_with("[PANIC]") { COL_RED }
        else if line.starts_with("[SECURITY]") { COL_RED }
        else { COL_FG }
    }

    fn draw_char(&mut self, x: usize, y: usize, c: char, fg: (u8, u8, u8)) {
        if let Some(bitmap) = font::get_char_bitmap(c) {
            for dy in 0..font::CHAR_H {
                let row = bitmap[dy];
                for dx in 0..font::CHAR_W {
                    if (row >> (7 - dx)) & 1 == 1 {
                        self.fb.set_pixel(x + dx, y + dy, fg.0, fg.1, fg.2);
                    } else {
                        self.fb.set_pixel(x + dx, y + dy, COL_BG.0, COL_BG.1, COL_BG.2);
                    }
                }
            }
        }
    }

    fn draw_text(&mut self, x: usize, y: usize, text: &str, fg: (u8, u8, u8)) {
        let mut cx = x;
        let w = self.fb.info.width;
        for c in text.chars() {
            if cx + font::CHAR_W > w { break; }
            self.draw_char(cx, y, c, fg);
            cx += font::CHAR_W;
        }
    }
}
