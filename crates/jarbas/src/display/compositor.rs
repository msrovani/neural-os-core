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
use crate::display::decorations;
use crate::display::fb::DoubleBuffer;
use crate::display::soul_mirror::{SoulMirrorRenderer, SoulMirrorState};
use crate::display::workspaces::Workspaces;
use crate::display::window::FloatingWindow;
use crate::display::focus::{FocusStack, FocusPolicy};
use crate::display::dock::Dock;
pub use crate::display::window::AppId;
use crate::display::window::{Window, WindowContent};
use crate::display::theme::{Theme, ThemeMode};
use crate::display::tiling::{TilingNode, Rect, SplitDirection, WindowId};
use crate::display::notifications::NotificationQueue;

pub static COMPOSITOR: Mutex<Option<JarvisDesktop>> = Mutex::new(None);
/// Banner power (confirm / shutting) — set pelo DisplayAgent.
pub static POWER_BANNER: Mutex<Option<&'static str>> = Mutex::new(None);

pub const POWER_BTN_W: usize = 48;
pub const POWER_BTN_H: usize = 28;

/// Diálogo de confirmação de desligamento (modal central).
/// Botões: Cancel (esquerda) e Turn off (direita).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PowerDialogAction {
    None,
    Cancel,
    TurnOff,
}

/// Geometria do diálogo de power (calculada em runtime conforme scr_w/scr_h).
pub fn power_dialog_rect(scr_w: usize, scr_h: usize) -> (usize, usize, usize, usize) {
    let dw = 360;
    let dh = 140;
    let dx = scr_w.saturating_sub(dw) / 2;
    let dy = scr_h.saturating_sub(dh) / 2;
    (dx, dy, dw, dh)
}

/// Retorna a ação correspondente ao clique dentro do diálogo, ou None.
pub fn hit_power_dialog(cx: usize, cy: usize, scr_w: usize, scr_h: usize) -> PowerDialogAction {
    let (dx, dy, dw, dh) = power_dialog_rect(scr_w, scr_h);
    if cx < dx || cx >= dx + dw || cy < dy || cy >= dy + dh {
        return PowerDialogAction::None;
    }
    // Botões na linha inferior: Cancel (esq) e Turn off (dir)
    let btn_y = dy + dh - 40;
    let btn_h = 28;
    if cy < btn_y || cy >= btn_y + btn_h {
        return PowerDialogAction::None;
    }
    let btn_w = 120;
    let gap = 20;
    let total_w = btn_w * 2 + gap;
    let start_x = dx + (dw.saturating_sub(total_w)) / 2;
    let cancel_x = start_x;
    let off_x = start_x + btn_w + gap;
    if cx >= cancel_x && cx < cancel_x + btn_w {
        PowerDialogAction::Cancel
    } else if cx >= off_x && cx < off_x + btn_w {
        PowerDialogAction::TurnOff
    } else {
        PowerDialogAction::None
    }
}

pub fn power_btn_rect(scr_w: usize) -> (usize, usize, usize, usize) {
    let x = scr_w.saturating_sub(POWER_BTN_W + 8);
    (x, 4, POWER_BTN_W, POWER_BTN_H)
}

pub fn hit_power_button(cx: usize, cy: usize, scr_w: usize) -> bool {
    let (bx, by, bw, bh) = power_btn_rect(scr_w);
    cx >= bx && cx < bx + bw && cy >= by && cy < by + bh
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer { OrbBackground, HermesOverlay, AppWindows, DockBar }



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
    // Compositor base
    pub fb: DoubleBuffer,
    pub layers: [Layer; 4], // mantém Z-order fixo para background/overlay

    // WM novo (FASE 1.1)
    pub workspaces: Workspaces,
    pub focus_stack: FocusStack,
    pub dock: Dock,
    pub tiling_enabled: bool,
    pub theme_mode: crate::display::theme::ThemeMode,

    // Janelas (unificadas)
    pub windows: Vec<Window>,
    pub next_window_id: u64,

    // WASM skills (icons)
    pub wasm_skills: Vec<WasmIcon>,
    pub active: AppId,
    pub avatar_visible: bool,
    pub w: usize, pub h: usize, pub tick: u64,
    icon_cache: BTreeMap<String, [u8; 64]>,
    // ADR-0065 FASE 1.1: card interaction state (temporary between click/eventbus)
    pub card_hit_button: Option<(u32, usize)>,
    dragging_card_id: Option<WindowId>,
    card_drag_off: (i32, i32),
    resizing_card_id: Option<WindowId>,
    // Onda 7: Soul Mirror — visual afetivo do BEI.
    pub soul_mirror: SoulMirrorRenderer,
    // Diálogo de confirmação de desligamento (None = fechado).
    pub power_dialog: bool,

    // Drag/resize state
    pub drag_state: Option<DragState>,
    // Input
    pub mouse_x: i32,
    pub mouse_y: i32,
    pub mouse_buttons: u8,
    // Notifications
    pub notifications: NotificationQueue,
}

