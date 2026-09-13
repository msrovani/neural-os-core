//! JARVIS Desktop — compositor multi-window + app icons + LLM icons.
//! Port da UI do SmileyOS + JARVIS .NET MAUI.

use alloc::vec::Vec;
use alloc::string::String;
use alloc::collections::BTreeMap;
use k_nano::sync::IrqSafeLock;
use k_nano::EVENT_BUS;
use crate::display::decorations;
use crate::display::fb::{cos_q8, sin_q8, DoubleBuffer};
use crate::display::soul_mirror::{OrbSignals, SoulMirrorRenderer};
use crate::display::workspaces::Workspaces;
use crate::display::focus::{FocusStack, FocusPolicy};
use crate::display::dock::Dock;
pub use crate::display::window::AppId;
use crate::display::window::{HitArea, Window, WindowContent};
use crate::display::theme::Theme;
use crate::display::tiling::{Rect, SplitDirection, WindowId};
use crate::display::notifications::NotificationQueue;

// JARVIS palette constants for compositor
const JARVIS_CYAN_R: u8 = 0;
const JARVIS_CYAN_G: u8 = 212;
const JARVIS_CYAN_B: u8 = 255;
/// Software cursor: underlay 24×24 (seta 11×16 + margem). Sem flash enorme.
const CURSOR_UNDER_W: usize = 24;
const CURSOR_UNDER_H: usize = 24;
const CURSOR_UNDER_OX: usize = 2;
const CURSOR_UNDER_OY: usize = 2;
const CURSOR_UNDER_BYTES: usize = CURSOR_UNDER_W * CURSOR_UNDER_H * 4; // bpp≤4


/// Tópico EventBus — botão de card (FeedbackAgent: `"id:idx"`).
pub const TOPIC_CARD_ACTION: &str = "CARD_ACTION";

pub static COMPOSITOR: IrqSafeLock<Option<JarbasDesktop>> = IrqSafeLock::new(None);

/// Modo de foco do Jarbas:
/// - Chat: interação por turnos via ChatWindow (teclado → input buffer)
/// - Ambient: wake-word + auto-listen após silêncio
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusMode {
    Chat,
    Ambient,
}
pub static FOCUS_MODE: IrqSafeLock<FocusMode> = IrqSafeLock::new(FocusMode::Ambient);
/// Banner power (mensagem de estado) — set pelo DisplayAgent.
pub static POWER_BANNER: IrqSafeLock<Option<&'static str>> = IrqSafeLock::new(None);

/// Estado da ação de energia: None (normal), Dialog (confirmação), ou ação em andamento.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerState {
    None,
    Dialog,
    ShuttingDown,
    Hibernating,
    Rebooting,
}
pub static POWER_STATE: IrqSafeLock<PowerState> = IrqSafeLock::new(PowerState::None);

/// Banner de boas-vindas (suit-boot) — texto + tick até quando exibir.
pub static WELCOME_BANNER: IrqSafeLock<Option<String>> = IrqSafeLock::new(None);
static WELCOME_UNTIL: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Cached HUD status line — recomputed only when values change.
/// Elimina alloc de String por frame no render loop (60Hz).
static HUD_CACHE_MEM: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(u64::MAX);
    static HUD_CACHE_STR: spin::Mutex<alloc::string::String> = spin::Mutex::new(alloc::string::String::new());

/// Ticks do DisplayAgent que chegaram ao render (diagnóstico freeze s317):
/// valor pintado no HUD; se a tela congelar, o último frame mostra até onde
/// o render chegou e qual agent estava em tick.
pub static RENDER_N: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Publica a fala de boas-vindas no HUD (~8s a 60Hz ≈ 480 ticks).
pub fn announce_welcome(body: &str) {
    let clean = body.trim();
    if clean.is_empty() {
        return;
    }
    let now = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
    WELCOME_UNTIL.store(now.saturating_add(480), core::sync::atomic::Ordering::Relaxed);
    *WELCOME_BANNER.lock() = Some(String::from(clean));
}

pub const POWER_BTN_W: usize = 48;
pub const POWER_BTN_H: usize = 28;

/// Diálogo de energia (modal central) com 3 opções.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PowerDialogAction {
    None,
    Cancel,
    ShutDown,
    Hibernate,
    Reboot,
}

/// Geometria do diálogo de energia (calculada em runtime conforme scr_w/scr_h).
pub fn power_dialog_rect(scr_w: usize, scr_h: usize) -> (usize, usize, usize, usize) {
    let dw = 260;
    let dh = 150;
    let dx = scr_w.saturating_sub(dw) / 2;
    let dy = scr_h.saturating_sub(dh) / 2;
    (dx, dy, dw, dh)
}

