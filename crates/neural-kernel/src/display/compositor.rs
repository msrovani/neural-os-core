//! JARVIS Desktop — compositor multi-window + app icons + WASM skill launcher.
//! Port da UI do SmileyOS + JARVIS .NET MAUI.

use alloc::vec::Vec;
use alloc::string::String;
use spin::Mutex;
use libm::sinf;
use crate::display::fb::DoubleBuffer;

pub static COMPOSITOR: Mutex<Option<JarvisDesktop>> = Mutex::new(None);

pub fn draw_text(fb: &mut DoubleBuffer, x: usize, y: usize, text: &str, scr_w: usize, r: u8, g: u8, b: u8) {
    crate::display::font::draw_text_scaled(fb, x, y, text, 1, scr_w, r, g, b);
}

/// #82: Renderiza tensor visualization overlay
pub fn render_tensor_viz(fb: &mut DoubleBuffer, x: usize, y: usize, w: usize, h: usize) {
    use core::f32::consts::PI;
    use libm::sinf;
    let mut data = [0.0f32; 400];
    for i in 0..20 { for j in 0..20 { data[i*20+j] = (sinf(i as f32*0.3)*sinf(j as f32*0.3)).abs()*0.5+0.5; } }
    crate::display::font::draw_tensor_heatmap(fb, x, y, &data, 20, 20);
    draw_text(fb, x, y+85, "Attention", fb.info.width, 0,200,255);
    let bars = [0.9f32,0.7,0.5,0.8,0.3,0.6,0.4,0.7,0.2,0.5];
    crate::display::font::draw_attention_graph(fb, x, y+105, &bars, 100, 25);
    draw_text(fb, x, y+135, "Scores", fb.info.width, 0,200,100);
}

#[derive(Clone, Copy, PartialEq)]
pub enum AppId { HermesChat, Settings, Power, Ide, WasmSkill(usize), None }

pub struct AppWindow { pub id: AppId, pub title: String, pub x: usize, pub y: usize, pub w: usize, pub h: usize, pub visible: bool, pub data: String }

#[derive(Clone)]
pub struct WasmIcon { pub name: String, pub description: String, pub idx: usize }

pub struct JarvisDesktop {
    pub fb: DoubleBuffer,
    pub apps: Vec<AppWindow>,
    pub wasm_skills: Vec<WasmIcon>,
    pub active: AppId,
    pub avatar_visible: bool,
    pub w: usize, pub h: usize, pub tick: u64,
}

impl JarvisDesktop {
    pub fn new(fb: DoubleBuffer) -> Self {
        let w = fb.info.width; let h = fb.info.height;
        JarvisDesktop { fb, apps: Vec::new(), wasm_skills: Vec::new(), active: AppId::None, avatar_visible: true, w, h, tick: 0 }
    }

    pub fn register_app(&mut self, id: AppId, title: &str) {
        if self.apps.iter().any(|a| a.id == id) { return; }
        let (aw, ah) = (self.w * 4 / 5, self.h * 3 / 5);
        let ax = (self.w - aw) / 2; let ay = (self.h - ah) / 2;
        self.apps.push(AppWindow { id, title: String::from(title), x: ax, y: ay, w: aw, h: ah, visible: false, data: String::new() });
    }

    /// Cria um novo ícone WASM no desktop. Quando clicado, roda o runtime.
    pub fn publish_wasm_skill(&mut self, name: &str, description: &str) {
        let idx = self.wasm_skills.len();
        self.wasm_skills.push(WasmIcon { name: String::from(name), description: String::from(description), idx });
        // Cria uma janela de app para o WASM skill
        let id = AppId::WasmSkill(idx);
        let (aw, ah) = (self.w * 3 / 5, self.h * 2 / 5);
        self.apps.push(AppWindow { id, title: alloc::format!("⚡ {}", name), x: 60, y: 60, w: aw, h: ah, visible: false, data: String::new() });
    }

    pub fn toggle_app(&mut self, id: AppId) {
        if let Some(app) = self.apps.iter_mut().find(|a| a.id == id) { app.visible = !app.visible; self.active = if app.visible { id } else { AppId::None }; }
    }

