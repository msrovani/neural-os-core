//! JARVIS Desktop — compositor multi-window + app icons + WASM skill launcher + LLM icons.
//! Port da UI do SmileyOS + JARVIS .NET MAUI.

/// Gera um icone (padrao 8x8) via IA a partir de uma descricao.
/// Usa o HWEXPERT_MODEL para sintetizar um pequeno bitmap.
pub fn generate_llm_icon(description: &str) -> [u8; 64] {
    let mut icon = [0u8; 64];
    if let Some(ref model) = *cortex::cortex::HWEXPERT_MODEL.lock() {
        let prompt = alloc::format!("gere icone 8x8 para: {}", description);
        let resp = model.generate(&prompt);
        // Parseia resposta como numeros 0-255
        let nums: Vec<u8> = resp.bytes().filter(|b| b.is_ascii_digit()).collect();
        for (i, &b) in nums.iter().enumerate().take(64) {
            icon[i] = b;
        }
    }
    // Se falhou, gera padrao geometrico baseado no hash da descricao
    if icon.iter().all(|&b| b == 0) {
        let hash = description.bytes().fold(0u64, |h, b| h.wrapping_mul(31).wrapping_add(b as u64));
        for i in 0..64 {
            let bit = (hash >> (i % 64)) & 1;
            icon[i] = if bit == 1 { 200 } else { 30 };
        }
    }
    icon
}

use alloc::vec::Vec;
use alloc::string::String;
use alloc::collections::BTreeMap;
use spin::Mutex;
use libm::sinf;
use crate::display::fb::DoubleBuffer;

pub static COMPOSITOR: Mutex<Option<JarvisDesktop>> = Mutex::new(None);

pub fn draw_text(fb: &mut DoubleBuffer, x: usize, y: usize, text: &str, scr_w: usize, r: u8, g: u8, b: u8) {
    crate::display::font::draw_text_scaled(fb, x, y, text, 1, scr_w, r, g, b);
}

