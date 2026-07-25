//! DisplayAgent — JARVIS Desktop com compositor multi-app.
//! Hermes Chat + Settings + Power + JARVIS avatar overlay.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use hermes;
use k_nano::EVENT_BUS;
use crate::display::fb::{DoubleBuffer, GPU};
use crate::display::compositor::{COMPOSITOR, JarvisDesktop, AppId, Layer, MOUSE_X, MOUSE_Y, MOUSE_BUTTONS, POWER_BANNER, hit_power_button};
use crate::display::avatar::{AvatarState, JarvisAvatar};
use crate::display::ui_spec::{self, TOPIC_UI_SPEC};
use crate::clipboard_notify::TOPIC_TOAST;

const DISPLAY_MANIFEST: AgentManifest = AgentManifest {
    name: "display",
    kind: AgentKind::Console,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

pub struct DisplayAgent {
    receiver: event_bus::Receiver,
    echo_receiver: event_bus::Receiver,
    mouse_receiver: event_bus::Receiver,
    click_receiver: event_bus::Receiver,
    ui_receiver: event_bus::Receiver,
    hitl_receiver: event_bus::Receiver,
    hitl_term_receiver: event_bus::Receiver,
    memory_nudge_receiver: event_bus::Receiver,
    toast_receiver: event_bus::Receiver,
    latent_receiver: Option<event_bus::LatentReceiver>,
    gpu_inited: bool,
    demo_ui_sent: bool,
    input_buffer: alloc::string::String,
    avatar: Option<JarvisAvatar>,
    dragging: bool,
    drag_id: AppId,
    drag_off_x: isize,
    drag_off_y: isize,
    /// Arm do botão OFF (tick até quando o 2º clique confirma).
    power_armed_until: usize,
}

impl DisplayAgent {
    pub fn new() -> Self {
        // NÃO claim_graphics aqui: no HW real o new() roda no register (ainda
        // mid-boot) e apagaria K*/BOOT.LOG visual. Claim só no 1º tick com FB.
        // LatentBus subscribe adiado p/ 1º tick — no HW real o new() aqui
        // coincidia com freeze pos-www (foto AIOS + 18 agents).
        DisplayAgent {
            receiver: EVENT_BUS.subscribe(hermes::hermes::TOPIC_HERMES_RESPONSE),
            echo_receiver: EVENT_BUS.subscribe("KEYBOARD_ECHO"),
            mouse_receiver: EVENT_BUS.subscribe(hermes::agents::mouse_agent::TOPIC_MOUSE_MOVED),
            click_receiver: EVENT_BUS.subscribe(hermes::agents::mouse_agent::TOPIC_MOUSE_CLICK),
            ui_receiver: EVENT_BUS.subscribe(TOPIC_UI_SPEC),
            hitl_receiver: EVENT_BUS.subscribe(hermes::hitl_ui::TOPIC_HITL_REQUEST),
            hitl_term_receiver: EVENT_BUS.subscribe(hermes::hitl_ui::TOPIC_HITL_TERMINAL),
            memory_nudge_receiver: EVENT_BUS.subscribe(hermes::cognitive_bridge::TOPIC_MEMORY_NUDGE),
            toast_receiver: EVENT_BUS.subscribe(TOPIC_TOAST),
            latent_receiver: None,
            gpu_inited: false,
            demo_ui_sent: false,
            input_buffer: alloc::string::String::new(),
            avatar: None,
            dragging: false,
            drag_id: AppId::None,
            drag_off_x: 0,
            drag_off_y: 0,
            power_armed_until: 0,
        }
    }

    /// Drena receiver → overlay Hermes (HITL / terminal / memory nudge).
    fn drain_hermes_overlay(
        avatar: &mut Option<JarvisAvatar>,
        rx: &mut event_bus::Receiver,
        mode: OverlayMode,
    ) {
        while let Some(ev) = rx.try_receive() {
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            if let Some(ref mut desktop) = *COMPOSITOR.lock() {
                desktop.ensure_hermes_overlay();
                if let Some(chat) = desktop.apps.iter_mut().find(|a| a.id == AppId::HermesChat) {
                    chat.visible = true;
                    match mode {
                        OverlayMode::HitlConfirm => {
                            chat.data.push_str("[HITL] Confirmação necessária\n");
                            chat.data.push_str(text);
                            chat.data.push('\n');
                            chat.data.push_str("Responda: /approve <id>  ou  /deny <id>\n");
                            chat.data.push_str(
                                "Preferência: /ui jarbas | /ui terminal | /commands\n",
                            );
                        }
                        OverlayMode::HitlTerminal => {
                            chat.title = alloc::string::String::from("Hermes Terminal");
                            chat.data.clear();
                            chat.data.push_str(text);
                            chat.data.push('\n');
                        }
                        OverlayMode::MemoryNudge => {
                            chat.data.push_str(text);
                            chat.data.push('\n');
                        }
                    }
                }
            }
            if matches!(mode, OverlayMode::HitlConfirm | OverlayMode::MemoryNudge) {
                if let Some(ref mut av) = avatar {
                    av.set_state(AvatarState::Listening);
                }
            }
            match mode {
                OverlayMode::HitlConfirm => k_nano::slog_jarbas!("JARBAS", "HITL", "request received"),
                OverlayMode::MemoryNudge => k_nano::slog_jarbas!("JARBAS", "info", "MEMORY_NUDGE"),
                OverlayMode::HitlTerminal => {}
            }
        }
    }

    /// Clique canônico: edge de `MOUSE_ABS_BTN` (não EventBus). Retorna hit p/ log.
    fn handle_pointer_click(&mut self, btn: u8, cx: usize, cy: usize) -> &'static str {
        let (scr_w, scr_h, dialog_open) = {
            let comp = COMPOSITOR.lock();
            match comp.as_ref() {
                Some(d) => (d.w, d.h, d.power_dialog),
                None => (1280, 800, false),
            }
        };

        // Se o diálogo de power está aberto, processa cliques nele primeiro
        if dialog_open {
            let action = crate::display::compositor::hit_power_dialog(cx, cy, scr_w, scr_h);
            match action {
                crate::display::compositor::PowerDialogAction::Cancel => {
                    COMPOSITOR.lock().as_mut().map(|d| d.close_power_dialog());
                    *POWER_BANNER.lock() = None;
                    self.power_armed_until = 0;
                    k_nano::slog_jarbas!("JARBAS", "POWER", "dialog CANCELADO pelo usuario");
                    return "power_cancel";
                }
                crate::display::compositor::PowerDialogAction::TurnOff => {
                    COMPOSITOR.lock().as_mut().map(|d| d.close_power_dialog());
                    *POWER_BANNER.lock() = Some("Shutting down...");
                    k_nano::slog_jarbas!("JARBAS", "POWER", "dialog CONFIRMADO — publicando SYSTEM_SHUTDOWN");
                    let _ = EVENT_BUS.publish(event_bus::Event {
                        id: 0,
                        topic: alloc::string::String::from("SYSTEM_SHUTDOWN"),
                        payload: b"ui_off".to_vec(),
                        token: event_bus::CapabilityToken::Legacy(1),
                    });
                    self.power_armed_until = 0;
                    return "power_off";
                }
                crate::display::compositor::PowerDialogAction::None => {
                    // Clique fora do diálogo (mas diálogo aberto) — ignora
                    return "power_dialog_bg";
                }
            }
        }

        // OFF canto SD — abre diálogo de confirmação
        if hit_power_button(cx, cy, scr_w) {
            COMPOSITOR.lock().as_mut().map(|d| d.open_power_dialog());
            *POWER_BANNER.lock() = Some("Confirme o desligamento");
            k_nano::slog_jarbas!("JARBAS", "POWER", "dialog de desligamento ABERTO");
            return "power_dialog_open";
        }
        // Clique fora desarma banner antigo (se houver)
        if self.power_armed_until != 0 {
            self.power_armed_until = 0;
            *POWER_BANNER.lock() = None;
        }

        let mut hit: &'static str = "miss";
        let mut card_action: Option<(u32, usize)> = None;
        let mut comp = COMPOSITOR.lock();
        if let Some(ref mut desktop) = *comp {
            // ADR-0058 S3: cards ficam por cima — testa antes de dock/app.
            if (btn & 1) != 0 {
                match desktop.card_click(cx as i32, cy as i32) {
                    crate::display::compositor::CardClick::Close => {
                        drop(comp);
                        return "card:close";
                    }
                    crate::display::compositor::CardClick::DragStart => {
                        self.dragging = true;
                        self.drag_id = AppId::None; // arraste de card é do compositor
                        drop(comp);
                        return "card:drag";
                    }
                    crate::display::compositor::CardClick::ResizeStart => {
                        self.dragging = true;
                        self.drag_id = AppId::None; // resize de card é do compositor
                        drop(comp);
                        return "card:resize";
                    }
                    crate::display::compositor::CardClick::Button(id, idx) => {
                        card_action = Some((id, idx));
                    }
                    crate::display::compositor::CardClick::Focus => {
                        drop(comp);
                        return "card:focus";
                    }
                    crate::display::compositor::CardClick::Miss => {}
                }
            }
            if card_action.is_some() {
                drop(comp);
                if let Some((id, idx)) = card_action {
                    let _ = EVENT_BUS.publish(event_bus::Event {
                        id: 0,
                        topic: alloc::string::String::from("CARD_ACTION"),
                        payload: alloc::format!("{}:{}", id, idx).into_bytes(),
                        token: event_bus::CapabilityToken::Legacy(1),
                    });
                }
                return "card:btn";
            }
            let apps_clone = desktop.apps.clone();
            let dock_y = desktop.h.saturating_sub(36);
            if cy >= dock_y {
                hit = "dock";
                for (idx, app) in apps_clone.iter().enumerate() {
                    if !app.visible {
                        continue;
                    }
                    let rx = 10 + idx * 66;
                    if cx >= rx && cx <= rx + 60 {
                        desktop.toggle_app(app.id);
                        hit = match app.id {
                            AppId::HermesChat => "dock:chat",
                            AppId::Settings => "dock:settings",
                            AppId::Power => "dock:power",
                            AppId::Ide => "dock:ide",
                            AppId::Camera => "dock:camera",
                            AppId::AudioViz => "dock:audio",
                            AppId::WasmSkill(_) => "dock:skill",
                            AppId::None => "dock",
                        };
                        break;
                    }
                }
            } else if (btn & 1) != 0 {
                for app in &apps_clone {
                    if !app.visible {
                        continue;
                    }
                    let cx_btn = app.x + app.w - 20;
                    if cx >= cx_btn
                        && cx <= cx_btn + 16
                        && cy >= app.y + 3
                        && cy <= app.y + 19
                    {
                        desktop.close_window(app.id);
                        hit = "close";
                        break;
                    }
                    if cx >= app.x
                        && cx <= app.x + app.w
                        && cy >= app.y
                        && cy <= app.y + 24
                    {
                        self.dragging = true;
                        self.drag_id = app.id;
                        self.drag_off_x = cx as isize - app.x as isize;
                        self.drag_off_y = cy as isize - app.y as isize;
                        hit = "titlebar";
                        break;
                    }
                }
            }
        }
        drop(comp);
        hit
    }

    fn apply_ui_spec(&mut self, json: &str) {
        // ADR-0058: se o JSON traz "body", é um card declarativo → spawn.
        if json.contains("\"body\"") {
            if let Some(decl) = crate::display::card::parse_card(json) {
                if let Some(ref mut desktop) = *COMPOSITOR.lock() {
                    let title = decl.title.clone();
                    desktop.spawn_card(decl);
                    ui_spec::mark_ui_ok();
                    k_nano::slog_jarbas!("UI", "info", "card spawn title={} (ADR-0058)", title);
                }
                return;
            }
        }
        if let Some(spec) = ui_spec::parse_window_spec(json) {
            if let Some(ref mut desktop) = *COMPOSITOR.lock() {
                if let Some(chat) = desktop.apps.iter_mut().find(|a| a.id == AppId::HermesChat) {
                    chat.data.push_str(&alloc::format!(
                        "[UI] {} @{},{} {}x{}\n",
                        spec.title, spec.x, spec.y, spec.w, spec.h
                    ));
                    for w in &spec.widgets {
                        chat.data.push_str(&alloc::format!("  - {}: {}\n", w.kind, w.text));
                    }
                }
                // Also surface as Settings window content
                if let Some(settings) = desktop.apps.iter_mut().find(|a| a.id == AppId::Settings) {
                    settings.data = alloc::format!("{} | {}", spec.title,
                        spec.widgets.first().map(|w| w.text.as_str()).unwrap_or(""));
                    settings.visible = true;
                    settings.x = spec.x.max(0) as usize;
                    settings.y = spec.y.max(0) as usize;
                    settings.w = spec.w.max(120) as usize;
                    settings.h = spec.h.max(80) as usize;
                }
            }
            ui_spec::mark_ui_ok();
            k_nano::slog_jarbas!("ADR", "0047-H", "ui_spec applied title={}", spec.title);
        }
    }
}

