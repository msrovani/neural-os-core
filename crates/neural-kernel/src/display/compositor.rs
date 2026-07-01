//! Compositor — gerenciador de janelas multi-window.
//! Renderiza janelas em ordem z-index, desenha dock bar, processa mouse.
//! Subscreve MOUSE_MOVED e MOUSE_CLICK do MouseAgent.

use alloc::vec;
use alloc::vec::Vec;
use alloc::string::String;
use alloc::boxed::Box;
use spin::Mutex;
use crate::display::fb::DoubleBuffer;
use crate::display::font;
use crate::display::theme;

pub const DOCK_H: u32 = 36;
pub const TITLE_H: u32 = 20;

pub static COMPOSITOR: Mutex<Option<Compositor>> = Mutex::new(None);

#[derive(Debug, Clone)]
pub struct Window {
    pub id: u32,
    pub title: String,
    pub x: i32, pub y: i32, pub w: u32, pub h: u32,
    pub z: i32,
    pub minimized: bool,
    pub buffer: Vec<u8>,
}

impl Window {
    pub fn new(id: u32, title: &str, x: i32, y: i32, w: u32, h: u32) -> Self {
        let buf_size = (w * h * 4) as usize;
        Window {
            id, title: String::from(title),
            x, y, w, h, z: id as i32,
            minimized: false,
            buffer: vec![0u8; buf_size],
        }
    }

    pub fn contains(&self, mx: i32, my: i32) -> bool {
        mx >= self.x && mx < (self.x + self.w as i32)
        && my >= self.y && my < (self.y + self.h as i32)
    }
}

pub struct Compositor {
    pub windows: Vec<Window>,
    next_id: u32,
    pub mouse_x: i32,
    pub mouse_y: i32,
    drag_win: Option<u32>,
    drag_off_x: i32, drag_off_y: i32,
    pub focus: Option<u32>,
    pub input_text: String,
    mouse_receiver: crate::Receiver,
    click_receiver: crate::Receiver,
    clock_ticks: u64,
}

impl Compositor {
    pub fn new() -> Self {
        Compositor {
            windows: Vec::new(),
            next_id: 1,
            mouse_x: 640, mouse_y: 360,
            drag_win: None, drag_off_x: 0, drag_off_y: 0,
            focus: None,
            input_text: String::new(),
            mouse_receiver: crate::EVENT_BUS.subscribe(crate::agents::mouse_agent::TOPIC_MOUSE_MOVED),
            click_receiver: crate::EVENT_BUS.subscribe(crate::agents::mouse_agent::TOPIC_MOUSE_CLICK),
            clock_ticks: 0,
        }
    }