/// #82: Renderiza tensor visualization overlay
pub fn render_tensor_viz(fb: &mut DoubleBuffer, x: usize, y: usize, _w: usize, _h: usize) {
    
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
pub enum AppId { HermesChat, Settings, Power, Ide, WasmSkill(usize), Camera, AudioViz, None }

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer { OrbBackground, HermesOverlay, AppWindows, DockBar }

#[derive(Clone)]
pub struct AppWindow { pub id: AppId, pub title: String, pub x: usize, pub y: usize, pub w: usize, pub h: usize, pub visible: bool, pub data: String, pub z: Layer }

// Estado global do mouse para o compositor
pub static MOUSE_X: spin::Mutex<usize> = spin::Mutex::new(640);
pub static MOUSE_Y: spin::Mutex<usize> = spin::Mutex::new(360);
pub static MOUSE_BUTTONS: spin::Mutex<u8> = spin::Mutex::new(0);

// Timing de frame para FPS control
pub static LAST_FRAME_TICK: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
pub const TARGET_FRAME_TICKS: u64 = 3; // ~60 FPS (assumindo ~5ms/tick)

#[derive(Clone)]
pub struct WasmIcon { pub name: String, pub description: String, pub idx: usize }

pub struct JarvisDesktop {
    pub fb: DoubleBuffer,
    pub apps: Vec<AppWindow>,
    pub wasm_skills: Vec<WasmIcon>,
    pub active: AppId,
    pub avatar_visible: bool,
    pub w: usize, pub h: usize, pub tick: u64,
    icon_cache: BTreeMap<String, [u8; 64]>,
}

impl JarvisDesktop {
    pub fn new(fb: DoubleBuffer) -> Self {
        let w = fb.info.width; let h = fb.info.height;
        JarvisDesktop { fb, apps: Vec::new(), wasm_skills: Vec::new(), active: AppId::None, avatar_visible: true, w, h, tick: 0, icon_cache: BTreeMap::new() }
    }

    pub fn register_app(&mut self, id: AppId, title: &str, z: Layer) {
        if self.apps.iter().any(|a| a.id == id) { return; }
        let (aw, ah) = (self.w * 4 / 5, self.h * 3 / 5);
        let ax = (self.w - aw) / 2; let ay = (self.h - ah) / 2;
        self.apps.push(AppWindow { id, title: String::from(title), x: ax, y: ay, w: aw, h: ah, visible: false, data: String::new(), z });
    }

    /// Retorna o overlay Hermes (cria se nao existe)
    pub fn ensure_hermes_overlay(&mut self) -> usize {
        let w = self.w * 2 / 5; let h = self.h / 3;
        for (i, app) in self.apps.iter().enumerate() {
            if app.id == AppId::HermesChat { return i; }
        }
        self.apps.push(AppWindow {
            id: AppId::HermesChat, title: String::from("Hermes"),
            x: self.w.saturating_sub(w + 10), y: 35,
            w, h, visible: true, data: String::new(), z: Layer::HermesOverlay,
        });
        self.apps.len() - 1
    }

    pub fn publish_wasm_skill(&mut self, name: &str, description: &str) {
        let idx = self.wasm_skills.len();
        self.wasm_skills.push(WasmIcon { name: String::from(name), description: String::from(description), idx });
        let id = AppId::WasmSkill(idx);
        let (aw, ah) = (self.w * 3 / 5, self.h * 2 / 5);
        self.apps.push(AppWindow { id, title: alloc::format!("⚡ {}", name), x: 60, y: 60, w: aw, h: ah, visible: false, data: String::new(), z: Layer::AppWindows });
    }

    pub fn toggle_app(&mut self, id: AppId) {
        if let Some(app) = self.apps.iter_mut().find(|a| a.id == id) { app.visible = !app.visible; self.active = if app.visible { id } else { AppId::None }; }
    }

    /// Arrasta janela: atualiza posicao se arrastando pela title bar
    pub fn drag_window(&mut self, id: AppId, dx: isize, dy: isize) {
        if let Some(app) = self.apps.iter_mut().find(|a| a.id == id && a.visible) {
            let nx = (app.x as isize + dx).max(0) as usize;
            let ny = (app.y as isize + dy).max(28) as usize; // nao encobre status bar
            app.x = nx.min(self.w.saturating_sub(100));
            app.y = ny.min(self.h.saturating_sub(100));
        }
    }

    /// Redimensiona janela: ajusta tamanho a partir do canto inferior direito
    pub fn resize_window(&mut self, id: AppId, dw: isize, dh: isize) {
        if let Some(app) = self.apps.iter_mut().find(|a| a.id == id && a.visible) {
            let nw = (app.w as isize + dw).max(160) as usize;
            let nh = (app.h as isize + dh).max(80) as usize;
            app.w = nw.min(self.w - app.x);
            app.h = nh.min(self.h - app.y);
        }
    }

    /// Fecha janela (toggle off)
    pub fn close_window(&mut self, id: AppId) {
        if let Some(app) = self.apps.iter_mut().find(|a| a.id == id) {
            app.visible = false;
            self.active = AppId::None;
        }
    }

    /// Dock: retorna lista de apps visiveis para a dock bar
    pub fn dock_apps(&self) -> Vec<(AppId, &str)> {
        let mut v = Vec::new();
        for app in &self.apps {
            if app.visible {
                let name = match app.id {
                    AppId::HermesChat => "Chat",
                    AppId::Settings => "Settings",
                    AppId::Power => "Power",
                    AppId::Ide => "IDE",
                    AppId::Camera => "Camera",
                    AppId::AudioViz => "Audio",
                    AppId::WasmSkill(_) => "WASM",
                    AppId::None => "",
                };
                v.push((app.id, name));
            }
        }
        v
    }

    pub fn render(&mut self, tick: u64) {
        self.tick = tick; let (w, h) = (self.w, self.h);

        // ── FPS control: skip frame se estiver muito rapido ──
        let last = LAST_FRAME_TICK.load(core::sync::atomic::Ordering::Relaxed);
        if tick.wrapping_sub(last) < TARGET_FRAME_TICKS { return; }
        LAST_FRAME_TICK.store(tick, core::sync::atomic::Ordering::Relaxed);

        // ═══════════════════════════════════════════════════════
        // LAYER 0: Fundo + Orb (mais profundo)
        // ═══════════════════════════════════════════════════════
        self.fb.fill_rect(0, 0, w, h, 10, 10, 15);

        // Orb JARVIS como layer de fundo (centro, pulsante)
        if self.avatar_visible {
            let orb = 60 + (sinf(tick as f32 * 0.04) * 20.0) as u32;
            let ax = (w - orb as usize) / 2;
            let ay = (h - orb as usize) / 2;
            // Glow
            self.fb.fill_rect(ax.saturating_sub(10), ay.saturating_sub(10), orb as usize + 20, orb as usize + 20, 0, 20, 40);
            // Orb com FFT audio (se disponivel) ou pulsacao padrao
            let fft_energy = crate::display::avatar::read_audio_energy();
            let bri = if fft_energy > 0.0 {
                130 + (fft_energy.min(1.0) * 100.0) as u8
            } else {
                130 + (sinf(tick as f32 * 0.05) * 40.0) as u8
            };
            let _g = if fft_energy > 0.5 { bri / 2 } else { bri / 3 };
            self.fb.fill_rect(ax, ay, orb as usize, orb as usize, 0, bri, 255);
            draw_text(&mut self.fb, ax + orb as usize / 2 - 20, ay + orb as usize + 5, "JARVIS", self.w, 0, 200, 255);
        }

        // ═══════════════════════════════════════════════════════
        // LAYER 1: Hermes CLI Overlay (semi-transparente, canto)
        // ═══════════════════════════════════════════════════════
        let overlay_w = w * 2 / 5;
        let overlay_h = h / 3;
        let ox = w.saturating_sub(overlay_w + 10);
        let oy = 35;
        // Fundo semi-transparente (alpha simulado com cor escura)
        self.fb.fill_rect(ox, oy, overlay_w, overlay_h, 8, 10, 15);
        // Borda sutil
        self.fb.fill_rect(ox.saturating_sub(1), oy.saturating_sub(1), overlay_w + 2, overlay_h + 2, 20, 25, 35);
        // Label Hermes
        draw_text(&mut self.fb, ox + 4, oy + 2, "[Hermes Console]", self.w, 0, 200, 255);
        // Conteudo do overlay: ultimas linhas do chat
        let cli_content = crate::display::console::get_overlay_text();
        let lines: Vec<&str> = cli_content.lines().collect();
        let start = lines.len().saturating_sub(((overlay_h - 24) / 16).max(1));
        for (i, line) in lines.iter().enumerate().skip(start).take((overlay_h - 24) / 16) {
            draw_text(&mut self.fb, ox + 4, oy + 22 + (i - start) * 16, line, self.w, 150, 200, 220);
        }

        // ═══════════════════════════════════════════════════════
        // LAYER 2: App Windows (ordenado por z_index)
        // ═══════════════════════════════════════════════════════
        let mut sorted: Vec<&AppWindow> = self.apps.iter().filter(|a| a.visible).collect();
        sorted.sort_by_key(|a| a.z);
        for app in &sorted {
            self.fb.fill_rect(app.x.saturating_sub(1), app.y.saturating_sub(1), app.w + 2, app.h + 2, 40, 50, 65);
            self.fb.fill_rect(app.x, app.y, app.w, app.h, 15, 18, 25);
            self.fb.fill_rect(app.x, app.y, app.w, 24, 30, 40, 55);
            draw_text(&mut self.fb, app.x + 6, app.y + 4, &app.title, self.w, 200, 210, 230);
            // Close button
            self.fb.fill_rect(app.x + app.w - 20, app.y + 3, 16, 16, 200, 50, 50);
            draw_text(&mut self.fb, app.x + app.w - 18, app.y + 4, "X", self.w, 255, 255, 255);
            self.fb.fill_rect(app.x + app.w - 10, app.y + app.h - 10, 10, 10, 60, 70, 85);
            render_app_content(&mut self.fb, app, self.w, self.h);
        }

        // ═══════════════════════════════════════════════════════
        // LAYER 3: Dock Bar + Cursor do Mouse
        // ═══════════════════════════════════════════════════════
        let dock_y = h.saturating_sub(36);
        self.fb.fill_rect(0, dock_y, w, 36, 20, 25, 35);
        let mut dx = 10;
        for app in &self.apps {
            if !app.visible { continue; }
            let name = match app.id {
                AppId::HermesChat => "Chat", AppId::Settings => "Set",
                AppId::Power => "Pwr", AppId::Ide => "IDE",
                AppId::Camera => "Cam", AppId::AudioViz => "Aud",
                AppId::WasmSkill(_) => "Sk", AppId::None => "",
            };
            self.fb.fill_rect(dx, dock_y + 3, 60, 30, 40, 55, 75);
            draw_text(&mut self.fb, dx + 6, dock_y + 8, name, self.w, 200, 210, 230);
            dx += 66;
        }

        // Cursor do mouse (cruz 7x7)
        let mx = *MOUSE_X.lock();
        let my = *MOUSE_Y.lock();
        self.fb.fill_rect(mx.saturating_sub(3), my, 7, 1, 255, 255, 255);
        self.fb.fill_rect(mx, my.saturating_sub(3), 1, 7, 255, 255, 255);

        self.fb.swap();
    }
}

fn render_app_content(fb: &mut DoubleBuffer, app: &AppWindow, scr_w: usize, _scr_h: usize) {
    let cx = app.x + 4; let cy = app.y + 28;
    match app.id {
        AppId::HermesChat => {
            let lines: Vec<&str> = app.data.lines().collect();
            for (i, line) in lines.iter().enumerate().take(((app.h - 40) / 16).max(1)) { draw_text(fb, cx, cy + i * 16, line, scr_w, 180, 200, 220); }
            draw_text(fb, cx, cy + (app.h - 40) / 16, "> ", scr_w, 0, 255, 100);
        }
        AppId::Settings => {
            let items = ["[1] Theme", "[2] Sound", "[3] Memory: BGE", "[4] Avatar", "[5] Network"];
            for (i, item) in items.iter().enumerate() { draw_text(fb, cx, cy + i * 16, item, scr_w, 180, 200, 220); }
            if app.data.contains("[2]") || app.data.contains("sound") || app.data.contains("som") {
                let vol = crate::audio::settings::AUDIO_VOLUME.load(core::sync::atomic::Ordering::Relaxed);
                let clone = if crate::audio::settings::VOICE_CLONE_ENABLED.load(core::sync::atomic::Ordering::Relaxed) { "ON" } else { "OFF" };
                let voice = crate::audio::settings::CURRENT_VOICE.lock();
                let voice_name = voice.as_deref().unwrap_or("default");
                let sound_items = [
                    &alloc::format!("   Volume: [{}%]  [+]/[-]", vol),
                    &alloc::format!("   Voice: {}", voice_name),
                    &alloc::format!("   Voice Clone: {}", clone),
                    &alloc::format!("   Wake Word: Jarvis"),
                    "   [B]ack to Settings",
                ];
                for (i, item) in sound_items.iter().enumerate() {
                    let color = if i == 0 { (0u8, 255u8, 100u8) } else { (180u8, 200u8, 220u8) };
                    draw_text(fb, cx, cy + (i + 5) * 16, item, scr_w, color.0, color.1, color.2);
                }
            }
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
            draw_text(fb, cx, cy, &alloc::format!("WASM #{}", idx), scr_w, 0, 200, 255);
            let skills = hermes::wasm_rt::init_wasm_runtime();
            let names = skills.list_skills();
            if let Some(&name) = names.get(idx % names.len().max(1)) {
                let mut rt = hermes::wasm_rt::init_wasm_runtime();
                match rt.execute(name) {
                    Ok(val) => { draw_text(fb, cx, cy + 20, &alloc::format!("= {}", val), scr_w, 0, 255, 0); }
                    Err(e) => { draw_text(fb, cx, cy + 20, &alloc::format!("ERR: {}", e), scr_w, 255, 50, 50); }
                }
            } else {
                draw_text(fb, cx, cy + 20, "No WASM skills loaded", scr_w, 180, 180, 180);
            }
        }
        AppId::Camera => {
            // Preview da camera (simulado)
            fb.fill_rect(cx, cy, 200, 150, 30, 35, 40);
            draw_text(fb, cx + 4, cy + 4, "[CAM]", scr_w, 0, 200, 100);
            draw_text(fb, cx + 4, cy + 140, "[F10] Stop capture", scr_w, 200, 100, 100);
            if let Some(desc) = app.data.split('\n').next() {
                draw_text(fb, cx + 60, cy + 70, &desc, scr_w, 180, 200, 200);
            }
        }
        AppId::AudioViz => {
            // Espectroscopio de audio (barras FFT simuladas)
            fb.fill_rect(cx, cy, 200, 120, 20, 25, 30);
            draw_text(fb, cx + 4, cy + 4, "[MIC]", scr_w, 0, 200, 100);
            let tick = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            let bars: [usize; 16] = core::array::from_fn(|i| {
                let v = libm::sinf((tick as f32 * 0.02) + (i as f32 * 2.5)) * 0.5 + 0.5 + (i as f32 * 0.03);
                (v.min(1.0).max(0.1) * 40.0) as usize
            });
            for (i, &h) in bars.iter().enumerate() {
                fb.fill_rect(cx + 8 + i * 12, cy + 90usize.saturating_sub(h), 8, h, 0, 200, 100);
            }
            draw_text(fb, cx + 4, cy + 108, "[F11] Mute", scr_w, 200, 100, 100);
        }
        AppId::WasmSkill(_) | AppId::None => {}
    }
}


