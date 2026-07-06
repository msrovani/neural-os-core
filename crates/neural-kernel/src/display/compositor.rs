//! JARVIS Desktop — compositor multi-window + app switcher.
//! Port da UI do SmileyOS + JARVIS .NET MAUI.
//! Hermes Chat + Settings + Power apps integrados com JARVIS avatar.

use alloc::vec::Vec;
use alloc::string::String;
use spin::Mutex;
use libm::sinf;
use crate::display::fb::DoubleBuffer;

pub static COMPOSITOR: Mutex<Option<JarvisDesktop>> = Mutex::new(None);

/// Desenha texto no framebuffer (função autônoma, sem &self)
pub fn draw_text(fb: &mut DoubleBuffer, x: usize, y: usize, text: &str, scr_w: usize, r: u8, g: u8, b: u8) {
    for (i, c) in text.chars().enumerate() {
        let px = x + i * 7;
        if px + 7 > scr_w { break; }
        if let Some(bitmap) = crate::display::font::get_char_bitmap(c) {
            for dy in 0..16 {
                let row = bitmap[dy];
                for dx in 0..8 {
                    if (row >> (7 - dx)) & 1 == 1 { fb.set_pixel(px + dx, y + dy, r, g, b); }
                }
            }
        }
    }
}

/// Renderiza conteúdo de cada app (função autônoma)
fn render_app_content(fb: &mut DoubleBuffer, app: &AppWindow, scr_w: usize, scr_h: usize) {
    let cx = app.x + 4; let cy = app.y + 28;
    match app.id {
        AppId::HermesChat => {
            let lines: Vec<&str> = app.data.lines().collect();
            for (i, line) in lines.iter().enumerate().take(((app.h - 40) / 16).max(1)) {
                draw_text(fb, cx, cy + i * 16, line, scr_w, 180, 200, 220);
            }
            draw_text(fb, cx, cy + (app.h - 40) / 16, "> ", scr_w, 0, 255, 100);
        }
        AppId::Settings => {
            let items = ["[1] Theme: Dark JARVIS", "[2] Voice: Kokoro-82M", "[3] Memory: BGE", "[4] Avatar: ON", "[5] Network: Static IP"];
            for (i, item) in items.iter().enumerate() {
                draw_text(fb, cx, cy + i * 16, item, scr_w, 180, 200, 220);
            }
        }
        AppId::Power => {
            let items = ["[1] Shutdown", "[2] Reboot", "[3] Hibernate", "[4] Sleep", "[5] Cancel"];
            for (i, item) in items.iter().enumerate() {
                let (r, g, b) = if i < 3 { (255u8, 100u8, 100u8) } else { (150u8, 200u8, 150u8) };
                draw_text(fb, cx, cy + i * 20, item, scr_w, r, g, b);
            }
        }
        AppId::None => {}
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum AppId { HermesChat, Settings, Power, None }

pub struct AppWindow {
    pub id: AppId,
    pub title: String,
    pub x: usize, pub y: usize, pub w: usize, pub h: usize,
    pub visible: bool,
    pub data: String,
}

pub struct JarvisDesktop {
    pub fb: DoubleBuffer,
    pub apps: Vec<AppWindow>,
    pub active: AppId,
    pub avatar_visible: bool,
    pub status_bar_h: usize,
    pub w: usize, pub h: usize,
    pub tick: u64,
}

impl JarvisDesktop {
    pub fn new(fb: DoubleBuffer) -> Self {
        let w = fb.info.width; let h = fb.info.height;
        let sb_h = 28; // status bar height
        JarvisDesktop {
            fb, apps: Vec::new(), active: AppId::None, avatar_visible: true,
            status_bar_h: sb_h, w, h, tick: 0,
        }
    }

    pub fn register_app(&mut self, id: AppId, title: &str) {
        if self.apps.iter().any(|a| a.id == id) { return; }
        let (aw, ah) = (self.w * 4 / 5, self.h * 3 / 5);
        let ax = (self.w - aw) / 2; let ay = (self.h - ah) / 2;
        self.apps.push(AppWindow { id, title: String::from(title), x: ax, y: ay, w: aw, h: ah, visible: false, data: String::new() });
        self.active = id;
    }

    pub fn toggle_app(&mut self, id: AppId) {
        if let Some(app) = self.apps.iter_mut().find(|a| a.id == id) {
            app.visible = !app.visible;
            self.active = if app.visible { id } else { AppId::None };
        }
    }

    pub fn render(&mut self, tick: u64) {
        self.tick = tick;
        let (w, h, sb_h) = (self.w, self.h, self.status_bar_h);

        // Clear background
        self.fb.fill_rect(0, 0, w, h, 10, 10, 15);

        // Status bar
        self.fb.fill_rect(0, 0, w, sb_h, 20, 25, 35);
        let status = alloc::format!("J.A.R.V.I.S.  t:{}  v0.91  [F1]Chat [F2]Settings [F3]Power", tick);
        draw_text(&mut self.fb, 6, 4, &status, self.w, 180, 190, 210);

        // Collect visible apps first to avoid borrow conflict
        let visible_apps: Vec<usize> = self.apps.iter().enumerate().filter(|(_, a)| a.visible).map(|(i, _)| i).collect();
        for &idx in &visible_apps {
            let app = &self.apps[idx];
            self.fb.fill_rect(app.x, app.y, app.w, app.h, 15, 18, 25);
            self.fb.fill_rect(app.x, app.y, app.w, 24, 30, 40, 55);
            draw_text(&mut self.fb, app.x + 6, app.y + 4, &app.title, self.w, 200, 210, 230);
            render_app_content(&mut self.fb, app, self.w, self.h);
        }

        // JARVIS avatar overlay (canto inferior direito)
        if self.avatar_visible {
            let ax = w.saturating_sub(110); let ay = h.saturating_sub(110);
            self.fb.fill_rect(ax, ay, 100, 100, 10, 15, 20);
            let pulse = 30 + (sinf(tick as f32 * 0.05) * 10.0) as u32;
            self.fb.fill_rect(ax + 50 - pulse as usize / 2, ay + 50 - pulse as usize / 2, pulse as usize, pulse as usize, 0, 150 + pulse as u8, 255);
            draw_text(&mut self.fb, ax + 20, ay + 85, "JARVIS", self.w, 0, 200, 255);
        }

        self.fb.swap();
    }
}