#[derive(Debug, Clone)]
pub enum DragState {
    Move { window_id: WindowId, offset_x: i32, offset_y: i32 },
    Resize { window_id: WindowId, area: crate::display::window::HitArea },
}

impl JarvisDesktop {
    pub fn new(fb: DoubleBuffer) -> Self {
        let w = fb.info.width; let h = fb.info.height;
        let mut dock = Dock::new(w as u32, h as u32);
        // Launchers fixos
        dock.add_launcher(AppId::HermesChat, "Chat");
        dock.add_launcher(AppId::Settings, "Settings");
        dock.add_launcher(AppId::Power, "Power");
        dock.add_launcher(AppId::Ide, "IDE");
        dock.add_launcher(AppId::Camera, "Camera");
        dock.add_launcher(AppId::AudioViz, "Audio");

        JarvisDesktop { 
            fb, 
            layers: [Layer::OrbBackground, Layer::HermesOverlay, Layer::AppWindows, Layer::DockBar],
            workspaces: Workspaces::default(),
            focus_stack: FocusStack::new(FocusPolicy::FollowsMouse),
            dock,
            tiling_enabled: true,
            theme_mode: crate::display::theme::ThemeMode::Dark,
            windows: Vec::new(),
            next_window_id: 1,
            wasm_skills: Vec::new(), 
            active: AppId::None, 
            avatar_visible: true, 
            w, h, tick: 0, 
            icon_cache: BTreeMap::new(), 
            card_hit_button: None,
            dragging_card_id: None,
            card_drag_off: (0, 0),
            resizing_card_id: None,
            soul_mirror: SoulMirrorRenderer::new(w, h), 
            power_dialog: false,
            drag_state: None,
            mouse_x: 0,
            mouse_y: 0,
            mouse_buttons: 0,
            notifications: NotificationQueue::new(),
        }
    }

    /// Spawn a new window (legacy or card or tiled)
    pub fn spawn_window(&mut self, content: WindowContent, title: &str, floating: bool) -> WindowId {
        let id = WindowId(self.next_window_id);
        self.next_window_id += 1;

        let active_ws = self.workspaces.active;
        let ws = self.workspaces.active_mut();
        let rect = if floating {
            Rect {
                x: (self.w as i32 - 800) / 2,
                y: (self.h as i32 - 600) / 2,
                width: 800,
                height: 600,
            }
        } else {
            Rect { x: 0, y: 0, width: 100, height: 100 }
        };

        let window = Window {
            id,
            app_id: match &content {
                WindowContent::App(a) => Some(*a),
                _ => None,
            },
            content,
            rect,
            workspace: active_ws,
            focused: true,
            decorated: true,
            floating,
            minimized: false,
            maximized: false,
            title: alloc::string::String::from(title),
            visible: true,
            data: alloc::string::String::new(),
            z: Layer::AppWindows,
        };

        let dock_app = match &window.content {
            WindowContent::App(app) => *app,
            _ => AppId::None,
        };

        if floating {
            ws.add_window_floating(crate::display::workspaces::FloatingWindow::new(id, rect, window.content.clone()));
        } else {
            ws.add_window_tiled(id, SplitDirection::Right);
        }

        self.focus_stack.focus(id);
        self.dock.set_running(dock_app, true, 1);
        self.windows.push(window);

        id
    }