/// Retorna a ação correspondente ao clique dentro do diálogo de energia.
/// Clique fora retorna `None` (usado para cancelar).
pub fn hit_power_dialog(cx: usize, cy: usize, scr_w: usize, scr_h: usize) -> PowerDialogAction {
    let (dx, dy, dw, dh) = power_dialog_rect(scr_w, scr_h);
    if cx < dx || cx >= dx + dw || cy < dy || cy >= dy + dh {
        return PowerDialogAction::None;
    }
    // 3 botões na linha inferior: Desligar | Hibernar | Reiniciar
    let btn_y = dy + dh - 44;
    let btn_h = 28;
    if cy < btn_y || cy >= btn_y + btn_h {
        return PowerDialogAction::None;
    }
    let btn_w = 68;
    let gap = 10;
    let total_w = btn_w * 3 + gap * 2;
    let start_x = dx + (dw.saturating_sub(total_w)) / 2;
    let desligar_x = start_x;
    let hibernar_x = start_x + btn_w + gap;
    let reiniciar_x = start_x + (btn_w + gap) * 2;
    if cx >= desligar_x && cx < desligar_x + btn_w {
        PowerDialogAction::ShutDown
    } else if cx >= hibernar_x && cx < hibernar_x + btn_w {
        PowerDialogAction::Hibernate
    } else if cx >= reiniciar_x && cx < reiniciar_x + btn_w {
        PowerDialogAction::Reboot
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

// ══════════════════════════════════════════════════════════════════════════
// Hub Health — glass plaque ancorado à direita, FORA do WM (nunca tile/focus).
// Geometria spec: x=W-436, y=44, w=420, h=min(H-88,560); tudo × s=min(w,h)/720.
// ══════════════════════════════════════════════════════════════════════════
const HUB_W: f32 = 420.0;
const HUB_H_MAX: f32 = 560.0;
const HUB_X_OFF: f32 = 436.0;
const HUB_Y: f32 = 44.0;
/// Deslocamento do slide-in (px) e duração (µs) — frames intermediários opacos.
const HUB_SLIDE_PX: usize = 24;
const HUB_SLIDE_US: u64 = 220_000;

/// Geometria do painel (escalada por `s = min(w,h)/720`).
pub fn hub_rect(w: usize, h: usize) -> (usize, usize, usize, usize) {
    let s = (core::cmp::min(w, h) as f32) / 720.0;
    let pw = (HUB_W * s) as usize;
    let ph = ((h as f32 - 88.0).min(HUB_H_MAX) * s) as usize;
    let px = w.saturating_sub((HUB_X_OFF * s) as usize);
    let py = ((HUB_Y * s) as usize).min(h.saturating_sub(ph));
    (px, py, pw, ph)
}

/// Região de staging (rect final + overshoot do slide) — underlay do painel.
pub fn hub_staging(w: usize, h: usize) -> (usize, usize, usize, usize) {
    let (px, py, pw, ph) = hub_rect(w, h);
    let sw = (pw + HUB_SLIDE_PX).min(w.saturating_sub(px));
    (px, py, sw, ph)
}

/// Micro badge (4 LEDs USB/MEM/NET/AGENTS) no HUD, perto do brand.
pub fn hub_badge_rect() -> (usize, usize, usize, usize) {
    (64, 8, 4 * 8 - 2, 6)
}

// Paleta do painel (acentos; vidro ~α=216 do bg #0A1220).
const HUB_BG: (u8, u8, u8) = (0x0A, 0x12, 0x20);
const HUB_BORDER: (u8, u8, u8) = (0x0A, 0x6E, 0x96);
const HUB_DIV: (u8, u8, u8) = (0x12, 0x30, 0x48);
const HUB_LABEL: (u8, u8, u8) = (110, 140, 160);
const HUB_VALUE: (u8, u8, u8) = (200, 240, 255);

/// Pill publicado pelo HubHealthAgent (código sev 0..3) → cor/label de render.
fn pill_color(code: u8) -> (u8, u8, u8) {
    match code {
        0 => (0x30, 0xFF, 0x90),
        1 => (0xFF, 0xB0, 0x20),
        2 => (0xFF, 0x4A, 0x5A),
        _ => (0x7A, 0x87, 0x94),
    }
}
fn pill_label(code: u8) -> &'static str {
    match code {
        0 => "OK",
        1 => "WARN",
        2 => "FAIL",
        _ => "n/a",
    }
}

/// Estados dos 4 LEDs do badge: USB, MEM(heap+ram), NET(mesh), AGENTS.
fn hub_badge_states(hh: &crate::display::gauges::HubHealth) -> [crate::display::gauges::HubState; 4] {
    use crate::display::gauges::HubState;
    let mem = hh.rows[7].state.worst(hh.rows[8].state);
    [hh.rows[1].state, mem, hh.rows[12].state, hh.rows[11].state]
}

pub fn draw_text(fb: &mut DoubleBuffer, x: usize, y: usize, text: &str, scr_w: usize, r: u8, g: u8, b: u8) {
    // §3.1 (ADR-0090 Tier 1): blit table — pixel-idêntico ao scale=1, zero set_pixel.
    crate::display::font::draw_text_blit(fb, x, y, text, scr_w, r, g, b);
}

// FASE 4.3: Hover zone detection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoverZone {
    None,
    PowerButton,
    DockItem(usize),
    Orb,
    Chat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer { OrbBackground, HermesOverlay, AppWindows, DockBar }



// Estado global do mouse para o compositor
pub static MOUSE_X: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(640);
pub static MOUSE_Y: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(360);
pub static MOUSE_BUTTONS: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

// Timing de frame para FPS control
// ADR-0104 — cadência alvo da UI (fps) e custo EWMA do trabalho de frame (µs).
pub const TARGET_FPS: u64 = 30;
static FRAME_COST_US: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

/// Gate de paint por RELÓGIO DE PAREDE (s328): um paint a cada `target_period_us()`.
/// Substitui o gate por tick (o `%3` do orb + `/30` do frame davam 15 fps reais).
static LAST_PRESENT_US: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Período-alvo do paint em µs (constante — NÃO derivar da cadência do rail).
pub fn target_period_us() -> u64 {
    1_000_000 / TARGET_FPS
}

/// Relógio da UI: TSC em µs; fallback p/ ticks do PIT quando `tsc_hz()==0`.
fn ui_now_us(tick: u64) -> u64 {
    let now = k_nano::tsc::now_us();
    if now != 0 {
        now
    } else {
        tick.saturating_mul(1_000_000 / k_hal::timer_cap::current_tick_hz().max(1))
    }
}

/// Custo EWMA do trabalho de frame em µs (0 = sem amostra).
pub fn frame_cost_us() -> u32 {
    FRAME_COST_US.load(core::sync::atomic::Ordering::Relaxed)
}

fn record_frame_cost(t0: u64) {
    let t1 = k_nano::tsc::rdtsc();
    let hz = k_nano::tsc::tsc_hz();
    if hz == 0 {
        return;
    }
    let dt = ((t1.wrapping_sub(t0) as u128) * 1_000_000 / hz as u128).min(u32::MAX as u128) as u32;
    let cur = FRAME_COST_US.load(core::sync::atomic::Ordering::Relaxed);
    let next = if cur == 0 { dt } else { cur - cur / 8 + dt / 8 };
    FRAME_COST_US.store(next, core::sync::atomic::Ordering::Relaxed);
    k_hal::timer_cap::note_frame_cost_us(next);
}

pub struct JarbasDesktop {
    // Compositor base
    pub fb: DoubleBuffer,
    pub layers: [Layer; 4], // mantém Z-order fixo para background/overlay

    // ── FASE 2: Per-layer dirty tracking ──
    pub dirty_orb: bool,
    pub dirty_hud: bool,
    pub dirty_windows: bool,
    pub dirty_mesh: bool,
    pub dirty_cursor: bool,
    pub dirty_dialog: bool,
    /// Último tick que chegou a pintar (cap: 1 paint por tick).
    pub last_paint_tick: u64,
    /// Sinais reais do orb (voz/cognição/saúde) — setados pelo DisplayAgent.
    pub orb_signals: OrbSignals,

    // ── FASE 4.3: Hover state ──
    pub hover_zone: HoverZone,
    pub hover_prev: HoverZone,

    // WM novo (FASE 1.1)
    pub workspaces: Workspaces,
    pub focus_stack: FocusStack,
    pub dock: Dock,
    pub tiling_enabled: bool,
    pub theme_mode: crate::display::theme::ThemeMode,

    // Janelas (unificadas)
    pub windows: Vec<Window>,
    pub next_window_id: u64,

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
    /// RGB underlay sob o cursor (anti-rastro no back buffer persistente).
    cursor_under: alloc::vec::Vec<u8>,
    cursor_under_x: usize,
    cursor_under_y: usize,
    cursor_under_valid: bool,
    /// Primeiro frame: swap full; depois só dirty-rects (anti-freeze TCG).
    full_swap_pending: bool,
    last_orb_x0: usize,
    last_orb_y0: usize,
    last_orb_w: usize,
    last_orb_h: usize,
    // Notifications
    pub notifications: NotificationQueue,
    // ── Hub Health (painel F12; fora do WM) ──
    // Política (open/close/pill/auto-close) vive no HubHealthAgent (hermes);
    // o compositor só renderiza o estado publicado (panel_gen/visible).
    dirty_panel: bool,
    /// Gen do estado publicado pelo agente (mudou → repaint do painel).
    hub_gen: u64,
    /// Full repaint do painel no próximo frame (toggle/checksum/1 Hz/janelas).
    hub_full: bool,
    hub_anim_start_us: u64,
    /// Underlay da região de staging (restore antes de recompor o vidro).
    hub_underlay: alloc::vec::Vec<u8>,
    hub_underlay_valid: bool,
    hub_painted_checksum: u64,
    hub_last_paint_us: u64,
    /// Região a apresentar (swap_rect) neste frame.
    hub_swap_rect: Option<(usize, usize, usize, usize)>,
}

#[derive(Debug, Clone, Copy)]
pub enum DragState {
    Move { window_id: WindowId, offset_x: i32, offset_y: i32 },
    Resize { window_id: WindowId, area: crate::display::window::HitArea },
}

impl JarbasDesktop {
    pub fn new(fb: DoubleBuffer) -> Self {
        let w = fb.info.width; let h = fb.info.height;
        let mut dock = Dock::new(w as u32, h as u32);
        // Launchers essenciais (stubs Ide/Camera/Audio removidos do dock)
        if crate::display::chat_window::chat_ui_enabled() {
            dock.add_launcher(AppId::HermesChat, "Chat");
        }
        dock.add_launcher(AppId::Power, "Power");

        JarbasDesktop { 
            fb, 
            layers: [Layer::OrbBackground, Layer::HermesOverlay, Layer::AppWindows, Layer::DockBar],
            dirty_orb: true, dirty_hud: true, dirty_windows: true,
            dirty_mesh: true, dirty_cursor: true, dirty_dialog: true,
            last_paint_tick: u64::MAX, orb_signals: OrbSignals::default(),
            hover_zone: HoverZone::None, hover_prev: HoverZone::None,
            workspaces: Workspaces::default(),
            focus_stack: FocusStack::new(FocusPolicy::FollowsMouse),
            dock,
            tiling_enabled: true,
            theme_mode: crate::display::theme::ThemeMode::Dark,
            windows: Vec::new(),
            next_window_id: 1,
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
            cursor_under: alloc::vec![0u8; CURSOR_UNDER_BYTES],
            cursor_under_x: 0,
            cursor_under_y: 0,
            cursor_under_valid: false,
            full_swap_pending: true,
            last_orb_x0: 0,
            last_orb_y0: 0,
            last_orb_w: 0,
            last_orb_h: 0,
            notifications: NotificationQueue::new(),
            dirty_panel: false,
            hub_gen: 0,
            hub_full: false,
            hub_anim_start_us: 0,
            hub_underlay: alloc::vec::Vec::new(),
            hub_underlay_valid: false,
            hub_painted_checksum: 0,
            hub_last_paint_us: 0,
            hub_swap_rect: None,
        }
    }

    /// Consome o último clique em botão de card (card_id, button_index).
    /// Retorna None se não houve clique. Padrão: ler-e-zerar para evitar re-trigger.
    pub fn take_card_hit_button(&mut self) -> Option<(u32, usize)> {
        self.card_hit_button.take()
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
            anim_scale: 0.0f32,
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
        self.invalidate_windows();

        id
    }

    pub fn close_tiled_window(&mut self, id: WindowId) {
        self.workspaces.active_mut().remove_window(id);
        self.focus_stack.unfocus(id);
        self.windows.retain(|w| w.id != id);
        // Re-layout tiling
        self.relayout_active_workspace();
        self.invalidate_windows();
    }

    pub fn close_app_window(&mut self, id: AppId) {
        if let Some(win) = self.windows.iter_mut().find(|w| w.app_id == Some(id)) {
            win.visible = false;
        }
        self.invalidate_windows();
    }

    pub fn relayout_active_workspace(&mut self) {
        self.invalidate_windows();
        if !self.tiling_enabled { return; }
        let ws = self.workspaces.active();
        let screen_rect = Rect {
            x: 0,
            y: 0,
            width: self.w as u32,
            height: self.h as u32 - if self.dock.visible { self.dock.height } else { 0 },
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
            self.invalidate_windows();
        }
    }


    // ── FASE 4.3: Hit-test hover ──────────────────────────────────────

    // ── FASE 2: Per-layer invalidation ─────────────────────────────────
    pub fn invalidate_orb(&mut self) { self.dirty_orb = true; }
    pub fn invalidate_hud(&mut self) { self.dirty_hud = true; }
    pub fn invalidate_windows(&mut self) { self.dirty_windows = true; }
    pub fn invalidate_mesh(&mut self) { self.dirty_mesh = true; }
    pub fn invalidate_cursor(&mut self) { self.dirty_cursor = true; }
    pub fn invalidate_dialog(&mut self) { self.dirty_dialog = true; }
    pub fn invalidate_panel(&mut self) { self.dirty_panel = true; self.hub_full = true; }
    pub fn invalidate_all(&mut self) {
        self.dirty_orb = true; self.dirty_hud = true; self.dirty_windows = true;
        self.dirty_mesh = true; self.dirty_cursor = true; self.dirty_dialog = true;
        self.invalidate_panel();
    }

    // ── Hub Health: política NO AGENTE (hermes::hub_health) ────────────
    // O compositor só encaminha comandos e sincroniza o estado publicado.

    pub fn hub_is_open(&self) -> bool { hermes::hub_health::panel_visible() }

    pub fn hub_badge_hit(&self, cx: usize, cy: usize) -> bool {
        let (x, y, w, h) = hub_badge_rect();
        cx >= x && cx < x + w && cy >= y && cy < y + h
    }

    /// Clique no orb = toggle do painel (orb não é janela — miss canônico).
    pub fn orb_hit(&self, mx: i32, my: i32) -> bool {
        let dx = (mx - self.soul_mirror.cx as i32).abs();
        let dy = (my - self.soul_mirror.cy as i32).abs();
        let r = (self.soul_mirror.base_r * 1.25) as i32;
        dx * dx + dy * dy <= r * r
    }

    fn publish_hub_cmd(cmd: &str) {
        let _ = EVENT_BUS.publish(event_bus::Event {
            id: 0,
            topic: alloc::string::String::from(hermes::hub_health::TOPIC_HUB_HEALTH_CMD),
            payload: cmd.as_bytes().to_vec(),
            token: event_bus::CapabilityToken::Legacy(1),
        });
    }

    pub fn toggle_hub_health(&mut self, _now_us: u64) {
        Self::publish_hub_cmd("toggle");
    }

    /// `auto_close_us` é ignorado aqui — o auto-close 8s é política do agente
    /// (HEALTH_ISSUE). Chamada mantida p/ compat dos callers.
    pub fn open_hub_health(&mut self, _now_us: u64, _auto_close_us: u64) {
        Self::publish_hub_cmd("open");
    }

    pub fn close_hub_health(&mut self) {
        Self::publish_hub_cmd("close");
    }

    /// Sincroniza com o estado publicado pelo HubHealthAgent (por render, barato):
    /// gen mudou → repaint; visível subiu → slide-in; caiu → prepara restore.
    fn sync_hub_policy(&mut self, now_us: u64) {
        let gen = hermes::hub_health::panel_gen();
        if gen == self.hub_gen {
            return;
        }
        self.hub_gen = gen;
        let visible = hermes::hub_health::panel_visible();
        if visible {
            if self.hub_anim_start_us == 0 {
                self.hub_anim_start_us = now_us.max(1);
            }
            self.hub_underlay_valid = false;
            self.hub_painted_checksum = 0;
            k_nano::slog_jarbas!("HUB", "info", "painel aberto (agente)");
        } else {
            self.hub_anim_start_us = 0;
            self.hub_painted_checksum = 0;
            k_nano::slog_jarbas!("HUB", "info", "painel fechado (agente)");
        }
        self.hub_full = true;
        self.dirty_panel = true;
    }

    pub fn hit_test_hover(&mut self, mx: usize, my: usize) -> HoverZone {
        let (w, h) = (self.w, self.h);
        let (bx, by, bw, bh) = power_btn_rect(w);
        if mx >= bx && mx < bx + bw && my >= by && my < by + bh {
            let zone = HoverZone::PowerButton;
            if zone != self.hover_zone {
                self.hover_prev = self.hover_zone;
                self.hover_zone = zone;
                self.dirty_hud = true;
                self.dirty_cursor = true;
            }
            return self.hover_zone;
        }
        if self.dock.visible {
            let dh = self.dock.height as usize;
            if my >= h.saturating_sub(dh) {
                let zone = HoverZone::DockItem(0);
                if zone != self.hover_zone {
                    self.hover_prev = self.hover_zone;
                    self.hover_zone = zone;
                    self.dirty_hud = true;
                    self.dirty_cursor = true;
                }
                return self.hover_zone;
            }
        }
        let cx = w / 2;
        let cy = h / 2;
        let dx = (mx as isize - cx as isize).unsigned_abs();
        let dy = (my as isize - cy as isize).unsigned_abs();
        let dist_sq = dx * dx + dy * dy;
        if dist_sq < 264 * 264 {
            let zone = HoverZone::Orb;
            if zone != self.hover_zone {
                self.hover_prev = self.hover_zone;
                self.hover_zone = zone;
                self.dirty_orb = true;
                self.dirty_cursor = true;
            }
            return self.hover_zone;
        }
        let left_w = w * 35 / 100;
        if mx < left_w {
            let zone = HoverZone::Chat;
            if zone != self.hover_zone {
                self.hover_prev = self.hover_zone;
                self.hover_zone = zone;
                self.dirty_cursor = true;
            }
            return self.hover_zone;
        }
        if self.hover_zone != HoverZone::None {
            self.hover_prev = self.hover_zone;
            self.hover_zone = HoverZone::None;
            self.dirty_cursor = true;
        }
        self.hover_zone
    }

    // ── FASE 4.4: Voice waveform removido (s328) — sem callers; a
    // visualização de voz do orb é o próprio SoulMirror (ripples/ticks).

    /// ADR-0104: wrapper que mede o custo do frame (EWMA µs) para o
    /// `timer_cap::recommend` — o gate de FPS lê a cadência atual.
    pub fn render(&mut self, tick: u64) {
        let t0 = k_nano::tsc::rdtsc();
        self.render_inner(tick);
        record_frame_cost(t0);
    }

    /// Sinais reais do orb (drenados pelo DisplayAgent a cada tick).
    pub fn set_orb_signals(&mut self, sig: OrbSignals) {
        self.orb_signals = sig;
    }

    fn render_inner(&mut self, tick: u64) {
        self.tick = tick; let (w, h) = (self.w, self.h);
        RENDER_N.fetch_add(1, core::sync::atomic::Ordering::Relaxed);

        // ── Gate de paint por relógio de parede (s328) ──
        // Um paint a cada target_period_us(); cap de 1 paint por tick. O relógio
        // do TSC é independente da cadência do rail (PIT/LAPIC adaptativo).
        let now = ui_now_us(tick);
        if now.wrapping_sub(LAST_PRESENT_US.load(core::sync::atomic::Ordering::Relaxed))
            < target_period_us()
        {
            return;
        }
        if self.last_paint_tick == tick {
            return;
        }
        self.last_paint_tick = tick;
        LAST_PRESENT_US.store(now, core::sync::atomic::Ordering::Relaxed);

        // Orb anima a cada paint (time-driven dentro do SoulMirror).
        self.dirty_orb = true;
        // HUD + dock clock: ~1s @18Hz (PIT) / ~0.5s @64Hz. Sem dirty_hud o
        // present_frame não apresenta o dock e o relógio fica 00:00.
        if tick % 16 == 0 {
            self.dirty_hud = true;
        }

        // Hub Health: política do agente → estado local (gen/visible).
        self.sync_hub_policy(now);

        // Heartbeat 1 Hz + checksum do fill 2 Hz (badge idem).
        if hermes::hub_health::panel_visible() {
            let hh = crate::display::gauges::hub_health();
            if hh.checksum != self.hub_painted_checksum {
                self.hub_full = true;
            }
            if now.saturating_sub(self.hub_last_paint_us) >= 1_000_000 {
                self.hub_full = true;
            }
            self.dirty_panel = true;
        }

        // ── FASE 2: early-exit ──
        if !self.dirty_orb && !self.dirty_hud && !self.dirty_windows
            && !self.dirty_mesh && !self.dirty_cursor && !self.dirty_dialog
            && !self.dirty_panel
        {
            return;
        }

        // SESSION_315: orb-only NÃO repinta HUD/dock/janelas (era o gap de CPU).
        let paint_hud = self.dirty_hud || self.dirty_dialog;
        let paint_windows = self.dirty_windows;
        let paint_dock = self.dirty_hud || self.dirty_windows || self.dirty_dialog;
        // Painel de vidro: janelas/HUD/banner por cima exigem recomposição full.
        if hermes::hub_health::panel_visible() && (paint_hud || paint_windows || self.dirty_dialog) {
            self.hub_full = true;
        }

        // Só cursor: restore+draw+dirty-rect — evita HUD/orb e swap full (TCG).
        if self.dirty_cursor
            && !self.dirty_orb
            && !self.dirty_hud
            && !self.dirty_windows
            && !self.dirty_mesh
            && !self.dirty_dialog
            && !self.dirty_panel
        {
            let prev_x = self.cursor_under_x;
            let prev_y = self.cursor_under_y;
            let had = self.cursor_under_valid;
            self.restore_cursor_underlay();
            let mx = MOUSE_X.load(core::sync::atomic::Ordering::Relaxed);
            let my = MOUSE_Y.load(core::sync::atomic::Ordering::Relaxed);
            self.save_cursor_underlay(mx, my);
            draw_mouse_cursor(&mut self.fb, mx, my, w, h);
            self.dirty_cursor = false;
            if had {
                self.fb.swap_rect(prev_x, prev_y, CURSOR_UNDER_W, CURSOR_UNDER_H);
            }
            self.fb.swap_rect(
                self.cursor_under_x,
                self.cursor_under_y,
                CURSOR_UNDER_W,
                CURSOR_UNDER_H,
            );
            self.fb.dirty = false;
            return;
        }

        let theme = crate::display::theme::current_theme();
        let mode = *FOCUS_MODE.lock();

        // ═════════════════════════════════════════════════════════════
        // ESTADO DE ENERGIA: tela preta + mensagem centralizada
        // ═════════════════════════════════════════════════════════════
        {
            let pstate = POWER_STATE.lock();
            if *pstate == PowerState::ShuttingDown
                || *pstate == PowerState::Hibernating
                || *pstate == PowerState::Rebooting
            {
                drop(pstate);
                self.fb.fill_rect(0, 0, w, h, 0, 0, 0);
                let msg = if let Some(banner) = *POWER_BANNER.lock() { banner } else { "" };
                if !msg.is_empty() {
                    let tw = msg.len() * 8; // approx pixel width
                    let tx = w.saturating_sub(tw) / 2;
                    draw_text(&mut self.fb, tx, h / 2 - 8, msg, w, 255, 200, 80);
                }
                draw_mouse_cursor(&mut self.fb, MOUSE_X.load(core::sync::atomic::Ordering::Relaxed), MOUSE_Y.load(core::sync::atomic::Ordering::Relaxed), w, h);
                self.fb.swap();
                return;
            }
        }

        // ═════════════════════════════════════════════════════════════
        // CAMADA 0: Fundo escuro + Orb responsivo (tela inteira)
        // Só redesenha fundo+orb+mesh quando dirty_orb ou dirty_mesh.
        // Quando só cursor muda, pula 1M+ pixels de fill.
        // ═════════════════════════════════════════════════════════════
        // Anti-rastro: restaura underlay ANTES de qualquer paint (cursor some do back).
        self.restore_cursor_underlay();

        // Only clear orb bounding box — NEVER full screen (anti-flicker).
        // Halo 2.1r cobre anéis/ticks; bbox = 2.9R + margem (bounds_radius).
        let mut orb_drawn = false;
        if self.dirty_orb || self.dirty_mesh {
            let orb_cr = self.soul_mirror.bounds_radius() as usize;
            let ox = self.soul_mirror.cx as usize;
            let oy = self.soul_mirror.cy as usize;
            let x0 = ox.saturating_sub(orb_cr).min(w);
            let y0 = oy.saturating_sub(orb_cr).min(h);
            let cw = (orb_cr * 2 + 24).min(w.saturating_sub(x0));
            let ch = (orb_cr * 2 + 24).min(h.saturating_sub(y0));
            if cw > 0 && ch > 0 {
                self.fb.fill_rect_fast(x0, y0, cw, ch, 8, 12, 24);  // JARVIS_BG
                self.last_orb_x0 = x0;
                self.last_orb_y0 = y0;
                self.last_orb_w = cw;
                self.last_orb_h = ch;
            }
            orb_drawn = true;
        }

        // Herói visual: Soul Mirror (brand).
        if self.avatar_visible && self.dirty_orb {
            self.draw_orb_layer();
            self.dirty_orb = false;
            orb_drawn = true;
        }
        crate::display::fb::diag_mark(5);

        // Mesh P2P — dentro da bbox do orb: a bbox é limpa a cada paint do
        // orb, então o grafo precisa ser redesenhado junto (não só no dirty_mesh).
        if orb_drawn {
            self.draw_mesh_graph();
            self.dirty_mesh = false;
        }

        // ═════════════════════════════════════════════════════════════
        // CAMADA 1: HUD mínimo — só quando dirty_hud/dialog (não a cada orb)
        // ═════════════════════════════════════════════════════════════
        let sb_h = 28usize;
        if paint_hud {
        self.fb.fill_rect_fast(0, 0, w, sb_h, 8, 12, 24);  // JARVIS_BG
        self.fb.fill_rect_fast(0, sb_h - 1, w, 1, JARVIS_CYAN_R, JARVIS_CYAN_G, JARVIS_CYAN_B);

        // Brand first (hero signal na barra)
        draw_text(
            &mut self.fb,
            12,
            6,
            "JARBAS",
            self.w,
            theme.accent.0,
            theme.accent.1,
            theme.accent.2,
        );

        // Micro badge do Hub Health (4 LEDs: USB/MEM/NET/AGENTS) — clique abre.
        if !hermes::hub_health::panel_visible() {
            let hh = crate::display::gauges::hub_health();
            let st = hub_badge_states(&hh);
            let (bx, by, _, _) = hub_badge_rect();
            for (i, s) in st.iter().enumerate() {
                let c = s.color();
                let lx = bx + i * 8;
                self.fb.fill_rect_fast(lx, by, 6, 6, c.0, c.1, c.2);
                self.fb.fill_rect_fast(lx, by, 6, 1, 8, 12, 24);
                self.fb.fill_rect_fast(lx, by + 5, 6, 1, 8, 12, 24);
                self.fb.fill_rect_fast(lx, by, 1, 6, 8, 12, 24);
                self.fb.fill_rect_fast(lx + 5, by, 1, 6, 8, 12, 24);
            }
        }

        let mem_mb = {
            let real = k_nano::memory::TOTAL_RAM_MB.load(core::sync::atomic::Ordering::Relaxed);
            if real > 0 { real } else { 0 }
        };
        let net = k_nano::env::net_hud_label();
        // Cache HUD string: recompute only when mem_mb changes
        let right = {
            let prev = HUD_CACHE_MEM.load(core::sync::atomic::Ordering::Relaxed);
            if prev != mem_mb {
                HUD_CACHE_MEM.store(mem_mb, core::sync::atomic::Ordering::Relaxed);
                let s = k_nano::boot_report::hud_line(mem_mb, net);
                *HUD_CACHE_STR.lock() = s.clone();
                s
            } else {
                HUD_CACHE_STR.lock().clone()
            }
        };
        let right_x = w.saturating_sub(right.len() * 8 + POWER_BTN_W + 24);
        draw_text(
            &mut self.fb,
            right_x,
            6,
            &right,
            self.w,
            theme.fg_muted.0,
            theme.fg_muted.1,
            theme.fg_muted.2,
        );

                // ── Per-core load bars (T-044: HUD pending/core) ──
        {
            let snap = crate::display::gauges::snapshot();
            let n = snap.core_count as usize;
            if n > 0 {
                let bar_w = 3usize;
                let bar_h = 10usize;
                let gap = 2usize;
                let base_x = 12 + 6 * 8 + 12;
                for c in 0..n.min(32) {
                    let load = snap.per_core_load[c].clamp(0.0, 1.0);
                    let filled = (load * bar_h as f32) as usize;
                    let x = base_x + c * (bar_w + gap);
                    self.fb.fill_rect(x, 14, bar_w, bar_h, 20, 25, 35);
                    if filled > 0 {
                        let y_fill = 14 + bar_h - filled;
                        let (cr, cg, cb) = if load < 0.55 {
                            (40, 200, 120)
                        } else if load < 0.80 {
                            (220, 180, 40)
                        } else {
                            (230, 70, 60)
                        };
                        self.fb.fill_rect(x, y_fill, bar_w, filled, cr, cg, cb);
                    }
                }
            }
        }

        // Diagnóstico freeze (s317): ticks de render + tick de agent em curso.
        // Se a tela congelar, o último frame pintado mostra o culpado ao vivo.
        {
            let rn = RENDER_N.load(core::sync::atomic::Ordering::Relaxed);
            let s = match agent_core::tick_in_progress() {
                Some((name, entered)) => alloc::format!(
                    "T{} IN:{} {}s",
                    rn,
                    name,
                    k_nano::tsc::now_ms().saturating_sub(entered) / 1000
                ),
                None => alloc::format!("T{}", rn),
            };
            let dx = 12 + 6 * 8 + 12 + 32 * (3 + 2) + 12;
            draw_text(
                &mut self.fb,
                dx,
                6,
                &s,
                self.w,
                theme.fg_muted.0,
                theme.fg_muted.1,
                theme.fg_muted.2,
            );
        }
        crate::display::fb::diag_mark(6);

// Botão OFF — canto SD
        {
            let (_bxp, by, bw, bh) = power_btn_rect(w);
            let off_x = w.saturating_sub(bw + 10);
            decorations::draw_rounded_rect(
                &mut self.fb,
                off_x,
                by,
                bw,
                bh,
                4,
                theme.error.0,
                theme.error.1,
                theme.error.2,
            );
            draw_text(
                &mut self.fb,
                off_x + 10,
                by + 6,
                "OFF",
                self.w,
                theme.fg.0,
                theme.fg.1,
                theme.fg.2,
            );
        }

        // Suit-boot welcome — linha centrada sob a barra (some após WELCOME_UNTIL)
        {
            let until = WELCOME_UNTIL.load(core::sync::atomic::Ordering::Relaxed);
            let now = k_nano::interrupts::wall_ticks();
            if now < until {
                if let Some(ref line) = *WELCOME_BANNER.lock() {
                    let max_chars = (w / 8).saturating_sub(4).max(16);
                    let shown = if line.len() > max_chars {
                        &line[..max_chars]
                    } else {
                        line.as_str()
                    };
                    let tw = shown.len() * 8;
                    let tx = w.saturating_sub(tw) / 2;
                    let ty = sb_h + 10;
                    self.fb.fill_rect(
                        tx.saturating_sub(8),
                        ty.saturating_sub(4),
                        tw + 16,
                        20,
                        12,
                        14,
                        22,
                    );
                    draw_text(
                        &mut self.fb,
                        tx,
                        ty,
                        shown,
                        self.w,
                        220,
                        230,
                        255,
                    );
                }
            } else if until != 0 {
                WELCOME_UNTIL.store(0, core::sync::atomic::Ordering::Relaxed);
                *WELCOME_BANNER.lock() = None;
            }
        }
        } // paint_hud

        // Dock pinta uma vez, depois das janelas (Z-order: dock por cima).

        // CAMADA 3: Workspace — ChatWindow (esquerdo, opaco, com gap)
        // ═════════════════════════════════════════════════════════════
        let gap = 4usize; // COSMIC tile gap
        let dock_h = if self.dock.visible { self.dock.height as usize } else { 0 };
        let panel_h = h.saturating_sub(sb_h + gap * 2 + dock_h);
        let left_w = w.saturating_sub(gap * 2); // margens laterais pequenas (painel Hermes removido)
        if paint_windows {
        let window_updates = {
            let ws = self.workspaces.active();
            let screen_rect = Rect {
                x: gap as i32, y: (sb_h + gap) as i32,
                width: left_w as u32, height: panel_h as u32,
            };
            let mut updates: Vec<(WindowId, Rect)> = Vec::new();
            if self.tiling_enabled { updates.extend(ws.layout_tiled(screen_rect)); }
            for fw in &ws.floating_windows { updates.push((fw.window_id, fw.rect)); }
            updates
        };
        for (wid, rect) in window_updates {
            if let Some(idx) = self.windows.iter().position(|w| w.id == wid) {
                if !self.windows[idx].visible || self.windows[idx].minimized {
                    continue;
                }
                // Aplica gap COSMIC entre tiles (floating já tem geom próprio)
                let gapped = if self.windows[idx].floating {
                    rect
                } else {
                    let gap = 4i32;
                    Rect {
                        x: rect.x + gap / 2,
                        y: rect.y + gap / 2,
                        width: (rect.width as i32 - gap).max(0) as u32,
                        height: (rect.height as i32 - gap).max(0) as u32,
                    }
                };
                self.windows[idx].rect = gapped;
                draw_window_fb(&mut self.fb, &self.windows[idx], theme, self.w);
                render_app_content(&mut self.fb, &self.windows[idx], self.w, self.h);
            }
        }
        } // paint_windows

        // ═════════════════════════════════════════════════════════════
        // CAMADA 4: Notificações + foco + power dialog
        // ═════════════════════════════════════════════════════════════
        // Overlay H2/H5 + RENDER_WINDOW — depois do fill, antes do swap (SESSION_261).
        if paint_windows || paint_hud {
            self.paint_overlays(theme);
        }

        if self.dock.visible && paint_dock {
            self.dock.render(&mut self.fb, theme);
        }

        if paint_hud || paint_windows {
            self.notifications.render(&mut self.fb, theme, Rect { x: 0, y: 0, width: w as u32, height: h as u32 }, self.tick);
        }

        // Modo Chat: hint discreto (Ambient = silêncio — orb é o sinal)
        if paint_hud && mode == FocusMode::Chat {
            let hint_y = if self.dock.visible {
                h.saturating_sub(self.dock.height as usize + 16)
            } else {
                h.saturating_sub(16)
            };
            draw_text(
                &mut self.fb,
                12,
                hint_y,
                "CHAT",
                self.w,
                theme.fg_muted.0,
                theme.fg_muted.1,
                theme.fg_muted.2,
            );
        }

        // Diálogo de energia (modal central) — 3 opções
        if self.power_dialog && self.dirty_dialog {
            let (dx, dy, dw, dh) = power_dialog_rect(self.w, self.h);
            // Sombra
            self.fb.fill_rect(dx + 4, dy + 4, dw, dh, 0, 0, 0);
            // Fundo
            self.fb.fill_rect(dx, dy, dw, dh, theme.bg_alt.0, theme.bg_alt.1, theme.bg_alt.2);
            // Borda superior de acento
            self.fb.fill_rect(dx, dy, dw, 2, theme.accent.0, theme.accent.1, theme.accent.2);
            // Título
            draw_text(&mut self.fb, dx + 16, dy + 12, "Opcoes de Energia", self.w,
                      theme.fg.0, theme.fg.1, theme.fg.2);
            // Instrução
            draw_text(&mut self.fb, dx + 16, dy + 36, "Selecione uma acao:", self.w,
                      theme.fg_muted.0, theme.fg_muted.1, theme.fg_muted.2);
            // 3 botões em linha
            let btn_y = dy + dh - 44;
            let btn_h = 28;
            let btn_w = 68;
            let gap = 10;
            let total_w = btn_w * 3 + gap * 2;
            let start_x = dx + (dw.saturating_sub(total_w)) / 2;

            // Desligar (error)
            self.fb.fill_rect(start_x, btn_y, btn_w, btn_h,
                              theme.error.0, theme.error.1, theme.error.2);
            draw_text(&mut self.fb, start_x + 8, btn_y + 6, "Desligar", self.w, 255, 255, 255);

            // Hibernar (success - energia)
            self.fb.fill_rect(start_x + btn_w + gap, btn_y, btn_w, btn_h,
                              theme.success.0, theme.success.1, theme.success.2);
            draw_text(&mut self.fb, start_x + btn_w + gap + 8, btn_y + 6, "Hibernar", self.w, 255, 255, 255);

            // Reiniciar (warning)
            self.fb.fill_rect(start_x + (btn_w + gap) * 2, btn_y, btn_w, btn_h,
                              theme.warning.0, theme.warning.1, theme.warning.2);
            draw_text(&mut self.fb, start_x + (btn_w + gap) * 2 + 8, btn_y + 6, "Reiniciar", self.w, 255, 255, 255);
        }

        // Virtual console
        let vcon_active = crate::vconsole::active();
        if vcon_active != 0 {
            let lines = crate::vconsole::get_active_visible();
            for (i, line) in lines.iter().enumerate() {
                let y = i * 16;
                if y + 16 > h { break; }
                self.fb.fill_rect(0, y, self.w, 16, line.bg.0, line.bg.1, line.bg.2);
                draw_text(&mut self.fb, 0, y, &line.text, self.w, line.fg.0, line.fg.1, line.fg.2);
            }
            draw_text(&mut self.fb, self.w - 60, 0, &alloc::format!("F{}", vcon_active + 1), self.w, 255, 255, 100);
        }

        // Hub Health plaque (por cima de tudo exceto o cursor; SESSION_261).
        if self.dirty_panel {
            let bbox = if orb_drawn {
                Some((self.last_orb_x0, self.last_orb_y0, self.last_orb_w, self.last_orb_h))
            } else {
                None
            };
            self.draw_hub_panel(now, bbox);
            self.dirty_panel = false;
        }

        // Cursor do mouse (save underlay → draw → present dirty)
        let mx = MOUSE_X.load(core::sync::atomic::Ordering::Relaxed);
        let my = MOUSE_Y.load(core::sync::atomic::Ordering::Relaxed);
        self.save_cursor_underlay(mx, my);
        draw_mouse_cursor(&mut self.fb, mx, my, self.w, self.h);
        self.dirty_cursor = false;
        let need_full = self.dirty_windows || self.dirty_dialog || vcon_active != 0;
        crate::display::fb::diag_mark(7);
        let hub_swap = self.hub_swap_rect.take();
        self.present_frame(need_full, paint_hud || paint_dock, orb_drawn, hub_swap);
        crate::display::fb::diag_mark(8);
        self.dirty_windows = false;
        self.dirty_dialog = false;
        self.dirty_hud = false;
    }

    /// 1º frame / janelas = swap full; orb animado = só dirty-rects (anti-freeze TCG).
    fn present_frame(
        &mut self,
        need_full: bool,
        swap_hud: bool,
        swap_orb: bool,
        hub_swap: Option<(usize, usize, usize, usize)>,
    ) {
        let w = self.w;
        let sb_h = 28usize;
        if self.full_swap_pending || need_full {
            self.fb.swap();
            self.full_swap_pending = false;
            return;
        }
        // HUD / dock só se foram pintados neste frame (orb-only = skip)
        if swap_hud {
            self.fb.swap_rect(0, 0, w, sb_h + 28);
            if self.dock.visible {
                let dh = self.dock.height as usize;
                let h = self.h;
                self.fb.swap_rect(0, h.saturating_sub(dh), w, dh);
            }
        }
        // Orb region
        if swap_orb && self.last_orb_w > 0 && self.last_orb_h > 0 {
            self.fb.swap_rect(
                self.last_orb_x0,
                self.last_orb_y0,
                self.last_orb_w,
                self.last_orb_h,
            );
        }
        // Hub Health region (toggle/slide/checksum — partial não muda pixels).
        if let Some((hx, hy, hw, hh)) = hub_swap {
            self.fb.swap_rect(hx, hy, hw, hh);
        }
        // Cursor underlay
        self.fb.swap_rect(
            self.cursor_under_x,
            self.cursor_under_y,
            CURSOR_UNDER_W,
            CURSOR_UNDER_H,
        );
        self.fb.dirty = false;
    }

    fn restore_cursor_underlay(&mut self) {
        if !self.cursor_under_valid {
            return;
        }
        self.fb.copy_rect_in(
            self.cursor_under_x,
            self.cursor_under_y,
            CURSOR_UNDER_W,
            CURSOR_UNDER_H,
            &self.cursor_under,
        );
        self.cursor_under_valid = false;
    }

    fn save_cursor_underlay(&mut self, mx: usize, my: usize) {
        let x0 = mx.saturating_sub(CURSOR_UNDER_OX);
        let y0 = my.saturating_sub(CURSOR_UNDER_OY);
        self.cursor_under_x = x0;
        self.cursor_under_y = y0;
        self.fb.copy_rect_out(x0, y0, CURSOR_UNDER_W, CURSOR_UNDER_H, &mut self.cursor_under);
        self.cursor_under_valid = true;
    }

    /// Mesh P2P — arestas + satélites em torno do orb (brand = Soul Mirror).
    /// Sem peers: no-op (orb sozinho = composição limpa). Redesenhado junto do
    /// orb (a bbox é limpa a cada paint); posições via SIN_LUT, sem trig.
    fn draw_mesh_graph(&mut self) {
        let peers = crate::display::agent::MESH_GRAPH.lock();
        let n = peers.len().min(8);
        if n == 0 {
            return;
        }
        let (w, h) = (self.w, self.h);
        let cx = (w / 2) as isize;
        let cy = (h / 2) as isize;
        let orbit = (core::cmp::min(w, h) as f32 * 0.32) as isize;
        let phase = (k_nano::tsc::now_us() / 31_250) as u8; // ~8 s/volta
        for (i, p) in peers.iter().take(n).enumerate() {
            let a8 = ((i as u32 * 256) / n as u32) as u8;
            let s = sin_q8(a8) as isize;
            let c = cos_q8(a8) as isize;
            let px = cx + ((c * orbit) >> 7);
            let py = cy + ((s * orbit) >> 7);
            let pulse = (sin_q8(phase.wrapping_add((i as u8) * 32)) as isize) / 24;
            let (er, eg, eb) = if p.reachable {
                let t = (p.p99_rtt.min(1500) as f32 / 1500.0).clamp(0.0, 1.0);
                let rr = (60.0 + t * 180.0) as u8;
                let gg = (220.0 - t * 160.0) as u8;
                (rr, gg, 60u8)
            } else {
                (70, 70, 70)
            };
            self.fb.draw_line(cx, cy, px, py, er, eg, eb);
            let r = 5 + pulse;
            self.fb.fill_circle_glow(px, py, r + 6, er, eg, eb, 40);
            self.fb.fill_circle_glow(px, py, r, er, eg, eb, 90);
        }
    }

    fn paint_overlays(&mut self, theme: &Theme) {
        {
            let marks = crate::display::overlay::EMBED_MARKS.lock();
            for m in marks.iter() {
                if m.splat {
                    crate::display::embed_viz::draw_thought_splat(
                        &mut self.fb, m.x, m.y, 8, m.color,
                    );
                } else {
                    crate::display::embed_viz::draw_embed_point(
                        &mut self.fb, m.x, m.y, m.color,
                    );
                }
            }
        }
        let overlays: alloc::vec::Vec<_> = crate::display::overlay::RENDER_OVERLAYS.lock().clone();
        if overlays.is_empty() {
            return;
        }
        let registry = crate::display::render_registry::RENDER_REGISTRY.lock();
        for ov in &overlays {
            let _ = registry.render(&ov.name, &mut self.fb, ov.rect, theme, &ov.data);
        }
    }

    /// Hit-test canónico: dock → cards → janelas tiled/floating. Orb/mesh = miss.
    /// Sem painel legado 35% (SESSION_261).
    pub fn handle_desktop_click(&mut self, cx: i32, cy: i32) -> &'static str {
        if self.dock.visible {
            if let Some(idx) = self.dock.hit_test(cx, cy) {
                if let Some(item) = self.dock.items.get(idx) {
                    let app = item.app_id;
                    if app == AppId::Power {
                        self.open_power_dialog();
                        return "dock:power";
                    }
                    self.toggle_app(app);
                    return "dock:app";
                }
            }
        }
        let card = self.card_click(cx, cy);
        if card != "miss" {
            return card;
        }
        self.hit_workspace_window(cx, cy)
    }

    fn hit_workspace_window(&mut self, cx: i32, cy: i32) -> &'static str {
        let ids: alloc::vec::Vec<WindowId> = {
            let ws = self.workspaces.active();
            let mut ids: alloc::vec::Vec<WindowId> = ws.floating_windows.iter().map(|f| f.window_id).collect();
            if let Some(root) = &ws.tiling_root {
                let mut tiled = alloc::vec::Vec::new();
                root.layout(Rect { x: 0, y: 0, width: 1, height: 1 }, &mut tiled);
                for (id, _) in tiled { ids.push(id); }
            }
            ids
        };
        for id in ids.into_iter().rev() {
            let idx = match self.windows.iter().position(|w| w.id == id && w.visible && !w.minimized) {
                Some(i) => i,
                None => continue,
            };
            if matches!(self.windows[idx].content, WindowContent::Card(_)) {
                continue;
            }
            let Some(area) = decorations::hit_test(cx as usize, cy as usize, &self.windows[idx]) else {
                continue;
            };
            match area {
                HitArea::CloseButton => {
                    self.close_tiled_window(id);
                    return "win:close";
                }
                HitArea::MaximizeButton => {
                    self.focus_stack.focus(id);
                    self.maximize_focused();
                    return "win:max";
                }
                HitArea::MinimizeButton => {
                    self.focus_stack.focus(id);
                    self.minimize_focused();
                    return "win:min";
                }
                HitArea::TitleBar => {
                    let wr = self.windows[idx].rect;
                    self.drag_state = Some(DragState::Move {
                        window_id: id,
                        offset_x: cx - wr.x,
                        offset_y: cy - wr.y,
                    });
                    return "win:drag";
                }
                HitArea::Body | HitArea::Client => {
                    if self.windows[idx].app_id == Some(AppId::HermesChat) {
                        *FOCUS_MODE.lock() = FocusMode::Chat;
                        let wr = self.windows[idx].rect;
                        let mut cw = crate::display::chat_window::CHAT_WINDOW.lock();
                        if let Some(ref mut chat) = *cw {
                            chat.handle_click(
                                cx as usize, cy as usize,
                                wr.x.max(0) as usize, wr.y.max(0) as usize,
                                wr.width as usize, wr.height as usize,
                            );
                        }
                    }
                    self.focus_stack.focus(id);
                    for w in &mut self.windows { w.focused = w.id == id; }
                    self.bring_to_front(id);
                    return "win:focus";
                }
                _ => {
                    self.focus_stack.focus(id);
                    return "win:resize";
                }
            }
        }
        *FOCUS_MODE.lock() = FocusMode::Ambient;
        "miss"
    }

    fn window_in_active_ws(&self, id: WindowId) -> bool {
        let ws = self.workspaces.active();
        if ws.floating_windows.iter().any(|f| f.window_id == id) {
            return true;
        }
        ws.tiling_root.as_ref().and_then(|r| r.find_window(id)).is_some()
    }

    fn sync_floating_rect(&mut self, id: WindowId, rect: Rect) {
        if let Some(fw) = self.workspaces.active_mut().floating_windows.iter_mut().find(|f| f.window_id == id) {
            fw.rect = rect;
            if let WindowContent::Card(ref mut d) = fw.content {
                d.x = rect.x;
                d.y = rect.y;
                d.w = rect.width as i32;
                d.h = rect.height as i32;
            }
        }
    }

    /// Garante janela no que `render()` itera (floating da workspace).
    pub fn show_app(&mut self, app_id: AppId) {
        if app_id == AppId::HermesChat && !crate::display::chat_window::chat_ui_enabled() {
            return;
        }
        self.invalidate_windows();
        if !self.windows.iter().any(|w| w.app_id == Some(app_id)) {
            let title = match app_id {
                AppId::HermesChat => "Jarbas Chat",
                AppId::Settings => "Settings",
                AppId::Power => "Power",
                _ => "App",
            };
            self.register_app(app_id, title, Layer::AppWindows);
        }
        let idx = match self.windows.iter().position(|w| w.app_id == Some(app_id)) {
            Some(i) => i,
            None => return,
        };
        let id = self.windows[idx].id;
        if self.windows[idx].visible && self.window_in_active_ws(id) {
            self.windows[idx].minimized = false;
            self.focus_stack.focus(id);
            self.bring_to_front(id);
            return;
        }
        let rect = Rect {
            x: 48,
            y: 40,
            width: (self.w as u32 * 3 / 5).clamp(360, 720),
            height: (self.h as u32 * 3 / 5).clamp(240, 520),
        };
        self.windows[idx].visible = true;
        self.windows[idx].minimized = false;
        self.windows[idx].floating = true;
        self.windows[idx].rect = rect;
        self.windows[idx].focused = true;
        if !self.window_in_active_ws(id) {
            let content = self.windows[idx].content.clone();
            self.workspaces.active_mut().add_window_floating(
                crate::display::window::FloatingWindow::new(id, rect, content),
            );
        } else {
            self.sync_floating_rect(id, rect);
        }
        self.focus_stack.focus(id);
        self.bring_to_front(id);
        self.dock.set_running(app_id, true, 1);
        if app_id == AppId::HermesChat {
            let mut cw = crate::display::chat_window::CHAT_WINDOW.lock();
            if cw.is_none() {
                *cw = Some(crate::display::chat_window::ChatWindow::new(0));
            }
            *FOCUS_MODE.lock() = FocusMode::Chat;
        }
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
            anim_scale: 0.0f32,
            data: alloc::string::String::new(),
            z: layer,
        });
        self.invalidate_windows();
    }

    pub fn render_window(&mut self, win: &Window, theme: &Theme) {
        draw_window_fb(&mut self.fb, win, theme, self.w);
    }

    pub fn open_power_dialog(&mut self) { self.power_dialog = true; self.invalidate_dialog(); }
    pub fn close_power_dialog(&mut self) { self.power_dialog = false; self.invalidate_dialog(); }

    /// Overlay Hermes = janela floating real (SESSION_261). Não pinta no tick.
    pub fn ensure_hermes_overlay(&mut self) {
        if !crate::display::chat_window::chat_ui_enabled() {
            return;
        }
        self.show_app(AppId::HermesChat);
    }

    pub fn spawn_card(&mut self, decl: crate::display::card::UiDeclaration) {
        let id = WindowId(self.next_window_id);
        self.next_window_id += 1;
        let title = decl.title.clone();
        let rect = Rect { x: decl.x, y: decl.y, width: decl.w as u32, height: decl.h as u32 };
        let content = WindowContent::Card(decl);
        self.windows.push(Window {
            id,
            app_id: None,
            content: content.clone(),
            rect,
            workspace: self.workspaces.active,
            focused: false,
            decorated: false,
            floating: true,
            minimized: false,
            maximized: false,
            title,
            visible: true,
            anim_scale: 0.0f32,
            data: alloc::string::String::new(),
            z: Layer::AppWindows,
        });
        // Também adiciona às floating windows da workspace para ser renderizado
        self.workspaces.active_mut().add_window_floating(
            crate::display::window::FloatingWindow::new(id, rect, content)
        );
        self.invalidate_windows();
    }

    pub fn card_click(&mut self, cx: i32, cy: i32) -> &'static str {
        self.card_hit_button = None;
        for i in (0..self.windows.len()).rev() {
            if !self.windows[i].visible {
                continue;
            }
            let content = self.windows[i].content.clone();
            if let WindowContent::Card(ref decl) = content {
                if cx >= decl.x && cx < decl.x + decl.w && cy >= decl.y && cy < decl.y + decl.h {
                    let card_id = decl.id;
                    let win_id = self.windows[i].id;
                    // Interacao com card e mudanca de estado: repaint das janelas.
                    self.invalidate_windows();
                    // Close button
                    let (crx, cry, crw, crh) = decl.close_rect();
                    if decl.closable && cx >= crx && cx < crx + crw && cy >= cry && cy < cry + crh {
                        self.workspaces.active_mut().remove_window(win_id);
                        self.windows.remove(i);
                        return "close";
                    }
                    // Resize handle (bottom-right corner)
                    let hx = decl.x + decl.w - 10;
                    let hy = decl.y + decl.h - 10;
                    if cx >= hx && cx < hx + 10 && cy >= hy && cy < hy + 10 {
                        self.resizing_card_id = Some(win_id);
                        return "resize";
                    }
                    // Title bar drag
                    if cy < decl.y + 22 {
                        self.dragging_card_id = Some(win_id);
                        self.card_drag_off = (cx - decl.x, cy - decl.y);
                        return "drag";
                    }
                    let hits = crate::display::card::hit_test_buttons(decl);
                    for btn in &hits {
                        if cx >= btn.x && cx < btn.x + btn.w && cy >= btn.y && cy < btn.y + btn.h {
                            self.card_hit_button = Some((card_id, btn.index));
                            let payload = alloc::format!("{}:{}", card_id, btn.index);
                            let _ = EVENT_BUS.publish(event_bus::Event {
                                id: 0,
                                topic: String::from(TOPIC_CARD_ACTION),
                                payload: payload.into_bytes(),
                                token: event_bus::CapabilityToken::Legacy(1),
                            });
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
        let Some(card_id) = self.dragging_card_id else { return; };
        let rect = if let Some(win) = self.windows.iter_mut().find(|w| w.id == card_id) {
            if let WindowContent::Card(ref mut decl) = win.content {
                decl.x = mx - self.card_drag_off.0;
                decl.y = my - self.card_drag_off.1;
                win.rect.x = decl.x;
                win.rect.y = decl.y;
                Some(win.rect)
            } else {
                None
            }
        } else {
            None
        };
        if let Some(rect) = rect {
            self.sync_floating_rect(card_id, rect);
            self.invalidate_windows();
        }
    }

    pub fn card_resize_step(&mut self, mx: i32, my: i32, btn_down: bool) {
        if !btn_down { self.resizing_card_id = None; return; }
        let Some(card_id) = self.resizing_card_id else { return; };
        let rect = if let Some(win) = self.windows.iter_mut().find(|w| w.id == card_id) {
            if let WindowContent::Card(ref mut decl) = win.content {
                decl.w = (mx - decl.x).max(100);
                decl.h = (my - decl.y).max(60);
                win.rect.width = decl.w.max(0) as u32;
                win.rect.height = decl.h.max(0) as u32;
                Some(win.rect)
            } else {
                None
            }
        } else {
            None
        };
        if let Some(rect) = rect {
            self.sync_floating_rect(card_id, rect);
            self.invalidate_windows();
        }
    }

    /// Arraste de janela app: atualiza `windows[]` e `floating_windows` (o que render lê).
    pub fn window_drag_step(&mut self, mx: i32, my: i32, btn_down: bool) {
        if !btn_down {
            self.drag_state = None;
            return;
        }
        let Some(DragState::Move { window_id, offset_x, offset_y }) = self.drag_state else {
            return;
        };
        let nx = (mx - offset_x).max(0);
        let ny = (my - offset_y).max(28);
        let rect = if let Some(win) = self.windows.iter_mut().find(|w| w.id == window_id) {
            win.rect.x = nx.min(self.w.saturating_sub(80) as i32);
            win.rect.y = ny.min(self.h.saturating_sub(80) as i32);
            Some(win.rect)
        } else {
            None
        };
        if let Some(rect) = rect {
            self.sync_floating_rect(window_id, rect);
        }
    }

    pub fn toggle_app(&mut self, app_id: AppId) {
        let idx = self.windows.iter().position(|w| w.app_id == Some(app_id));
        if let Some(idx) = idx {
            let id = self.windows[idx].id;
            let on_ws = self.window_in_active_ws(id);
            if self.windows[idx].visible && on_ws && !self.windows[idx].minimized {
                self.windows[idx].visible = false;
                self.workspaces.active_mut().remove_window(id);
                self.dock.set_running(app_id, false, 0);
                if app_id == AppId::HermesChat {
                    *FOCUS_MODE.lock() = FocusMode::Ambient;
                }
                self.invalidate_windows();
                return;
            }
        }
        self.show_app(app_id);
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
            self.invalidate_windows();
        }
    }

    /// Minimiza a janela focada (visible=false).
    pub fn minimize_focused(&mut self) {
        if let Some(id) = self.focus_stack.focused() {
            if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
                w.minimized = true;
                w.visible = false;
            }
            self.invalidate_windows();
        }
    }

    /// Toggle dock visibility.
    pub fn toggle_dock(&mut self) {
        self.dock.visible = !self.dock.visible;
        self.invalidate_windows();
    }

    /// Toggle floating da janela focada.
    pub fn toggle_floating_focused(&mut self) {
        if let Some(id) = self.focus_stack.focused() {
            if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
                w.floating = !w.floating;
            }
            self.invalidate_windows();
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

    /// Soul Mirror — orb afetivo (Onda 7) substitui o orb cyan fixo.
    /// Movimento vem do AffectVector + LoopPhase; cor/acento do OrbState
    /// (sinais reais setados pelo DisplayAgent em `set_orb_signals`).
    fn draw_orb_layer(&mut self) {
        let snap = hermes::globals::AFFECT_SNAPSHOT.lock();
        let affect = hermes::affect::AffectVector {
            valence: snap.valence,
            arousal: snap.arousal,
            dominance: snap.dominance,
            uncertainty: snap.uncertainty,
            urgency: snap.urgency,
            fatigue: snap.fatigue,
            curiosity: snap.curiosity,
            coherence: snap.coherence,
        };
        let phase_deg = snap.phase_deg;
        drop(snap);
        self.soul_mirror.set_affect(&affect, phase_deg);
        let fft_energy = crate::display::avatar::read_audio_energy();
        let sig = self.orb_signals;
        let now = ui_now_us(self.tick);
        let _ = self.soul_mirror.paint(&mut self.fb, fft_energy, now, &sig);
    }

    /// Glass plaque do Hub Health (só aqui — SESSION_261).
    ///
    /// `full` (toggle/slide/checksum/1 Hz/janelas) restaura o underlay e
    /// recompe o vidro inteiro; caso contrário só a interseção com o bbox do orb
    /// (que acabou de pintar por cima) é recomposta — sem acúmulo de alpha.
    fn draw_hub_panel(&mut self, now_us: u64, orb_bbox: Option<(usize, usize, usize, usize)>) {
        let (w, h) = (self.w, self.h);
        let (px, py, pw, ph) = hub_rect(w, h);
        if pw == 0 || ph == 0 {
            return;
        }

        let (sx, sy, sw, sh) = hub_staging(w, h);
        if !hermes::hub_health::panel_visible() {
            if self.hub_underlay_valid {
                self.fb.copy_rect_in(sx, sy, sw, sh, &self.hub_underlay);
                self.hub_underlay_valid = false;
                self.hub_swap_rect = Some((sx, sy, sw, sh));
            }
            return;
        }

        let bpp = self.fb.info.bpp;
        if self.hub_underlay.len() < sw * sh * bpp {
            self.hub_underlay = alloc::vec![0u8; sw * sh * bpp];
            self.hub_underlay_valid = false;
        }

        let animating = self.hub_anim_start_us != 0
            && now_us.saturating_sub(self.hub_anim_start_us) < HUB_SLIDE_US;
        let full = self.hub_full || animating || !self.hub_underlay_valid;

        if full {
            if self.hub_underlay_valid {
                self.fb.copy_rect_in(sx, sy, sw, sh, &self.hub_underlay);
            }
            self.fb.copy_rect_out(sx, sy, sw, sh, &mut self.hub_underlay);
            self.hub_underlay_valid = true;
        }

        // Slide-in (0 no repouso): frames intermediários são opacos.
        let off = if animating && now_us != 0 {
            let t = (now_us.saturating_sub(self.hub_anim_start_us) as f32 / HUB_SLIDE_US as f32).clamp(0.0, 1.0);
            ((1.0 - t) * HUB_SLIDE_PX as f32) as usize
        } else {
            0
        };
        let x = px + off;
        let y = py;

        if animating {
            self.fb.fill_rect_fast(x, y, pw, ph, HUB_BG.0, HUB_BG.1, HUB_BG.2);
        } else if full {
            let _ = self.fb.fill_rect_darken_tint(x, y, pw, ph, HUB_BG.0, HUB_BG.1, HUB_BG.2, 216);
        } else if let Some((bx, by, bw, bh)) = orb_bbox {
            let ix0 = x.max(bx);
            let iy0 = y.max(by);
            let ix1 = (x + pw).min(bx + bw);
            let iy1 = (y + ph).min(by + bh);
            if ix1 > ix0 && iy1 > iy0 {
                let _ = self.fb.fill_rect_darken_tint(
                    ix0, iy0, ix1 - ix0, iy1 - iy0, HUB_BG.0, HUB_BG.1, HUB_BG.2, 216,
                );
            }
        }

        // Borda 1px #0A6E96 + acento superior 2px ciano.
        let bd = HUB_BORDER;
        self.fb.fill_rect_fast(x, y, pw, 1, bd.0, bd.1, bd.2);
        self.fb.fill_rect_fast(x, y + ph.saturating_sub(1), pw, 1, bd.0, bd.1, bd.2);
        self.fb.fill_rect_fast(x, y, 1, ph, bd.0, bd.1, bd.2);
        self.fb.fill_rect_fast(x + pw.saturating_sub(1), y, 1, ph, bd.0, bd.1, bd.2);
        self.fb.fill_rect_fast(x, y, pw, 2, 0, 212, 255);

        // Conteúdo (layout escalado em q8 a partir de s = min(w,h)/720).
        let hh = crate::display::gauges::hub_health();
        let s_q = (core::cmp::min(w, h) * 256 / 720).max(64) as usize;
        let pad = (14 * s_q) >> 8;
        let mut cy = y + ((12 * s_q) >> 8);

        // Header + pill do worst (pill vem do agente — código sev 0..3).
        draw_text(&mut self.fb, x + pad, cy, "HUB HEALTH", self.w, 0, 212, 255);
        let pill_w = (60 * s_q) >> 8;
        let pill_h = (20 * s_q) >> 8;
        let pill_x = x + pw.saturating_sub(pill_w + pad);
        let pc = pill_color(hermes::hub_health::panel_pill());
        decorations::draw_rounded_rect(&mut self.fb, pill_x, cy - 2, pill_w, pill_h, 4, pc.0, pc.1, pc.2);
        let plabel = pill_label(hermes::hub_health::panel_pill());
        let plx = pill_x + pill_w.saturating_sub(plabel.len() * 9) / 2;
        draw_text(&mut self.fb, plx, cy + 1, plabel, self.w, 8, 12, 24);

        cy += (24 * s_q) >> 8;
        draw_text(&mut self.fb, x + pad, cy, hh.live_line(), self.w, 122, 150, 170);
        cy += (18 * s_q) >> 8;
        self.fb.fill_rect_fast(x + pad / 2, cy, pw.saturating_sub(pad), 1, HUB_DIV.0, HUB_DIV.1, HUB_DIV.2);
        cy += (8 * s_q) >> 8;

        // ~13-15 linhas agrupadas: 0 xhci/usb | 2 timer/frame | 4 storage/bootlog/kv
        // | 7 heap/ram/arena/model | 11 agents/mesh/infer/fault.
        const GROUP_START: [usize; 5] = [0, 2, 4, 7, 11];
        let worst_row = hermes::hub_health::panel_worst_row();
        let pitch = (23 * s_q) >> 8;
        let sq = (8 * s_q) >> 8;
        for (i, row) in hh.rows.iter().enumerate() {
            if i != 0 && GROUP_START.contains(&i) {
                self.fb.fill_rect_fast(
                    x + pad / 2,
                    cy.saturating_sub(3),
                    pw.saturating_sub(pad),
                    1,
                    HUB_DIV.0,
                    HUB_DIV.1,
                    HUB_DIV.2,
                );
            }
            let rc = row.state.color();
            self.fb.fill_rect_fast(x + pad, cy + (pitch.saturating_sub(sq)) / 2, sq, sq, rc.0, rc.1, rc.2);
            // Pior subsystema (do agente) tem o label aceso — highlight.
            let (lr, lg, lb) = if worst_row != 0xFF && i == worst_row as usize {
                rc
            } else {
                HUB_LABEL
            };
            draw_text(&mut self.fb, x + pad + sq + 8, cy, row.label, self.w, lr, lg, lb);
            let v = row.value();
            let vx = x + pw.saturating_sub(pad + v.len() * 9);
            draw_text(&mut self.fb, vx, cy, v, self.w, HUB_VALUE.0, HUB_VALUE.1, HUB_VALUE.2);
            cy += pitch;
        }

        self.hub_painted_checksum = hh.checksum;
        self.hub_last_paint_us = now_us;
        self.hub_full = false;
        // Partial não muda pixels no front (orb apagou e recompe o mesmo vidro);
        // só full/slide precisam apresentar a região.
        if full {
            self.hub_swap_rect = Some((sx, sy, sw, sh));
        }
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
    // Flash curto no clique — cabe no underlay 24×24 (sem rastro).
    let flash = k_nano::interrupts::MOUSE_CLICK_FLASH.load(core::sync::atomic::Ordering::Acquire);
    if flash > 0 {
        k_nano::interrupts::MOUSE_CLICK_FLASH.fetch_sub(1, core::sync::atomic::Ordering::AcqRel);
        let r = 4 + (flash as usize).min(8);
        let ox = mx.saturating_sub(r / 2);
        let oy = my.saturating_sub(r / 2);
        fb.fill_rect(ox, oy, r.min(scr_w.saturating_sub(ox)), 1, 255, 220, 40);
        fb.fill_rect(ox, oy, 1, r.min(scr_h.saturating_sub(oy)), 255, 220, 40);
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
                fb.set_pixel(x, y, 0, 0, 0);
            } else {
                fb.set_pixel(x, y, 255, 255, 255);
            }
        }
    }
}

/// Renderiza conteúdo interno da janela.
/// No WM cosmic, só o ChatWindow (HermesChat) tem conteúdo ativo.
fn render_app_content(fb: &mut DoubleBuffer, win: &Window, scr_w: usize, _scr_h: usize) {
    match &win.content {
        WindowContent::Card(decl) => {
            let _cx = win.rect.x as usize;
            let _cy = win.rect.y as usize;
            let _buttons = crate::display::card::render_card(fb, decl);
        }
        _ => {
            if win.app_id() == AppId::HermesChat {
                let cw = crate::display::chat_window::CHAT_WINDOW.lock();
                if let Some(ref chat) = *cw {
                    let cx = win.rect.x as usize;
                    let cy = win.rect.y as usize;
                    chat.render(fb, cx, cy, win.rect.width as usize, win.rect.height as usize, scr_w);
                }
            }
        }
    }
}

#[cfg(test)]
mod hub_panel_tests {
    use super::*;

    #[test]
    fn hub_rect_spec_at_1280x720() {
        // s = 1 -> geometria literal da spec.
        assert_eq!(hub_rect(1280, 720), (844, 44, 420, 560));
    }

    #[test]
    fn hub_rect_scales_and_clamps() {
        // 1440p: s = 2 (min=1440/720) -> w=840, h=min(1440-88,560)*2=1120, x=2560-872.
        let (x, y, w, h) = hub_rect(2560, 1440);
        assert_eq!(w, 840);
        assert_eq!(x, 2560 - 872);
        assert_eq!(y, 88);
        assert_eq!(h, 1120);
        // 640x480 (s~0.667): nunca estoura a tela.
        let (x, y, w, h) = hub_rect(640, 480);
        assert!(x + w <= 640, "painel fora da tela: {x}+{w}");
        assert!(y + h <= 480, "painel fora da tela: {y}+{h}");
        assert!(w > 0 && h > 0);
    }

    #[test]
    fn hub_staging_covers_slide_range() {
        let (px, py, pw, ph) = hub_rect(1280, 720);
        let (sx, sy, sw, sh) = hub_staging(1280, 720);
        assert_eq!((sx, sy), (px, py));
        assert_eq!(sh, ph);
        // Staging cobre o painel inteiro + o overshoot do slide que cabe na tela
        // (a borda direita do painel está a 16px da tela; slide 24px é clipado).
        assert!(sw >= pw);
        assert_eq!(sw, (pw + HUB_SLIDE_PX).min(1280 - px));
        assert!(sx + sw <= 1280);
    }

    #[test]
    fn hub_badge_inside_hud_bar() {
        let (x, y, w, h) = hub_badge_rect();
        assert!(x >= 60 && x + w < 1280);
        assert!(y + h <= 28, "badge fora da barra HUD");
    }
}

#[cfg(test)]
mod invalidation_tests {
    use super::*;
    use crate::display::fb::GpuDevice;

    fn test_desktop() -> JarbasDesktop {
        // FB pequeno sintetico (addr 0 -> diag/FB writes sao no-op no host).
        let gpu = GpuDevice::from_probe(0, 320, 200, 320, 4, false);
        JarbasDesktop::new(DoubleBuffer::from_gpu(&gpu))
    }

    #[test]
    fn show_app_invalidates_windows() {
        let mut d = test_desktop();
        d.dirty_windows = false;
        d.show_app(AppId::Settings);
        assert!(d.dirty_windows, "show_app deve agendar repaint das janelas");
    }

    #[test]
    fn spawn_card_invalidates_windows() {
        let mut d = test_desktop();
        d.dirty_windows = false;
        let decl = crate::display::card::UiDeclaration::new(1, "card", 8, 8, 120, 90);
        d.spawn_card(decl);
        assert!(d.dirty_windows, "spawn_card deve agendar repaint das janelas");
    }

    #[test]
    fn toggle_app_and_dock_invalidate_windows() {
        let mut d = test_desktop();
        d.show_app(AppId::Settings);
        d.dirty_windows = false;
        d.toggle_app(AppId::Settings); // fecha a janela visivel
        assert!(d.dirty_windows, "fechar app deve agendar repaint");

        d.dirty_windows = false;
        d.toggle_dock();
        assert!(d.dirty_windows, "toggle_dock deve agendar repaint");
    }

    #[test]
    fn hit_test_miss_keeps_windows_clean() {
        // Sem mutacao (clique que erra): nao agenda repaint de janelas.
        let mut d = test_desktop();
        d.dirty_windows = false;
        assert_eq!(d.handle_desktop_click(1, 1), "miss");
        assert!(!d.dirty_windows, "miss nao deve agendar repaint");
    }

    #[test]
    fn no_op_render_keeps_windows_clean() {
        let mut d = test_desktop();
        d.dirty_windows = false;
        d.last_paint_tick = 0;
        // Segura o gate de relogio de parede -> render sai antes de pintar/agendar.
        LAST_PRESENT_US.store(ui_now_us(7), core::sync::atomic::Ordering::Relaxed);
        d.render(7);
        assert!(!d.dirty_windows, "render no-op nao deve agendar janelas");
    }
}