    pub fn create_window(&mut self, title: &str, w: u32, h: u32) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let x = 50 + (self.windows.len() as i32 * 20) % 300;
        let y = 40 + (self.windows.len() as i32 * 20) % 200;
        self.windows.push(Window::new(id, title, x, y, w, h));
        self.focus = Some(id);
        id
    }

    pub fn poll_mouse(&mut self) {
        while let Some(ev) = self.mouse_receiver.try_receive() {
            if ev.payload.len() >= 8 {
                let x = i32::from(i16::from_le_bytes([ev.payload[0], ev.payload[1]]));
                let y = i32::from(i16::from_le_bytes([ev.payload[2], ev.payload[3]]));
                self.mouse_x = x;
                self.mouse_y = y;

                if let Some(drag_id) = self.drag_win {
                    let drag_id2 = drag_id;
                    let drag_off_x2 = self.drag_off_x;
                    let drag_off_y2 = self.drag_off_y;
                    for win in self.windows.iter_mut() {
                        if win.id == drag_id2 {
                            win.x = x - drag_off_x2;
                            win.y = y - drag_off_y2;
                        }
                    }
                }
            }
        }

        let mut click_handled = false;
        while let Some(ev) = self.click_receiver.try_receive() {
            if ev.payload.len() >= 5 {
                let _button = ev.payload[0];
                let mx = i32::from(i16::from_le_bytes([ev.payload[1], ev.payload[2]]));
                let my = i32::from(i16::from_le_bytes([ev.payload[3], ev.payload[4]]));
                let screen_h = 720i32;

                // Hit test dock
                if my >= screen_h.saturating_sub(DOCK_H as i32) {
                    let dock_x = mx / 64;
                    if dock_x >= 0 && (dock_x as usize) < self.windows.len() {
                        let idx = dock_x as usize;
                        self.windows[idx].minimized = false;
                        self.focus = Some(self.windows[idx].id);
                        click_handled = true;
                    }
                    continue;
                }

                // Copy windows indices sorted by z descending
                let indices: Vec<usize> = {
                    let mut v: Vec<usize> = (0..self.windows.len()).collect();
                    v.sort_by(|&a, &b| self.windows[b].z.cmp(&self.windows[a].z));
                    v
                };

                for &idx in &indices {
                    let in_title;
                    let in_body;
                    let w_id;
                    {
                        let win = &self.windows[idx];
                        if win.minimized { continue; }
                        w_id = win.id;
                        in_title = mx >= win.x && mx < win.x + win.w as i32
                            && my >= win.y && my < win.y + TITLE_H as i32;
                        in_body = mx >= win.x && mx < win.x + win.w as i32
                            && my >= win.y && my < win.y + win.h as i32;
                    }

                    if in_title {
                        self.focus = Some(w_id);
                        let max_z = self.windows.iter().map(|w| w.z).max().unwrap_or(0);
                        if let Some(w) = self.windows.iter_mut().find(|w| w.id == w_id) {
                            w.z = max_z + 1;
                            self.drag_win = Some(w_id);
                            self.drag_off_x = mx - w.x;
                            self.drag_off_y = my - w.y;
                        }
                        click_handled = true;
                        break;
                    }
                    if in_body {
                        self.focus = Some(w_id);
                        let max_z = self.windows.iter().map(|w| w.z).max().unwrap_or(0);
                        if let Some(w) = self.windows.iter_mut().find(|w| w.id == w_id) {
                            w.z = max_z + 1;
                        }
                        click_handled = true;
                        break;
                    }
                }
            }
        }

        if !click_handled {
            self.drag_win = None;
        }
    }

    pub fn render(&mut self, fb: &mut DoubleBuffer, tick: u64) {
        let w = fb.info.width;
        let h = fb.info.height;
        let t = theme::current();
        self.clock_ticks = tick;

        fb.fill_rect(0, 0, w, h, t.bg.0, t.bg.1, t.bg.2);

        // Sort windows by z
        let mut order: Vec<usize> = (0..self.windows.len()).collect();
        order.sort_by(|&a, &b| self.windows[a].z.cmp(&self.windows[b].z));

        for &idx in &order {
            let win = &self.windows[idx];
            if win.minimized { continue; }

            Self::draw_rect(fb, win.x, win.y, win.w, win.h, t.terminal_bg);
            Self::draw_rect(fb, win.x, win.y, win.w, TITLE_H, t.accent);

            let title_color = if Some(win.id) == self.focus { t.fg } else { t.secondary };
            Self::draw_text(fb, win.x + 4, win.y + 2, &win.title, title_color, t.bg);
        }

        // Dock
        let dock_y = (h as i32).saturating_sub(DOCK_H as i32);
        Self::draw_rect(fb, 0, dock_y, w as u32, DOCK_H, t.secondary);

        // Workspace indicator (COSMIC-style)
        let ws_indicator = alloc::format!("WS:{}", 
            if crate::display::workspace::WORKSPACE_MANAGER.lock().is_some() {
                let wm = crate::display::workspace::WORKSPACE_MANAGER.lock();
                let ws = wm.as_ref().unwrap();
                ws.current + 1
            } else { 1 }
        );
        Self::draw_text(fb, 4, dock_y + DOCK_H as i32 / 2 - 4, &ws_indicator, t.accent, t.bg);

        for (i, win) in self.windows.iter().enumerate() {
            let label: String = win.title.chars().take(6).collect();
            let col = if Some(win.id) == self.focus { t.accent } else { t.fg };
            Self::draw_text(fb, (i * 64 + 60) as i32, dock_y + DOCK_H as i32 / 2 - 4, &label, col, t.bg);
        }

        // Clock — minutos reais (tick / 18 = segundos)
        let total_sec = tick / 18;
        let hrs = (total_sec / 3600) % 24;
        let mins = (total_sec / 60) % 60;
        let clock = alloc::format!("{:02}:{:02}", hrs, mins);
        Self::draw_text(fb, (w as i32) - 64, dock_y + DOCK_H as i32 / 2 - 4, &clock, t.fg, t.bg);

        // Keyboard input buffer — mostra digitacao acima da dock
        if !self.input_text.is_empty() {
            let input_y = dock_y - 20;
            Self::draw_text(fb, 4, input_y, &alloc::format!("> {}", self.input_text), t.accent, t.bg);
        }

        // Cursor
        let cx = self.mouse_x as usize;
        let cy = self.mouse_y as usize;
        if cx < w && cy < h {
            for dx in 0..7usize { let px = cx + dx; if px < w { fb.set_pixel(px, cy, t.accent.0, t.accent.1, t.accent.2); } }
            for dy in 0..7usize { let py = cy + dy; if py < h { fb.set_pixel(cx, py, t.accent.0, t.accent.1, t.accent.2); } }
        }
    }

    pub fn draw_rect(fb: &mut DoubleBuffer, x: i32, y: i32, w: u32, h: u32, color: (u8,u8,u8)) {
        if x < 0 || y < 0 { return; }
        fb.fill_rect(x as usize, y as usize, w as usize, h as usize, color.0, color.1, color.2);
    }

    pub fn draw_text(fb: &mut DoubleBuffer, x: i32, y: i32, text: &str, fg: (u8,u8,u8), bg: (u8,u8,u8)) {
        let mut cx = x.max(0) as usize;
        let cy = y.max(0) as usize;
        for c in text.chars() {
            if let Some(bitmap) = font::get_char_bitmap(c) {
                for dy in 0..font::CHAR_H {
                    let row = bitmap[dy];
                    for dx in 0..font::CHAR_W {
                        if cx + dx < fb.info.width && cy + dy < fb.info.height {
                            if (row >> (7 - dx)) & 1 == 1 {
                                fb.set_pixel(cx + dx, cy + dy, fg.0, fg.1, fg.2);
                            } else {
                                fb.set_pixel(cx + dx, cy + dy, bg.0, bg.1, bg.2);
                            }
                        }
                    }
                }
                cx += font::CHAR_W;
            }
        }
    }
}