#[derive(Clone, Copy)]
enum OverlayMode {
    HitlConfirm,
    HitlTerminal,
    MemoryNudge,
}

impl Agent for DisplayAgent {
    fn manifest(&self) -> &AgentManifest { &DISPLAY_MANIFEST }

    fn tick(&mut self, tick: u64, _count: u64) -> AgentTickResult {
        if !self.gpu_inited {
            // Não segurar GPU.lock() durante claim_graphics (spin::Mutex ≠ reentrante).
            let built = {
                let gpu = GPU.lock();
                gpu.as_ref().map(|gpu_dev| {
                    let fb = DoubleBuffer::from_gpu(gpu_dev);
                    let av = JarvisAvatar::new(gpu_dev);
                    (fb, av, gpu_dev.fb_width, gpu_dev.fb_height)
                })
            };
            if let Some((fb, av, fw, fh)) = built {
                let mut desktop = JarvisDesktop::new(fb);
                desktop.register_app(AppId::HermesChat, "Hermes Chat", Layer::HermesOverlay);
                desktop.register_app(AppId::Settings, "Settings", Layer::AppWindows);
                desktop.register_app(AppId::Power, "Power", Layer::AppWindows);
                desktop.register_app(AppId::Ide, "BitNet IDE", Layer::AppWindows);
                desktop.register_app(AppId::Camera, "Camera", Layer::AppWindows);
                desktop.register_app(AppId::AudioViz, "Audio Visualizer", Layer::AppWindows);
                desktop.ensure_hermes_overlay();
                // ADR-0058 S1/S2 self-tests (sem modelo) + cards demo (S4).
                let _ = crate::display::eg::self_test(&mut desktop.fb);
                let _ = crate::display::card::self_test();
                desktop.spawn_card(crate::display::card::demo_status_card());
                desktop.spawn_card(crate::display::card::demo_weather_card());
                desktop.spawn_card(crate::display::card::demo_call_card());
                k_nano::slog_jarbas!("UI", "info", "ADR-0058 cards demo: sistema + clima + chamada");
                *COMPOSITOR.lock() = Some(desktop);
                self.avatar = Some(av);
                // Limites + centro para IRQ mouse
                k_nano::interrupts::MOUSE_MAX_X.store(fw.saturating_sub(1), core::sync::atomic::Ordering::Release);
                k_nano::interrupts::MOUSE_MAX_Y.store(fh.saturating_sub(1), core::sync::atomic::Ordering::Release);
                k_nano::interrupts::MOUSE_ABS_X.store(fw / 2, core::sync::atomic::Ordering::Release);
                k_nano::interrupts::MOUSE_ABS_Y.store(fh / 2, core::sync::atomic::Ordering::Release);
                *MOUSE_X.lock() = (fw / 2) as usize;
                *MOUSE_Y.lock() = (fh / 2) as usize;
                crate::display::fb::claim_graphics();
                k_nano::slog_jarbas!("Jarbas", "info", "Desktop iniciado @ {}x{}", fw, fh);
                k_nano::interrupts::mouse_log_status("desktop_ready");
            }
            self.gpu_inited = true;
            return AgentTickResult::Pending;
        }

        // Poll mouse todo frame (IRQ pode ter atualizado MOUSE_ABS_* durante Hermes)
        k_nano::interrupts::mouse_poll_bytes();
        unsafe {
            let _ = k_nano::xhci::poll_mouse();
        }
        // Expira arm do OFF
        {
            let tick = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            if self.power_armed_until != 0 && tick > self.power_armed_until {
                self.power_armed_until = 0;
                *POWER_BANNER.lock() = None;
            }
        }
        let (mx, my, btn) = {
            use core::sync::atomic::Ordering;
            let mx = k_nano::interrupts::MOUSE_ABS_X.load(Ordering::Acquire) as usize;
            let my = k_nano::interrupts::MOUSE_ABS_Y.load(Ordering::Acquire) as usize;
            let btn = k_nano::interrupts::MOUSE_ABS_BTN.load(Ordering::Acquire);
            *MOUSE_X.lock() = mx;
            *MOUSE_Y.lock() = my;
            *MOUSE_BUTTONS.lock() = btn;
            (mx, my, btn)
        };
        // Clique confiável: edge no Display (nao so EventBus — pacotes intermediários
        // se perdem no AtomicU32 do LAST_MOUSE_PACKET).
        {
            use core::sync::atomic::Ordering;
            let prev = k_nano::interrupts::MOUSE_PREV_BTN.swap(btn, Ordering::AcqRel);
            let pressed = btn & !prev;
            if pressed != 0 {
                k_nano::interrupts::MOUSE_CLICK_FLASH.store(18, Ordering::Release);
                let hit = self.handle_pointer_click(pressed, mx, my);
                k_nano::slog_jarbas!(
                    "MOUSE",
                    "info",
                    "CLICK btn={:#x} @{}x{} hit={}",
                    pressed,
                    mx,
                    my,
                    hit
                );
            }
        }
        static STATUS_LOG: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
        let sn = STATUS_LOG.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        if sn < 5 || sn % 120 == 0 {
            k_nano::interrupts::mouse_log_status("display_tick");
        }

        // ADR-0047-H1: publish demo UI_SPEC once
        if !self.demo_ui_sent {
            let _ = EVENT_BUS.publish(event_bus::Event {
                id: 0,
                topic: alloc::string::String::from(TOPIC_UI_SPEC),
                payload: ui_spec::demo_ui_json().as_bytes().to_vec(),
                token: event_bus::CapabilityToken::Legacy(1),
            });
            self.demo_ui_sent = true;
        }

        // Generative UI specs
        while let Some(ev) = self.ui_receiver.try_receive() {
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            self.apply_ui_spec(text);
        }

        // Toast notifications — drain TOAST topic and render via compositor
        if let Some(ref mut desktop) = *COMPOSITOR.lock() {
            let now = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            while let Some(ev) = self.toast_receiver.try_receive() {
                let text = core::str::from_utf8(&ev.payload).unwrap_or("");
                if !text.is_empty() {
                    crate::clipboard_notify::toast_push(text);
                }
            }
            // Get active toasts and render as overlay
            let toasts = crate::clipboard_notify::toast_get_active(now);
            if !toasts.is_empty() {
                // Render toasts at bottom of screen
                let (w, h) = (desktop.w, desktop.h);
                let toast_h = 24;
                let start_y = h.saturating_sub(toast_h * toasts.len().min(4) + 10);
                for (i, msg) in toasts.iter().rev().take(4).enumerate() {
                    let y = start_y + i * toast_h;
                    // Semi-transparent background
                    desktop.fb.fill_rect(10, y, w.saturating_sub(20), toast_h - 4, 20, 25, 35);
                    // Border
                    desktop.fb.fill_rect(10, y, w.saturating_sub(20), 1, 80, 100, 120);
                    desktop.fb.fill_rect(10, y + toast_h - 5, w.saturating_sub(20), 1, 80, 100, 120);
                    // Text
                    crate::display::compositor::draw_text(&mut desktop.fb, 16, y + 4, msg, w, 200, 220, 255);
                }
            }
        }

        // H4: avatar telemetria from LatentBus norm + H2/H5 viz
        if self.latent_receiver.is_none() {
            self.latent_receiver =
                Some(k_nano::LATENT_BUS.subscribe(event_bus::TOPIC_THOUGHT_LLM));
        }
        if let Some(ref rx) = self.latent_receiver {
            while let Some(pkt) = rx.try_receive() {
            let norm = f32::from_bits(pkt.norm_bits);
            if let Some(ref mut avatar) = self.avatar {
                let st = if norm > 8.0 {
                    AvatarState::Speaking
                } else if norm > 2.0 {
                    AvatarState::Processing
                } else if norm > 0.1 {
                    AvatarState::Listening
                } else {
                    AvatarState::Idle
                };
                avatar.set_state(st);
                ui_spec::mark_avatar_telem();
            }
            if let Some(ref mut desktop) = *COMPOSITOR.lock() {
                let w = desktop.fb.info.width;
                let h = desktop.fb.info.height;
                let (x, y) = crate::display::embed_viz::latent_to_xy(&pkt.vec, w, h);
                crate::display::embed_viz::draw_embed_point(
                    &mut desktop.fb,
                    x,
                    y,
                    0x00_7F_CF,
                );
                crate::display::embed_viz::mark_h2();
                if norm > 0.5 {
                    crate::display::embed_viz::draw_thought_splat(
                        &mut desktop.fb,
                        x,
                        y,
                        8,
                        0xCF_7F_00,
                    );
                    crate::display::embed_viz::mark_h5();
                }
            }
            } // while pkt
        } // if latent_receiver

        // Keyboard echo
        while let Some(ev) = self.echo_receiver.try_receive() {
            self.input_buffer = core::str::from_utf8(&ev.payload).unwrap_or("").into();
        }

        // Hermes response
        while let Some(ev) = self.receiver.try_receive() {
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            if let Some(ref mut desktop) = *COMPOSITOR.lock() {
                if let Some(chat) = desktop.apps.iter_mut().find(|a| a.id == AppId::HermesChat) {
                    chat.data.push_str(text);
                    chat.data.push('\n');
                }
            }
        }

        // HITL / terminal / memory nudge → overlay Hermes
        Self::drain_hermes_overlay(
            &mut self.avatar,
            &mut self.hitl_receiver,
            OverlayMode::HitlConfirm,
        );
        Self::drain_hermes_overlay(
            &mut self.avatar,
            &mut self.hitl_term_receiver,
            OverlayMode::HitlTerminal,
        );
        Self::drain_hermes_overlay(
            &mut self.avatar,
            &mut self.memory_nudge_receiver,
            OverlayMode::MemoryNudge,
        );

        // App switching via keyboard commands
        if self.input_buffer.contains("[F1]") { if let Some(ref mut d) = *COMPOSITOR.lock() { d.toggle_app(AppId::HermesChat); }}
        if self.input_buffer.contains("[F2]") {
            if let Some(ref mut d) = *COMPOSITOR.lock() {
                d.toggle_app(AppId::Settings);
                if let Some(s) = d.apps.iter_mut().find(|a| a.id == AppId::Settings) { s.data.clear(); }
            }
        }
        if self.input_buffer.contains("[F3]") { if let Some(ref mut d) = *COMPOSITOR.lock() { d.toggle_app(AppId::Power); }}
        if self.input_buffer.contains("[F4]") { if let Some(ref mut d) = *COMPOSITOR.lock() { d.toggle_app(AppId::Ide); }}
        if self.input_buffer.contains("[F10]") { if let Some(ref mut d) = *COMPOSITOR.lock() { d.toggle_app(AppId::Camera); }}
        if self.input_buffer.contains("[F11]") { if let Some(ref mut d) = *COMPOSITOR.lock() { d.toggle_app(AppId::AudioViz); }}

        // Settings navigation
        if self.input_buffer.contains("[2]") || self.input_buffer.contains("sound") || self.input_buffer.contains("som") {
            if let Some(ref mut d) = *COMPOSITOR.lock() {
                if let Some(s) = d.apps.iter_mut().find(|a| a.id == AppId::Settings) {
                    s.data = alloc::string::String::from("[2] sound");
                }
            }
        }
        if self.input_buffer.contains("[B]") || self.input_buffer.contains("back") {
            if let Some(ref mut d) = *COMPOSITOR.lock() {
                if let Some(s) = d.apps.iter_mut().find(|a| a.id == AppId::Settings) {
                    s.data.clear();
                }
            }
        }
        // Volume controls
        if self.input_buffer.contains("+") {
            let v = crate::audio::settings::AUDIO_VOLUME.load(core::sync::atomic::Ordering::Relaxed);
            crate::audio::settings::AUDIO_VOLUME.store((v + 5).min(100), core::sync::atomic::Ordering::Relaxed);
        }
        if self.input_buffer.contains("-") {
            let v = crate::audio::settings::AUDIO_VOLUME.load(core::sync::atomic::Ordering::Relaxed);
            crate::audio::settings::AUDIO_VOLUME.store(v.saturating_sub(5), core::sync::atomic::Ordering::Relaxed);
        }

        // IDE: Generate WASM skill
        if self.input_buffer.contains("[GEN]") {
            let skill_name = {
                let mut comp = COMPOSITOR.lock();
                let mut name = alloc::string::String::new();
                if let Some(ref mut d) = *comp {
                    if let Some(ide) = d.apps.iter_mut().find(|a| a.id == AppId::Ide) {
                        name = alloc::string::String::from(ide.data.trim());
                        ide.data.clear();
                    }
                }
                drop(comp);
                name
            };
            if !skill_name.is_empty() {
                let mut comp2 = COMPOSITOR.lock();
                if let Some(ref mut d2) = *comp2 {
                    d2.publish_wasm_skill(&skill_name, &alloc::format!("WASM: {}", skill_name));
                    if let Some(chat) = d2.apps.iter_mut().find(|a| a.id == AppId::HermesChat) {
                        chat.data.push_str(&alloc::format!("[IDE] WASM '{}' published! Icon on desktop.\n", skill_name));
                    }
                }
            }
        }

        // ── Mouse input: atualiza cursor e processa clique ──
        while let Some(ev) = self.mouse_receiver.try_receive() {
            if ev.payload.len() >= 4 {
                let mx = u16::from_le_bytes([ev.payload[0], ev.payload[1]]) as usize;
                let my = u16::from_le_bytes([ev.payload[2], ev.payload[3]]) as usize;
                *MOUSE_X.lock() = mx;
                *MOUSE_Y.lock() = my;
            }
        }
        // EventBus MOUSE_CLICK: drenar só (UI já tratada no edge MOUSE_ABS_BTN).
        while self.click_receiver.try_receive().is_some() {}

        // Drag: solta no release do botão esquerdo; move enquanto pressionado.
        if self.dragging && self.drag_id == AppId::None {
            // ADR-0058 S3: arraste de card (estado no compositor).
            if (btn & 1) == 0 {
                self.dragging = false;
            }
            let mut comp = COMPOSITOR.lock();
            if let Some(ref mut desktop) = *comp {
                desktop.card_drag_step(mx as i32, my as i32, (btn & 1) != 0);
                desktop.card_resize_step(mx as i32, my as i32, (btn & 1) != 0);
            }
        } else if self.dragging {
            if (btn & 1) == 0 {
                self.dragging = false;
            } else {
                let mut comp = COMPOSITOR.lock();
                if let Some(ref mut desktop) = *comp {
                    let app = desktop
                        .apps
                        .iter_mut()
                        .find(|a| a.id == self.drag_id && a.visible);
                    if let Some(a) = app {
                        let nx = (mx as isize - self.drag_off_x).max(0) as usize;
                        let ny = (my as isize - self.drag_off_y).max(28) as usize;
                        a.x = nx.min(desktop.w.saturating_sub(100));
                        a.y = ny.min(desktop.h.saturating_sub(100));
                    } else {
                        self.dragging = false;
                    }
                }
            }
        }

        // Render desktop: orb circular no compositor (avatar partículas após clear interno)
        let mut comp = COMPOSITOR.lock();
        if let Some(ref mut desktop) = *comp {
            desktop.render(tick, self.avatar.as_mut());
        }
        drop(comp);

        self.input_buffer.clear();
        AgentTickResult::Pending
    }
}