    pub fn close_tiled_window(&mut self, id: WindowId) {
        self.workspaces.active_mut().remove_window(id);
        self.focus_stack.unfocus(id);
        self.windows.retain(|w| w.id != id);
        // Re-layout tiling
        self.relayout_active_workspace();
    }

    pub fn close_app_window(&mut self, id: AppId) {
        if let Some(win) = self.windows.iter_mut().find(|w| w.app_id == Some(id)) {
            win.visible = false;
        }
    }

    pub fn relayout_active_workspace(&mut self) {
        if !self.tiling_enabled { return; }
        let ws = self.workspaces.active();
        let screen_rect = Rect {
            x: 0,
            y: 0,
            width: self.w as u32,
            height: self.h as u32 - self.dock.height,
        };
        let layouts = ws.layout_tiled(screen_rect);
        for (wid, rect) in layouts {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == wid) {
                win.rect = rect;
            }
        }
    }

    pub fn bring_to_front(&mut self, id: WindowId) {
        if let Some(idx) = self.windows.iter().position(|w| w.id == id) {
            let win = self.windows.remove(idx);
            self.windows.push(win);
        }
    }

    pub fn render(&mut self, tick: u64, avatar: Option<&mut crate::display::avatar::JarvisAvatar>) {
        self.tick = tick; let (w, h) = (self.w, self.h);

        // ── FPS control: skip frame se estiver muito rapido ──
        let last = LAST_FRAME_TICK.load(core::sync::atomic::Ordering::Relaxed);
        if tick.wrapping_sub(last) < TARGET_FRAME_TICKS { return; }
        LAST_FRAME_TICK.store(tick, core::sync::atomic::Ordering::Relaxed);

        let theme = crate::display::theme::current_theme();

        // ═══════════════════════════════════════════════════════
        // LAYER 0: Fundo + Orb circular (resolução nativa — sem texto)
        // ═══════════════════════════════════════════════════════
        self.fb.fill_rect(0, 0, w, h, theme.bg.0, theme.bg.1, theme.bg.2);

        if self.avatar_visible {
            self.draw_orb_layer(tick, w, h);
            if let Some(av) = avatar {
                // Partículas/estado por cima do orb (não antes do clear)
                av.render_particles(&mut self.fb);
            }
        }

        // Barra de relógios CPU/MEM/GPU/HD (métricas honestas)
        crate::display::gauges::draw_status_gauges(&mut self.fb, w);

        // ═══════════════════════════════════════════════════════
        // LAYER 1: Hermes CLI Overlay (semi-transparente, canto)
        // ═══════════════════════════════════════════════════════
        let overlay_w = w * 2 / 5;
        let overlay_h = h / 3;
        let ox = w.saturating_sub(overlay_w + 10);
        let oy = 35;
        // Fundo semi-transparente (alpha simulado com cor escura)
        self.fb.fill_rect(ox, oy, overlay_w, overlay_h, theme.bg_alt.0, theme.bg_alt.1, theme.bg_alt.2);
        // Borda sutil
        self.fb.fill_rect(ox.saturating_sub(1), oy.saturating_sub(1), overlay_w + 2, overlay_h + 2, theme.border.0, theme.border.1, theme.border.2);
        // Label Hermes
        draw_text(&mut self.fb, ox + 4, oy + 2, "[Hermes Console]", self.w, theme.fg.0, theme.fg.1, theme.fg.2);
        // Conteudo do overlay: ultimas linhas do chat
        let cli_content = crate::display::console::get_overlay_text();
        let lines: Vec<&str> = cli_content.lines().collect();
        let start = lines.len().saturating_sub(((overlay_h - 24) / 16).max(1));
        for (i, line) in lines.iter().enumerate().skip(start).take((overlay_h - 24) / 16) {
            draw_text(&mut self.fb, ox + 4, oy + 22 + (i - start) * 16, line, self.w, theme.fg_muted.0, theme.fg_muted.1, theme.fg_muted.2);
        }

        // ═══════════════════════════════════════════════════════
        // LAYER 2: Workspace tiling + floating windows
        // ═══════════════════════════════════════════════════════
        // Collect window layout updates in a local vec to avoid borrow conflict
        // between ws (immutable borrow of self.workspaces) and render_window (mutable self).
        let window_updates = {
            let ws = self.workspaces.active();
            let screen_rect = Rect { x: 0, y: 0, width: w as u32, height: h as u32 - self.dock.height };
            let mut updates: Vec<(WindowId, Rect)> = Vec::new();
            if self.tiling_enabled {
                updates.extend(ws.layout_tiled(screen_rect));
            }
            for fw in &ws.floating_windows {
                updates.push((fw.window_id, fw.rect));
            }
            updates
        };
        for (wid, rect) in window_updates {
            if let Some(idx) = self.windows.iter().position(|w| w.id == wid) {
                self.windows[idx].rect = rect;
                draw_window_fb(&mut self.fb, &self.windows[idx], theme, self.w);
            }
        }

        // ═══════════════════════════════════════════════════════
        // LAYER 2b: Cards declarativos (ADR-0058) — por cima das janelas
        // ═══════════════════════════════════════════════════════
        for win in &self.windows {
            if let WindowContent::Card(ref decl) = win.content {
                let _hits = crate::display::card::render_card(&mut self.fb, decl);
                // Handle de resize (canto inf-dir).
                let hx = (decl.x + decl.w - 10).max(0) as usize;
                let hy = (decl.y + decl.h - 10).max(0) as usize;
                self.fb.fill_rect(hx, hy, 10, 10, 0, 120, 180);
            }
        }

        // ═══════════════════════════════════════════════════════
        // LAYER 3: Dock Bar + Notifications + Cursor do Mouse
        // ═══════════════════════════════════════════════════════
        self.dock.render(&mut self.fb, theme);

        // Notifications (top-right)
        self.notifications.render(&mut self.fb, theme, Rect { x: 0, y: 0, width: w as u32, height: h as u32 }, self.tick);

        // Botão OFF — canto superior direito (sempre visível)
        {
            let (bx, by, bw, bh) = power_btn_rect(w);
            self.fb.fill_rect(bx, by, bw, bh, theme.error.0, theme.error.1, theme.error.2);
            draw_text(&mut self.fb, bx + 10, by + 8, "OFF", self.w, theme.fg.0, theme.fg.1, theme.fg.2);
        }
if let Some(banner) = *POWER_BANNER.lock() {
            let bx = w.saturating_sub(280) / 2;
            self.fb.fill_rect(bx, 40, 280, 28, theme.bg_alt.0, theme.bg_alt.1, theme.bg_alt.2);
            draw_text(&mut self.fb, bx + 8, 48, banner, self.w, theme.warning.0, theme.warning.1, theme.warning.2);
        }

        // Diálogo de confirmação de desligamento (modal central)
        if self.power_dialog {
            let (dx, dy, dw, dh) = power_dialog_rect(self.w, self.h);
            // Sombra
            self.fb.fill_rect(dx + 6, dy + 6, dw, dh, 0, 0, 0);
            // Fundo do diálogo
            self.fb.fill_rect(dx, dy, dw, dh, theme.bg_alt.0, theme.bg_alt.1, theme.bg_alt.2);
            // Borda
            self.fb.fill_rect(dx, dy, dw, 2, theme.accent.0, theme.accent.1, theme.accent.2);
            self.fb.fill_rect(dx, dy + dh - 2, dw, 2, theme.accent.0, theme.accent.1, theme.accent.2);
            self.fb.fill_rect(dx, dy, 2, dh, theme.accent.0, theme.accent.1, theme.accent.2);
            self.fb.fill_rect(dx + dw - 2, dy, 2, dh, theme.accent.0, theme.accent.1, theme.accent.2);
            // Título
            draw_text(&mut self.fb, dx + 16, dy + 12, "Desligamento solicitado", self.w, theme.fg.0, theme.fg.1, theme.fg.2);
            // Mensagem
            draw_text(&mut self.fb, dx + 16, dy + 44, "O sistema sera desligado.", self.w, theme.fg_muted.0, theme.fg_muted.1, theme.fg_muted.2);
            draw_text(&mut self.fb, dx + 16, dy + 60, "Salve seu trabalho antes.", self.w, theme.fg_muted.0, theme.fg_muted.1, theme.fg_muted.2);
            // Botões
            let btn_y = dy + dh - 40;
            let btn_h = 28;
            let btn_w = 120;
            let gap = 20;
            let total_w = btn_w * 2 + gap;
            let start_x = dx + (dw.saturating_sub(total_w)) / 2;
            // Cancel (cinza)
            self.fb.fill_rect(start_x, btn_y, btn_w, btn_h, theme.border.0, theme.border.1, theme.border.2);
            draw_text(&mut self.fb, start_x + 30, btn_y + 8, "Cancel", self.w, theme.fg.0, theme.fg.1, theme.fg.2);
            // Turn off (vermelho)
            let off_x = start_x + btn_w + gap;
            self.fb.fill_rect(off_x, btn_y, btn_w, btn_h, theme.error.0, theme.error.1, theme.error.2);
            draw_text(&mut self.fb, off_x + 22, btn_y + 8, "Turn off", self.w, theme.fg.0, theme.fg.1, theme.fg.2);
        }

        // Virtual console overlay (Ctrl+Alt+F1-F6)
        let vcon_active = crate::vconsole::active();
        if vcon_active != 0 {
            let lines = crate::vconsole::get_active_visible();
            let ch = 16usize;
            let cw = 8usize;
            let start_y = 0;
            for (i, line) in lines.iter().enumerate() {
                let y = start_y + i * ch;
                if y + ch > self.h { break; }
                // Background
                self.fb.fill_rect(0, y, self.w, ch, line.bg.0, line.bg.1, line.bg.2);
                // Text
                draw_text(&mut self.fb, 0, y, &line.text, self.w, line.fg.0, line.fg.1, line.fg.2);
            }
            // Show console indicator at top-right
            draw_text(&mut self.fb, self.w.saturating_sub(60), 0, &alloc::format!("F{}", vcon_active + 1), self.w, 255, 255, 100);
        }

        // Cursor do mouse — seta visível (não só cruz 1px)
        let mx = *MOUSE_X.lock();
        let my = *MOUSE_Y.lock();
        draw_mouse_cursor(&mut self.fb, mx, my, self.w, self.h);

        self.fb.swap();
    }

    // ── Métodos cosméticos do WM (ADR-0065) ────────────────────────────

    pub fn register_app(&mut self, app_id: AppId, title: &str, layer: Layer) {
        if self.windows.iter().any(|w| w.app_id == Some(app_id)) { return; }
        let next_id = self.next_window_id;
        self.next_window_id += 1;
        self.windows.push(Window {
            id: WindowId(next_id),
            app_id: Some(app_id),
            content: WindowContent::App(app_id),
            rect: Rect { x: 0, y: 28, width: 400, height: 300 },
            workspace: self.workspaces.active,
            focused: false,
            decorated: true,
            floating: false,
            minimized: false,
            maximized: false,
            title: alloc::string::String::from(title),
            visible: false,
            data: alloc::string::String::new(),
            z: layer,
        });
    }

    pub fn ensure_hermes_overlay(&mut self) {
        if !self.windows.iter().any(|w| w.app_id == Some(AppId::HermesChat)) {
            self.register_app(AppId::HermesChat, "Hermes Chat", Layer::HermesOverlay);
        }
        if let Some(chat) = self.windows.iter_mut().find(|w| w.app_id == Some(AppId::HermesChat)) {
            chat.visible = true;
        }
    }

    pub fn render_window(&mut self, win: &Window, theme: &Theme) {
        draw_window_fb(&mut self.fb, win, theme, self.w);
    }

    pub fn open_power_dialog(&mut self) { self.power_dialog = true; }
    pub fn close_power_dialog(&mut self) { self.power_dialog = false; }

    pub fn spawn_card(&mut self, decl: crate::display::card::UiDeclaration) {
        let id = WindowId(self.next_window_id);
        self.next_window_id += 1;
        let title = decl.title.clone();
        let rect = Rect { x: decl.x, y: decl.y, width: decl.w as u32, height: decl.h as u32 };
        self.windows.push(Window {
            id,
            app_id: None,
            content: WindowContent::Card(decl),
            rect,
            workspace: self.workspaces.active,
            focused: false,
            decorated: false,
            floating: true,
            minimized: false,
            maximized: false,
            title,
            visible: true,
            data: alloc::string::String::new(),
            z: Layer::AppWindows,
        });
    }

    pub fn card_click(&mut self, cx: i32, cy: i32) -> &'static str {
        self.card_hit_button = None;
        for i in (0..self.windows.len()).rev() {
            let content = self.windows[i].content.clone();
            if let WindowContent::Card(ref decl) = content {
                if cx >= decl.x && cx < decl.x + decl.w && cy >= decl.y && cy < decl.y + decl.h {
                    // Close button
                    let (crx, cry, crw, crh) = decl.close_rect();
                    if decl.closable && cx >= crx && cx < crx + crw && cy >= cry && cy < cry + crh {
                        self.windows.remove(i);
                        return "close";
                    }
                    // Resize handle (bottom-right corner)
                    let hx = decl.x + decl.w - 10;
                    let hy = decl.y + decl.h - 10;
                    if cx >= hx && cx < hx + 10 && cy >= hy && cy < hy + 10 {
                        self.resizing_card_id = Some(self.windows[i].id);
                        return "resize";
                    }
                    // Title bar drag
                    if cy < decl.y + 22 {
                        self.dragging_card_id = Some(self.windows[i].id);
                        self.card_drag_off = (cx - decl.x, cy - decl.y);
                        return "drag";
                    }
                    // Button check — usa hit_test_buttons puro (sem side-effect de render).
                    let hits = crate::display::card::hit_test_buttons(decl);
                    for btn in &hits {
                        if cx >= btn.x && cx < btn.x + btn.w && cy >= btn.y && cy < btn.y + btn.h {
                            self.card_hit_button = Some((decl.id, btn.index));
                            return "btn";
                        }
                    }
                    return "focus";
                }
            }
        }
        "miss"
    }

    pub fn card_drag_step(&mut self, mx: i32, my: i32, btn_down: bool) {
        if !btn_down { self.dragging_card_id = None; return; }
        if let Some(card_id) = self.dragging_card_id {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == card_id) {
                if let WindowContent::Card(ref mut decl) = win.content {
                    decl.x = mx - self.card_drag_off.0;
                    decl.y = my - self.card_drag_off.1;
                    win.rect.x = decl.x;
                    win.rect.y = decl.y;
                }
            }
        }
    }

    pub fn card_resize_step(&mut self, mx: i32, my: i32, btn_down: bool) {
        if !btn_down { self.resizing_card_id = None; return; }
        if let Some(card_id) = self.resizing_card_id {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == card_id) {
                if let WindowContent::Card(ref mut decl) = win.content {
                    decl.w = (mx - decl.x).max(100);
                    decl.h = (my - decl.y).max(60);
                    win.rect.width = decl.w.max(0) as u32;
                    win.rect.height = decl.h.max(0) as u32;
                }
            }
        }
    }

    pub fn toggle_app(&mut self, app_id: AppId) {
        // Count visible windows first (immutable), then mutate via index.
        let cnt = self.windows.iter().filter(|w| w.visible).count();
        let idx = self.windows.iter().position(|w| w.app_id == Some(app_id));
        if let Some(idx) = idx {
            self.windows[idx].visible = !self.windows[idx].visible;
            if self.windows[idx].visible {
                self.windows[idx].rect.x = (40 + (cnt % 5) * 20) as i32;
                self.windows[idx].rect.y = (60 + (cnt % 4) * 20) as i32;
            }
        }
        if app_id != AppId::HermesChat {
            self.ensure_hermes_overlay();
        }
    }

    // ── WM actions (ADR-0065 FASE 1.2 — implementação real) ───────────────

    /// Cycle focus: próximo WindowId no MRU stack. Se nada focado, foca o primeiro visível.
    pub fn cycle_focus(&mut self, reverse: bool) {
        let next = if reverse {
            self.focus_stack.cycle_prev()
        } else {
            self.focus_stack.cycle_next()
        };
        let target = next.or_else(|| {
            self.windows.iter()
                .find(|w| w.visible && matches!(w.content, WindowContent::App(_)))
                .map(|w| w.id)
        });
        if let Some(id) = target {
            // Desfoca todos, foca o target
            for w in &mut self.windows { w.focused = false; }
            if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
                w.focused = true;
            }
            self.focus_stack.focus(id);
            self.bring_to_front(id);
        }
    }

    /// Fecha a janela focada (ou a última focada se nenhuma marcada).
    pub fn close_focused_window(&mut self) {
        let target = self.focus_stack.focused()
            .or_else(|| self.windows.iter().find(|w| w.visible).map(|w| w.id));
        if let Some(id) = target {
            self.close_tiled_window(id);
        }
    }

    /// Maximiza a janela focada (rect = screen minus dock).
    pub fn maximize_focused(&mut self) {
        if let Some(id) = self.focus_stack.focused() {
            if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
                w.maximized = !w.maximized;
                if w.maximized {
                    w.rect = Rect {
                        x: 0,
                        y: 28,
                        width: self.w as u32,
                        height: (self.h as u32).saturating_sub(self.dock.height + 28),
                    };
                }
            }
        }
    }

    /// Minimiza a janela focada (visible=false).
    pub fn minimize_focused(&mut self) {
        if let Some(id) = self.focus_stack.focused() {
            if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
                w.minimized = true;
                w.visible = false;
            }
        }
    }

    /// Toggle dock visibility.
    pub fn toggle_dock(&mut self) {
        self.dock.visible = !self.dock.visible;
    }

    /// Toggle floating da janela focada.
    pub fn toggle_floating_focused(&mut self) {
        if let Some(id) = self.focus_stack.focused() {
            if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
                w.floating = !w.floating;
            }
        }
    }

    /// Split a janela focada no tiling tree (Horizontal ou Vertical).
    pub fn split_focused(&mut self, direction: SplitDirection) {
        if let Some(id) = self.focus_stack.focused() {
            let ws = self.workspaces.active_mut();
            ws.add_window_tiled(id, direction);
            self.relayout_active_workspace();
        }
    }

    /// Resize split da janela focada (delta em % fixed-point: 20 = 2%).
    pub fn resize_split_focused(&mut self, delta: i32) {
        if let Some(id) = self.focus_stack.focused() {
            let ws = self.workspaces.active_mut();
            if let Some(root) = ws.tiling_root.as_mut() {
                root.resize_split(id, delta);
            }
            self.relayout_active_workspace();
        }
    }

    pub fn publish_wasm_skill(&mut self, name: &str, description: &str) {
        let idx = self.wasm_skills.len();
        self.wasm_skills.push(WasmIcon {
            name: alloc::string::String::from(name),
            description: alloc::string::String::from(description),
            idx,
        });
    }

    /// Soul Mirror — orb afetivo (Onda 7) substitui o orb cyan fixo.
    /// Cor/brilho/pulsação/anéis/rotação vêm do AffectVector + LoopPhase.
    fn draw_orb_layer(&mut self, _tick: u64, _w: usize, _h: usize) {
        // Tenta puxar affect do ExecutiveSupervisor (se hermes já registrou).
        if let Some(sup) = hermes::globals::EXECUTIVE_SUPERVISOR.lock().as_ref() {
            let mirror = SoulMirrorState::from_affect(
                &sup.affect.affect,
                sup.phase.rotation_deg(),
            );
            self.soul_mirror.update_state(mirror);
        }
        let fft_energy = crate::display::avatar::read_audio_energy();
        self.soul_mirror.render(&mut self.fb, fft_energy);
    }
}

