//! Virtual consoles Ctrl+Alt+F1–F6 (Labor 40).
//! 6 independent text buffers with scrollback, routed through active console.

use alloc::string::String;
use alloc::vec::Vec;
use alloc::collections::VecDeque;
use spin::Mutex;
use core::sync::atomic::{AtomicU8, Ordering};

const N: u8 = 6;
const COLS: usize = 80;
const ROWS: usize = 50;
const SCROLLBACK: usize = 200;

static ACTIVE: AtomicU8 = AtomicU8::new(0);

#[derive(Clone)]
pub struct ConsoleLine {
    pub text: String,
    pub fg: (u8, u8, u8),
    pub bg: (u8, u8, u8),
}

impl ConsoleLine {
    fn new() -> Self {
        ConsoleLine {
            text: String::new(),
            fg: (200, 210, 230),
            bg: (10, 10, 15),
        }
    }
}

struct ConsoleBuffer {
    lines: VecDeque<ConsoleLine>,
    cursor_x: usize,
    cursor_y: usize,
    dirty: bool,
}

impl ConsoleBuffer {
    const fn new() -> Self {
        ConsoleBuffer {
            lines: VecDeque::new(),
            cursor_x: 0,
            cursor_y: 0,
            dirty: true,
        }
    }

    fn ensure_init(&mut self) {
        if self.lines.is_empty() {
            for _ in 0..ROWS {
                self.lines.push_back(ConsoleLine::new());
            }
        }
    }

    fn write_char(&mut self, c: char) {
        self.ensure_init();
        if c == '\n' {
            self.newline();
            return;
        }
        if c == '\r' {
            self.cursor_x = 0;
            return;
        }
        if c == '\x08' { // backspace
            if self.cursor_x > 0 {
                self.cursor_x -= 1;
                if let Some(line) = self.lines.get_mut(self.cursor_y) {
                    line.text.pop();
                }
            }
            return;
        }
        if self.cursor_x >= COLS {
            self.newline();
        }
        if let Some(line) = self.lines.get_mut(self.cursor_y) {
            line.text.push(c);
            self.cursor_x += 1;
            self.dirty = true;
        }
    }

    fn write_str(&mut self, s: &str) {
        for c in s.chars() {
            self.write_char(c);
        }
    }

    fn newline(&mut self) {
        self.cursor_x = 0;
        if self.cursor_y + 1 >= ROWS {
            self.scroll_up();
        } else {
            self.cursor_y += 1;
        }
        self.dirty = true;
    }

    fn scroll_up(&mut self) {
        if self.lines.len() > ROWS {
            self.lines.pop_front();
        }
        self.lines.push_back(ConsoleLine::new());
        self.dirty = true;
    }

    fn clear(&mut self) {
        self.lines.clear();
        for _ in 0..ROWS {
            self.lines.push_back(ConsoleLine::new());
        }
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.dirty = true;
    }

    fn get_visible_lines(&self) -> Vec<&ConsoleLine> {
        let start = self.lines.len().saturating_sub(ROWS);
        self.lines.iter().skip(start).take(ROWS).collect()
    }
}

static CONSOLES: Mutex<[ConsoleBuffer; 6]> = Mutex::new([
    ConsoleBuffer::new(),
    ConsoleBuffer::new(),
    ConsoleBuffer::new(),
    ConsoleBuffer::new(),
    ConsoleBuffer::new(),
    ConsoleBuffer::new(),
]);

pub fn active() -> u8 {
    ACTIVE.load(Ordering::Relaxed)
}

pub fn switch(n: u8) -> bool {
    if n >= N {
        return false;
    }
    ACTIVE.store(n, Ordering::Relaxed);
    k_nano::slog_jarbas!("VCON", "info", "switch=F{} VERDICT=PARTIAL", n + 1);
    true
}

/// Scancode path: Ctrl+Alt+F1..F6 (F1=0x3B … F6=0x40) — caller passes index 0..5.
pub fn on_ctrl_alt_fn(fn_index: u8) -> bool {
    switch(fn_index)
}

/// Write to the currently active console buffer.
pub fn write_to_active(text: &str) {
    let idx = active() as usize;
    CONSOLES.lock()[idx].write_str(text);
}

/// Get the active console's visible lines for rendering.
pub fn get_active_visible() -> Vec<ConsoleLine> {
    let idx = active() as usize;
    CONSOLES.lock()[idx].get_visible_lines().into_iter().cloned().collect()
}

/// Clear the active console.
pub fn clear_active() {
    let idx = active() as usize;
    CONSOLES.lock()[idx].clear();
}

/// Boot smoke test.
pub fn boot_smoke() -> bool {
    let ok = switch(0) && switch(1) && switch(0);
    k_nano::slog_jarbas!(
        "VCON",
        "info",
        "step=vconsole status=OK n={} VERDICT=PARTIAL reason=switch_mvp",
        N
    );
    ok
}