    pub fn render(&mut self, tick: u64) {
        self.tick = tick; let (w, h) = (self.w, self.h);
        self.fb.fill_rect(0, 0, w, h, 10, 10, 15);

        // Status bar
        self.fb.fill_rect(0, 0, w, 28, 20, 25, 35);
        draw_text(&mut self.fb, 6, 4, &alloc::format!("J.A.R.V.I.S.  t:{}  [F1]Chat [F2]Settings [F3]Power [F4]IDE", tick), self.w, 180, 190, 210);

        // Desktop icons for WASM skills
        for (i, skill) in self.wasm_skills.iter().enumerate() {
            let ix = 20 + (i % 6) * 100; let iy = 40 + (i / 6) * 90;
            self.fb.fill_rect(ix, iy, 80, 70, 30, 40, 55); // icon bg
            self.fb.fill_rect(ix + 20, iy + 5, 40, 40, 60, 120, 200); // icon square
            draw_text(&mut self.fb, ix + 5, iy + 52, &skill.name, self.w, 200, 200, 200);
        }

        // Tensor visualization panel (canto superior direito)
        render_tensor_viz(&mut self.fb, w.saturating_sub(160), 35, 120, 160);

        // App windows
        for app in &self.apps {
            if !app.visible { continue; }
            self.fb.fill_rect(app.x, app.y, app.w, app.h, 15, 18, 25);
            self.fb.fill_rect(app.x, app.y, app.w, 24, 30, 40, 55);
            draw_text(&mut self.fb, app.x + 6, app.y + 4, &app.title, self.w, 200, 210, 230);
            render_app_content(&mut self.fb, app, self.w, self.h);
        }

        // JARVIS avatar
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

fn render_app_content(fb: &mut DoubleBuffer, app: &AppWindow, scr_w: usize, scr_h: usize) {
    let cx = app.x + 4; let cy = app.y + 28;
    match app.id {
        AppId::HermesChat => {
            let lines: Vec<&str> = app.data.lines().collect();
            for (i, line) in lines.iter().enumerate().take(((app.h - 40) / 16).max(1)) { draw_text(fb, cx, cy + i * 16, line, scr_w, 180, 200, 220); }
            draw_text(fb, cx, cy + (app.h - 40) / 16, "> ", scr_w, 0, 255, 100);
        }
        AppId::Settings => {
            let items = ["[1] Theme", "[2] Voice: Kokoro", "[3] Memory: BGE", "[4] Avatar", "[5] Network"];
            for (i, item) in items.iter().enumerate() { draw_text(fb, cx, cy + i * 16, item, scr_w, 180, 200, 220); }
        }
        AppId::Power => {
            let items = ["[1] Shutdown", "[2] Reboot", "[3] Hibernate"];
            for (i, item) in items.iter().enumerate() { let (r,g,b) = if i < 3 {(255u8,100u8,100u8)} else {(150u8,200u8,150u8)}; draw_text(fb, cx, cy + i * 20, item, scr_w, r, g, b); }
        }
        AppId::Ide => {
            draw_text(fb, cx, cy, "BitNet IDE v0.1", scr_w, 0, 200, 255);
            draw_text(fb, cx, cy + 20, "Describe a skill:", scr_w, 180, 200, 220);
            draw_text(fb, cx, cy + 40, &app.data, scr_w, 200, 200, 200);
            draw_text(fb, cx, cy + 60, "[Enter] Generate WASM skill", scr_w, 100, 200, 100);
        }
        AppId::WasmSkill(idx) => {
            draw_text(fb, cx, cy, &alloc::format!("WASM Skill #{}", idx), scr_w, 0, 200, 255);
            draw_text(fb, cx, cy + 20, "Running via WASM Runtime...", scr_w, 180, 200, 220);
            draw_text(fb, cx, cy + 40, &app.data, scr_w, 200, 200, 200);
        }
        AppId::WasmSkill(_) | AppId::None => {}
    }
}