/// Free function to render a window frame — takes fb directly to avoid borrow conflicts.
fn draw_window_fb(fb: &mut DoubleBuffer, win: &Window, theme: &Theme, scr_w: usize) {
    let bg_color = if win.focused { theme.bg_alt } else { theme.bg };
    fb.fill_rect(
        win.rect.x as usize, win.rect.y as usize,
        win.rect.width as usize, win.rect.height as usize,
        bg_color.0, bg_color.1, bg_color.2,
    );
    decorations::draw_window_decorations(fb, win, theme, scr_w);
}

fn draw_mouse_cursor(fb: &mut DoubleBuffer, mx: usize, my: usize, scr_w: usize, scr_h: usize) {
    // Flash amarelo no clique (MOUSE_CLICK_FLASH decrementado por frame).
    let flash = k_nano::interrupts::MOUSE_CLICK_FLASH.load(core::sync::atomic::Ordering::Acquire);
    if flash > 0 {
        k_nano::interrupts::MOUSE_CLICK_FLASH.fetch_sub(1, core::sync::atomic::Ordering::AcqRel);
        let r = 10 + (flash as usize);
        let ox = mx.saturating_sub(r / 2);
        let oy = my.saturating_sub(r / 2);
        fb.fill_rect(ox, oy, r.min(scr_w.saturating_sub(ox)), 2, 255, 220, 40);
        fb.fill_rect(ox, oy, 2, r.min(scr_h.saturating_sub(oy)), 255, 220, 40);
        let bx = mx.saturating_add(r / 2).min(scr_w.saturating_sub(1));
        let by = my.saturating_add(r / 2).min(scr_h.saturating_sub(1));
        fb.fill_rect(ox, by.saturating_sub(1), r.min(scr_w.saturating_sub(ox)), 2, 255, 220, 40);
        fb.fill_rect(bx.saturating_sub(1), oy, 2, r.min(scr_h.saturating_sub(oy)), 255, 220, 40);
    }

    // Seta 11×16 (1=preto contorno, 2=branco fill) — legível no orb escuro.
    const ARROW: [[u8; 11]; 16] = [
        [2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        [2, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        [2, 1, 2, 0, 0, 0, 0, 0, 0, 0, 0],
        [2, 1, 1, 2, 0, 0, 0, 0, 0, 0, 0],
        [2, 1, 1, 1, 2, 0, 0, 0, 0, 0, 0],
        [2, 1, 1, 1, 1, 2, 0, 0, 0, 0, 0],
        [2, 1, 1, 1, 1, 1, 2, 0, 0, 0, 0],
        [2, 1, 1, 1, 1, 1, 1, 2, 0, 0, 0],
        [2, 1, 1, 1, 1, 1, 1, 1, 2, 0, 0],
        [2, 1, 1, 1, 1, 1, 1, 1, 1, 2, 0],
        [2, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2],
        [2, 1, 1, 2, 1, 1, 2, 0, 0, 0, 0],
        [2, 1, 2, 0, 2, 1, 1, 2, 0, 0, 0],
        [2, 2, 0, 0, 2, 1, 1, 2, 0, 0, 0],
        [2, 0, 0, 0, 0, 2, 1, 1, 2, 0, 0],
        [0, 0, 0, 0, 0, 2, 2, 2, 0, 0, 0],
    ];
    for (row, line) in ARROW.iter().enumerate() {
        let y = my + row;
        if y >= scr_h {
            break;
        }
        for (col, &pix) in line.iter().enumerate() {
            if pix == 0 {
                continue;
            }
            let x = mx + col;
            if x >= scr_w {
                break;
            }
            if pix == 1 {
                fb.fill_rect(x, y, 1, 1, 0, 0, 0);
            } else {
                fb.fill_rect(x, y, 1, 1, 255, 255, 255);
            }
        }
    }
}

fn render_app_content(fb: &mut DoubleBuffer, win: &Window, scr_w: usize, _scr_h: usize) {
    let cx = win.rect.x as usize + 4; let cy = win.rect.y as usize + 28;
    let h = win.rect.height as usize;
    match win.app_id() {
        AppId::HermesChat => {
            let lines: Vec<&str> = win.data.lines().collect();
            for (i, line) in lines.iter().enumerate().take(((h - 40) / 16).max(1)) { draw_text(fb, cx, cy + i * 16, line, scr_w, 180, 200, 220); }
            draw_text(fb, cx, cy + (h - 40) / 16, "> ", scr_w, 0, 255, 100);
        }
        AppId::Settings => {
            let items = ["[1] Theme", "[2] Sound", "[3] Memory: BGE", "[4] Avatar", "[5] Network"];
            for (i, item) in items.iter().enumerate() { draw_text(fb, cx, cy + i * 16, item, scr_w, 180, 200, 220); }
            if win.data.contains("[2]") || win.data.contains("sound") || win.data.contains("som") {
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
            draw_text(fb, cx, cy + 40, &win.data, scr_w, 200, 200, 200);
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
            if let Some(desc) = win.data.split('\n').next() {
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
        AppId::None => {}
    }
}


