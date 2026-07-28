//! DisplayAgent — JARVIS Desktop com compositor multi-app + WM cosmic-like.
//! Hermes Chat + Settings + Power + JARVIS avatar overlay.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use hermes;
use k_nano::EVENT_BUS;
use crate::display::fb::{DoubleBuffer, GPU};
use crate::display::compositor::{COMPOSITOR, JarbasDesktop, AppId, Layer, MOUSE_X, MOUSE_Y, MOUSE_BUTTONS, POWER_BANNER, POWER_STATE, PowerState, PowerDialogAction, hit_power_button};
use crate::display::avatar::{AvatarState, JarbasAvatar};
use crate::display::ui_spec::{self, TOPIC_UI_SPEC};
use crate::display::shortcuts::{KeyCombo, Modifiers, WmAction, scancode_to_keycode};
use hermes::agents::TOPIC_KEY_EVENT;
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
    user_intent_receiver: event_bus::Receiver,
    stt_text_receiver: event_bus::Receiver,
    render_receiver: event_bus::Receiver,
    render_window_receiver: event_bus::Receiver,
    mouse_receiver: event_bus::Receiver,
    click_receiver: event_bus::Receiver,
    ui_receiver: event_bus::Receiver,
    hitl_receiver: event_bus::Receiver,
    hitl_term_receiver: event_bus::Receiver,
    memory_nudge_receiver: event_bus::Receiver,
    toast_receiver: event_bus::Receiver,
    /// FIX 1: subscreve KEY_EVENT do InputAgent para dispatch de atalhos WM.
    key_event_receiver: event_bus::Receiver,
    latent_receiver: Option<event_bus::LatentReceiver>,
    llm_stream_receiver: event_bus::Receiver,
    gpu_inited: bool,
    demo_ui_sent: bool,
    input_buffer: alloc::string::String,
    avatar: Option<JarbasAvatar>,
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
            user_intent_receiver: EVENT_BUS.subscribe(hermes::hermes::TOPIC_USER_INTENT),
            mouse_receiver: EVENT_BUS.subscribe(hermes::agents::mouse_agent::TOPIC_MOUSE_MOVED),
            click_receiver: EVENT_BUS.subscribe(hermes::agents::mouse_agent::TOPIC_MOUSE_CLICK),
            ui_receiver: EVENT_BUS.subscribe(TOPIC_UI_SPEC),
            hitl_receiver: EVENT_BUS.subscribe(hermes::hitl_ui::TOPIC_HITL_REQUEST),
            hitl_term_receiver: EVENT_BUS.subscribe(hermes::hitl_ui::TOPIC_HITL_TERMINAL),
            memory_nudge_receiver: EVENT_BUS.subscribe(hermes::cognitive_bridge::TOPIC_MEMORY_NUDGE),
            toast_receiver: EVENT_BUS.subscribe(TOPIC_TOAST),
            key_event_receiver: EVENT_BUS.subscribe(TOPIC_KEY_EVENT),
            llm_stream_receiver: EVENT_BUS.subscribe(hermes::stream_packet::TOPIC_LLM_STREAM),
            stt_text_receiver: EVENT_BUS.subscribe(crate::audio::TOPIC_STT_TEXT),
            render_receiver: EVENT_BUS.subscribe(crate::display::render_registry::TOPIC_RENDER_REGISTER),
            render_window_receiver: EVENT_BUS.subscribe(crate::display::render_registry::TOPIC_RENDER_WINDOW),
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
        avatar: &mut Option<JarbasAvatar>,
        rx: &mut event_bus::Receiver,
        mode: OverlayMode,
    ) {
        while let Some(ev) = rx.try_receive() {
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            if let Some(ref mut desktop) = *COMPOSITOR.lock() {
                desktop.ensure_hermes_overlay();
                if let Some(chat) = desktop.windows.iter_mut().find(|w| w.app_id == Some(AppId::HermesChat)) {
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

        // Se o diálogo de energia está aberto, processa cliques
        if dialog_open {
            let action = crate::display::compositor::hit_power_dialog(cx, cy, scr_w, scr_h);
            match action {
                PowerDialogAction::Cancel | PowerDialogAction::None => {
                    // Cancel ou clique fora → fecha diálogo
                    COMPOSITOR.lock().as_mut().map(|d| d.close_power_dialog());
                    *POWER_BANNER.lock() = None;
                    *POWER_STATE.lock() = PowerState::None;
                    self.power_armed_until = 0;
                    let tag = if action == PowerDialogAction::Cancel { "power_cancel" } else { "power_outside" };
                    k_nano::slog_jarbas!("JARBAS", "POWER", "dialog CANCELADO ({})", tag);
                    return tag;
                }
                PowerDialogAction::ShutDown => {
                    COMPOSITOR.lock().as_mut().map(|d| d.close_power_dialog());
                    *POWER_STATE.lock() = PowerState::ShuttingDown;
                    *POWER_BANNER.lock() = Some("Desligando...");
                    k_nano::slog_jarbas!("JARBAS", "POWER", "DESLIGAR — publicando SYSTEM_SHUTDOWN");
                    let _ = EVENT_BUS.publish(event_bus::Event {
                        id: 0,
                        topic: alloc::string::String::from("SYSTEM_SHUTDOWN"),
                        payload: b"ui_off".to_vec(),
                        token: event_bus::CapabilityToken::Legacy(1),
                    });
                    self.power_armed_until = 0;
                    return "power_shutdown";
                }
                PowerDialogAction::Hibernate => {
                    COMPOSITOR.lock().as_mut().map(|d| d.close_power_dialog());
                    *POWER_STATE.lock() = PowerState::Hibernating;
                    *POWER_BANNER.lock() = Some("Hibernando...");
                    k_nano::slog_jarbas!("JARBAS", "POWER", "HIBERNAR — publicando SYSTEM_HIBERNATE");
                    let _ = EVENT_BUS.publish(event_bus::Event {
                        id: 0,
                        topic: alloc::string::String::from("SYSTEM_HIBERNATE"),
                        payload: b"ui_off".to_vec(),
                        token: event_bus::CapabilityToken::Legacy(1),
                    });
                    self.power_armed_until = 0;
                    return "power_hibernate";
                }
                PowerDialogAction::Reboot => {
                    COMPOSITOR.lock().as_mut().map(|d| d.close_power_dialog());
                    *POWER_STATE.lock() = PowerState::Rebooting;
                    *POWER_BANNER.lock() = Some("Reiniciando...");
                    k_nano::slog_jarbas!("JARBAS", "POWER", "REINICIAR — publicando SYSTEM_REBOOT");
                    let _ = EVENT_BUS.publish(event_bus::Event {
                        id: 0,
                        topic: alloc::string::String::from("SYSTEM_REBOOT"),
                        payload: b"ui_off".to_vec(),
                        token: event_bus::CapabilityToken::Legacy(1),
                    });
                    self.power_armed_until = 0;
                    return "power_reboot";
                }
            }
        }

        // OFF canto SD — abre diálogo de energia
        if hit_power_button(cx, cy, scr_w) {
            COMPOSITOR.lock().as_mut().map(|d| d.open_power_dialog());
            *POWER_STATE.lock() = PowerState::Dialog;
            *POWER_BANNER.lock() = None;
            k_nano::slog_jarbas!("JARBAS", "POWER", "dialog ABERTO");
            return "power_dialog_open";
        }
        // Clique fora desarma banner antigo (se houver)
        if self.power_armed_until != 0 {
            self.power_armed_until = 0;
            *POWER_BANNER.lock() = None;
        }

        // FIX 11: Notification click — testa antes do dock/app.
        if (btn & 1) != 0 {
            let notif_hit = {
                let comp = COMPOSITOR.lock();
                comp.as_ref().and_then(|d| d.notifications.hit_test(cx, cy, scr_w))
            };
            if let Some(notif_id) = notif_hit {
                let mut comp = COMPOSITOR.lock();
                if let Some(ref mut d) = *comp {
                    d.notifications.handle_click(notif_id);
                    d.notifications.dismiss(notif_id);
                }
                return "notification";
            }
        }

        // ── FocusMode: clique no painel esquerdo = Chat, fora = Ambient ──
        let left_w = scr_w * 35 / 100;
        if cx < left_w {
            *crate::display::compositor::FOCUS_MODE.lock() = crate::display::compositor::FocusMode::Chat;
            // Repassa clique pro ChatWindow (handle_click para toggle mic, etc.)
            let mut cw = crate::display::chat_window::CHAT_WINDOW.lock();
            if let Some(ref mut chat) = *cw {
                chat.handle_click(cx, cy, 2, 0, left_w.saturating_sub(4), scr_h);
            }
            return "focus:chat";
        } else {
            *crate::display::compositor::FOCUS_MODE.lock() = crate::display::compositor::FocusMode::Ambient;
            return "focus:ambient";
        }
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
                if let Some(chat) = desktop.windows.iter_mut().find(|w| w.app_id == Some(AppId::HermesChat)) {
                    chat.data.push_str(&alloc::format!(
                        "[UI] {} @{},{} {}x{}\n",
                        spec.title, spec.x, spec.y, spec.w, spec.h
                    ));
                    for w in &spec.widgets {
                        chat.data.push_str(&alloc::format!("  - {}: {}\n", w.kind, w.text));
                    }
                }
                // Also surface as Settings window content
                if let Some(settings) = desktop.windows.iter_mut().find(|w| w.app_id == Some(AppId::Settings)) {
                    settings.data = alloc::format!("{} | {}", spec.title,
                        spec.widgets.first().map(|w| w.text.as_str()).unwrap_or(""));
                    settings.visible = true;
                    settings.rect.x = spec.x.max(0);
                    settings.rect.y = spec.y.max(0);
                    settings.rect.width = spec.w.max(120) as u32;
                    settings.rect.height = spec.h.max(80) as u32;
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

// === Keyboard shortcut dispatch (ADR-0065 FASE 1.1 — FIX 1) ===
//
// FIX 1: parse_input_to_keycombo era broken (shortcut_to_text retornava None sempre).
// Substituído por dispatch direto via KEY_EVENT topic do InputAgent.
// Payload: [scancode, ctrl, alt, shift, super_key, pressed]
fn dispatch_key_event(payload: &[u8]) -> Option<WmAction> {
    if payload.len() < 6 { return None; }
    let scancode = payload[0];
    let ctrl = payload[1] != 0;
    let alt = payload[2] != 0;
    let shift = payload[3] != 0;
    let super_key = payload[4] != 0;
    let pressed = payload[5] != 0;
    if !pressed { return None; }
    let key = scancode_to_keycode(scancode)?;
    let combo = KeyCombo {
        modifiers: Modifiers { super_key, ctrl, alt, shift },
        key,
    };
    WmAction::from_keycombo(combo)
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
                    let av = JarbasAvatar::new(gpu_dev);
                    (fb, av, gpu_dev.fb_width, gpu_dev.fb_height)
                })
            };
            if let Some((fb, av, fw, fh)) = built {
                let mut desktop = JarbasDesktop::new(fb);
                desktop.register_app(AppId::HermesChat, "Jarbas Chat", Layer::AppWindows);
                desktop.ensure_hermes_overlay();
                k_nano::slog_jarbas!("UI", "info", "Jarbas Chat — desktop 3 painéis");
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

        // ── LLM_STREAM: processa pacotes streaming no ChatWindow ──
        while let Some(ev) = self.llm_stream_receiver.try_receive() {
            if let Some(pkt) = hermes::stream_packet::StreamPacket::decode(&ev.payload) {
                let mut cw = crate::display::chat_window::CHAT_WINDOW.lock();
                if cw.is_none() {
                    *cw = Some(crate::display::chat_window::ChatWindow::new(0));
                }
                if let Some(ref mut chat) = *cw {
                    chat.process_packet(pkt);
                }
            }
        }

        // ── USER_INTENT: registra mensagem do usuário no ChatWindow ──
        while let Some(ev) = self.user_intent_receiver.try_receive() {
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            if !text.is_empty() {
                let mut cw = crate::display::chat_window::CHAT_WINDOW.lock();
                if cw.is_none() {
                    *cw = Some(crate::display::chat_window::ChatWindow::new(0));
                }
                if let Some(ref mut chat) = *cw {
                    chat.process_packet(hermes::stream_packet::StreamPacket::UserMessage {
                        content: alloc::string::String::from(text),
                    });
                }
            }
        }

        // ── STT_TEXT: transcrição de voz → input buffer do ChatWindow ──
        while let Some(ev) = self.stt_text_receiver.try_receive() {
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            if !text.is_empty() {
                let mut cw = crate::display::chat_window::CHAT_WINDOW.lock();
                if cw.is_none() {
                    *cw = Some(crate::display::chat_window::ChatWindow::new(0));
                }
                if let Some(ref mut chat) = *cw {
                    chat.input_buffer = alloc::string::String::from(text);
                    chat.input_cursor = text.len();
                    chat.dirty = true;
                }
            }
        }

        // ── RENDER_REGISTER: skills de renderização dinâmica ──
        while let Some(ev) = self.render_receiver.try_receive() {
            crate::display::render_registry::process_event(
                crate::display::render_registry::TOPIC_RENDER_REGISTER,
                &ev.payload,
            );
        }

        // ── RENDER_WINDOW: executa skill de render no framebuffer ──
        while let Some(ev) = self.render_window_receiver.try_receive() {
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            if let Some((name, data)) = text.split_once('|') {
                let registry = crate::display::render_registry::RENDER_REGISTRY.lock();
                // Obtém framebuffer + rect do compositor
                let mut comp = COMPOSITOR.lock();
                if let Some(ref mut desktop) = *comp {
                    let rect = crate::display::tiling::Rect {
                        x: 60, y: 60,
                        width: (desktop.w.saturating_sub(120)) as u32,
                        height: (desktop.h.saturating_sub(120)) as u32,
                    };
                    let theme = crate::display::theme::current_theme();
                    if registry.render(name, &mut desktop.fb, rect, &theme, data.as_bytes()) {
                        k_nano::slog_jarbas!("RENDER", "info", "window '{}' ({} bytes)", name, data.len());
                    } else {
                        k_nano::slog_jarbas!("RENDER", "warn", "skill '{}' nao registrada", name);
                    }
                }
            }
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

        // Process keyboard shortcuts via WmAction dispatch (ADR-0065 FASE 1.1 — FIX 1)
        // Drena KEY_EVENT do InputAgent (payload: [scancode, ctrl, alt, shift, super_key, pressed]).
        while let Some(ev) = self.key_event_receiver.try_receive() {
            if let Some(action) = dispatch_key_event(&ev.payload) {
                let mut comp = COMPOSITOR.lock();
                if let Some(ref mut desktop) = *comp {
                    match action {
                        WmAction::WorkspaceSwitch(idx) => { desktop.workspaces.switch(idx); }
                        WmAction::WorkspacePrev => { desktop.workspaces.prev(); }
                        WmAction::WorkspaceNext => { desktop.workspaces.next(); }
                        WmAction::WorkspacePrevious => { desktop.workspaces.switch_previous(); }
                        WmAction::CycleWindow => { desktop.cycle_focus(false); }
                        WmAction::CycleWindowReverse => { desktop.cycle_focus(true); }
                        WmAction::CloseWindow => { desktop.close_focused_window(); }
                        WmAction::MaximizeWindow => { desktop.maximize_focused(); }
                        WmAction::MinimizeWindow => { desktop.minimize_focused(); }
                        WmAction::ToggleTiling => { desktop.tiling_enabled = !desktop.tiling_enabled; }
                        WmAction::ToggleDock => { desktop.toggle_dock(); }
                        WmAction::ToggleFloating => { desktop.toggle_floating_focused(); }
                        WmAction::TileSplitHorizontal => { desktop.split_focused(crate::display::tiling::SplitDirection::Right); }
                        WmAction::TileSplitVertical => { desktop.split_focused(crate::display::tiling::SplitDirection::Down); }
                        WmAction::TileResizeLeft => { desktop.resize_split_focused(-20); }
                        WmAction::TileResizeRight => { desktop.resize_split_focused(20); }
                        WmAction::TileResizeUp => { desktop.resize_split_focused(-20); }
                        WmAction::TileResizeDown => { desktop.resize_split_focused(20); }
                        WmAction::LaunchApp(a) => { desktop.toggle_app(a); }
                        WmAction::ShowLauncher => { desktop.toggle_app(AppId::HermesChat); }
                    }
                    k_nano::slog_jarbas!("WM", "info", "action={:?}", action);
                }
            }
        }

        // Echo buffer — KEYBOARD_ECHO contém o buffer completo do input.
        // Sincroniza com o ChatWindow para exibição.
        while let Some(ev) = self.echo_receiver.try_receive() {
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            self.input_buffer = alloc::string::String::from(text);
            let mut cw = crate::display::chat_window::CHAT_WINDOW.lock();
            if let Some(ref mut chat) = *cw {
                chat.input_buffer = alloc::string::String::from(text);
                chat.input_cursor = text.len();
                chat.dirty = true;
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
                    let win = desktop
                        .windows
                        .iter_mut()
                        .find(|w| w.app_id == Some(self.drag_id) && w.visible);
                    if let Some(w) = win {
                        let nx = (mx as isize - self.drag_off_x).max(0) as i32;
                        let ny = (my as isize - self.drag_off_y).max(28) as i32;
                        w.rect.x = nx.min(desktop.w.saturating_sub(100) as i32);
                        w.rect.y = ny.min(desktop.h.saturating_sub(100) as i32);
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
