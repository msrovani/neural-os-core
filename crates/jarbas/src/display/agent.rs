//! DisplayAgent — desktop JARBAS: orb (brand) + mesh + HUD + cards on-demand.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use hermes;
use k_nano::EVENT_BUS;
use crate::display::fb::{DoubleBuffer, GPU};
use crate::display::compositor::{COMPOSITOR, JarbasDesktop, AppId, Layer, MOUSE_X, MOUSE_Y, MOUSE_BUTTONS, POWER_BANNER, POWER_STATE, PowerState, PowerDialogAction, hit_power_button};
use crate::display::avatar8::{Avatar8State, Avatar8};
use crate::display::ui_spec::{self, TOPIC_UI_SPEC};
use crate::display::shortcuts::{KeyCombo, Modifiers, WmAction, scancode_to_keycode};
use crate::display::gpu_backend;
use hermes::agents::TOPIC_KEY_EVENT;
use crate::clipboard_notify::TOPIC_TOAST;
use k_nano::net::mesh::TOPIC_MESH_HEALTH;
use k_nano::sync::IrqSafeLock;

// Simple JSON parser for MESH_HEALTH payload (no_std compatible)
mod mesh_health_json {
    use alloc::vec::Vec;

    #[derive(Debug, Clone)]
    pub struct PeerHealthJson {
        pub node_id: u8,
        pub reachable: bool,
        pub avg_rtt: u64,
        pub p99_rtt: u64,
        pub tx: u64,
        pub ack: u64,
        pub fail: u8,
        pub probe_to: u64,
    }

    /// Parse JSON array: [{"node_id":1,"reachable":true,"avg_rtt":10,"p99_rtt":20,"tx":100,"ack":90,"fail":0,"probe_to":50},...]
    pub fn parse(json: &str) -> Vec<PeerHealthJson> {
        let mut result = Vec::new();
        let mut i = 0;
        let bytes = json.as_bytes();
        let len = bytes.len();

        // Skip whitespace and '['
        while i < len && (bytes[i] == b' ' || bytes[i] == b'\n' || bytes[i] == b'\t' || bytes[i] == b'[') {
            i += 1;
        }

        while i < len {
            // Skip whitespace
            while i < len && (bytes[i] == b' ' || bytes[i] == b'\n' || bytes[i] == b'\t' || bytes[i] == b',') {
                i += 1;
            }
            if i >= len || bytes[i] == b']' {
                break;
            }
            if bytes[i] != b'{' {
                i += 1;
                continue;
            }

            // Parse object
            let mut node_id = 0u8;
            let mut reachable = false;
            let mut avg_rtt = 0u64;
            let mut p99_rtt = 0u64;
            let mut tx = 0u64;
            let mut ack = 0u64;
            let mut fail = 0u8;
            let mut probe_to = 0u64;

            i += 1; // skip '{'
            while i < len && bytes[i] != b'}' {
                // Parse key
                while i < len && (bytes[i] == b' ' || bytes[i] == b'\n' || bytes[i] == b'\t' || bytes[i] == b'"') {
                    i += 1;
                }
                let key_start = i;
                while i < len && bytes[i] != b'"' {
                    i += 1;
                }
                let key = if key_start < i {
                    core::str::from_utf8(&bytes[key_start..i]).unwrap_or("")
                } else { "" };
                while i < len && bytes[i] != b':' {
                    i += 1;
                }
                i += 1; // skip ':'

                // Parse value
                while i < len && (bytes[i] == b' ' || bytes[i] == b'\n' || bytes[i] == b'\t') {
                    i += 1;
                }
                let val_start = i;
                while i < len && bytes[i] != b',' && bytes[i] != b'}' {
                    i += 1;
                }
                let val_str = if val_start < i {
                    core::str::from_utf8(&bytes[val_start..i]).unwrap_or("")
                } else { "" };

                match key {
                    "node_id" => node_id = val_str.parse().unwrap_or(0),
                    "reachable" => reachable = val_str == "true",
                    "avg_rtt" => avg_rtt = val_str.parse().unwrap_or(0),
                    "p99_rtt" => p99_rtt = val_str.parse().unwrap_or(0),
                    "tx" => tx = val_str.parse().unwrap_or(0),
                    "ack" => ack = val_str.parse().unwrap_or(0),
                    "fail" => fail = val_str.parse().unwrap_or(0),
                    "probe_to" => probe_to = val_str.parse().unwrap_or(0),
                    _ => {}
                }
            }
            if i < len && bytes[i] == b'}' {
                i += 1;
            }
            result.push(PeerHealthJson {
                node_id, reachable, avg_rtt, p99_rtt, tx, ack, fail, probe_to,
            });
        }
        result
    }
}

/// Snapshot de um peer do mesh, consumido pelo compositor (draw_mesh_graph).
pub struct MeshPeerNode {
    pub node_id: u8,
    pub reachable: bool,
    pub avg_rtt: u32,
    pub p99_rtt: u32,
}
pub(crate) static MESH_GRAPH: IrqSafeLock<alloc::vec::Vec<MeshPeerNode>> =
    IrqSafeLock::new(alloc::vec::Vec::new());

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
    mesh_health_receiver: Option<event_bus::Receiver>,
    /// ADR-0086 A5: receiver para solicitação de UI de seleção de disco.
    install_ui_receiver: Option<event_bus::Receiver>,
    gpu_inited: bool,
    demo_ui_sent: bool,
    input_buffer: alloc::string::String,
    avatar: Option<Avatar8>,
    /// Current avatar state label for Jarbas palette override
    avatar_state_label: Option<&'static str>,
    dragging: bool,
    drag_id: AppId,
    drag_off_x: isize,
    drag_off_y: isize,
    /// Arm do botão OFF (tick até quando o 2º clique confirma).
    power_armed_until: usize,
    /// Última posição do mouse (para dirty_cursor só no movimento).
    last_pointer_x: usize,
    last_pointer_y: usize,
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
            mesh_health_receiver: None,
            install_ui_receiver: None,
            gpu_inited: false,
            demo_ui_sent: false,
            input_buffer: alloc::string::String::new(),
            avatar: None,
            avatar_state_label: None,
            dragging: false,
            drag_id: AppId::None,
            drag_off_x: 0,
            drag_off_y: 0,
            power_armed_until: 0,
            last_pointer_x: usize::MAX,
            last_pointer_y: usize::MAX,
        }
    }

    /// Trata clique em botão de card. Card 7902 = seleção de disco do instalador.
    fn handle_card_button(&mut self, card_id: u32, btn_idx: usize) {
        match card_id {
            7902 => {
                // Disk selection card: btn_idx → disco não-boot → DISK_SELECTION
                if let Some(disk_idx) = crate::cards::disk_selection_card::button_index_to_disk_index(btn_idx) {
                    k_nano::installer_agent::DISK_SELECTION.store(disk_idx as i8, core::sync::atomic::Ordering::Relaxed);
                    k_nano::slog_jarbas!("INSTALL", "info", "disco #{} selecionado via UI", disk_idx);
                    // Dispara instalação com o disco escolhido
                    let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
                        id: 0,
                        topic: alloc::string::String::from(k_nano::installer_agent::TOPIC_SYS_INSTALL),
                        payload: alloc::vec::Vec::new(),
                        token: event_bus::CapabilityToken::Legacy(1),
                    });
                }
            }
            _ => {}
        }
    }

    /// Drena receiver → overlay HITL/chat (spawn de janela, sem draw no tick).
    fn drain_hermes_overlay(
        avatar: &mut Option<Avatar8>,
        rx: &mut event_bus::Receiver,
        mode: OverlayMode,
    ) {
        for _ in 0..DRAIN_CAP {
            let Some(ev) = rx.try_receive() else { break; };
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            if let Some(ref mut desktop) = *COMPOSITOR.lock() {
                if crate::display::chat_window::chat_ui_enabled() {
                    desktop.show_app(AppId::HermesChat);
                }
                if let Some(chat) = desktop.windows.iter_mut().find(|w| w.app_id == Some(AppId::HermesChat)) {
                    match mode {
                        OverlayMode::HitlConfirm => {
                            chat.data.push_str("[HITL] Confirmacao necessaria\n");
                            chat.data.push_str(text);
                            chat.data.push('\n');
                            chat.data.push_str("Responda: /approve <id>  ou  /deny <id>\n");
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
                if matches!(mode, OverlayMode::HitlConfirm) {
                    spawn_or_update_hitl_card(desktop, text);
                }
            }
            {
                if crate::display::chat_window::chat_ui_enabled() {
                let mut cw = crate::display::chat_window::CHAT_WINDOW.lock();
                if cw.is_none() {
                    *cw = Some(crate::display::chat_window::ChatWindow::new(0));
                }
                if let Some(ref mut chat) = *cw {
                    chat.process_packet(hermes::stream_packet::StreamPacket::UserMessage {
                        content: alloc::format!("[{}] {}", overlay_tag(mode), text),
                    });
                }
                }
            }
            if matches!(mode, OverlayMode::HitlConfirm | OverlayMode::MemoryNudge) {
                if let Some(ref mut av) = avatar {
                    av.set_state(Avatar8State::Listening);
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

        // Hit-test real: dock → cards → janelas. Clique no orb/grafo = miss.
        let hit = {
            let mut comp = COMPOSITOR.lock();
            match comp.as_mut() {
                Some(d) => d.handle_desktop_click(cx as i32, cy as i32),
                None => "miss",
            }
        };
        if hit == "drag" || hit == "resize" || hit == "win:drag" {
            self.dragging = true;
            self.drag_id = AppId::None;
        }
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

const DRAIN_CAP: usize = 16;
const HITL_CARD_ID: u32 = 8001;

fn overlay_tag(mode: OverlayMode) -> &'static str {
    match mode {
        OverlayMode::HitlConfirm => "HITL",
        OverlayMode::HitlTerminal => "TERM",
        OverlayMode::MemoryNudge => "MEM",
    }
}

fn spawn_or_update_hitl_card(desktop: &mut crate::display::compositor::JarbasDesktop, text: &str) {
    let body = alloc::format!("{}", text);
    if let Some(win) = desktop.windows.iter_mut().find(|w| {
        matches!(&w.content, crate::display::window::WindowContent::Card(d) if d.id == HITL_CARD_ID)
    }) {
        if let crate::display::window::WindowContent::Card(d) = &mut win.content {
            d.body.clear();
            d.body.push(crate::display::card::Widget::Text(body));
            d.body.push(crate::display::card::Widget::Button(alloc::string::String::from("/approve")));
            d.body.push(crate::display::card::Widget::Button(alloc::string::String::from("/deny")));
        }
        win.visible = true;
        return;
    }
    let decl = crate::display::card::UiDeclaration::new(HITL_CARD_ID, "HITL", 72, 48, 440, 200)
        .push(crate::display::card::Widget::Text(body))
        .push(crate::display::card::Widget::Button(alloc::string::String::from("/approve")))
        .push(crate::display::card::Widget::Button(alloc::string::String::from("/deny")));
    desktop.spawn_card(decl);
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

    fn has_pending(&self) -> bool {
        self.receiver.has_pending()
            || self.hitl_receiver.has_pending()
            || self.hitl_term_receiver.has_pending()
            || self.memory_nudge_receiver.has_pending()
            || self.ui_receiver.has_pending()
            || self.toast_receiver.has_pending()
            || self.render_window_receiver.has_pending()
    }

    fn tick(&mut self, tick: u64, _count: u64) -> AgentTickResult {
        // SESSION_310: raw serial counter — visible even with slog filter
        static DISPLAY_TICKS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
        let dt = DISPLAY_TICKS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        if dt < 3 || dt % 500 == 0 {
            unsafe fn dsp_putc(c: u8) {
                core::arch::asm!("out dx, al", in("dx") 0x3F8u16, in("al") c, options(nostack, preserves_flags));
            }
            unsafe fn dsp_putdec(mut n: u64) {
                if n == 0 { dsp_putc(b'0'); return; }
                let mut buf = [0u8; 20];
                let mut i = 20;
                while n > 0 { i -= 1; buf[i] = (n % 10) as u8 + b'0'; n /= 10; }
                for &b in &buf[i..] { dsp_putc(b); }
            }
            unsafe {
                for &c in b"[DSP_TICK] " { dsp_putc(c); }
                dsp_putdec(dt);
                dsp_putc(b'\n');
            }
        }
        if !self.gpu_inited {
            // Initialize GPU backend (k_hal GPU BE) — check compute state
            if let Err(e) = gpu_backend::init_gpu_backend() {
                k_nano::slog_jarbas!("GPU", "init", "backend init: {}", e);
            } else {
                k_nano::slog_jarbas!(
                    "GPU",
                    "init",
                    "canary Ready / copy-engine; device math = None until KernelPack"
                );
            }

            // Não segurar GPU.lock() durante claim_graphics (spin::Mutex ≠ reentrante).
            let built = {
                let gpu = GPU.lock();
                gpu.as_ref().map(|gpu_dev| {
                    let fb = DoubleBuffer::from_gpu(gpu_dev);
                    let av = Avatar8::new(gpu_dev.fb_width as usize, gpu_dev.fb_height as usize);
                    (fb, av, gpu_dev.fb_width, gpu_dev.fb_height)
                })
            };
            if let Some((fb, av, fw, fh)) = built {
                let mut desktop = JarbasDesktop::new(fb);
                if crate::display::chat_window::chat_ui_enabled() {
                    desktop.register_app(AppId::HermesChat, "Jarbas Chat", Layer::AppWindows);
                }
                k_nano::slog_jarbas!("UI", "info", "Desktop limpo — orb + HUD");
                *COMPOSITOR.lock() = Some(desktop);
                self.avatar = Some(av);
                // Limites + centro para IRQ mouse
                k_nano::interrupts::MOUSE_MAX_X.store(fw.saturating_sub(1), core::sync::atomic::Ordering::Release);
                k_nano::interrupts::MOUSE_MAX_Y.store(fh.saturating_sub(1), core::sync::atomic::Ordering::Release);
                k_nano::interrupts::MOUSE_ABS_X.store(fw / 2, core::sync::atomic::Ordering::Release);
                k_nano::interrupts::MOUSE_ABS_Y.store(fh / 2, core::sync::atomic::Ordering::Release);
                MOUSE_X.store((fw / 2) as usize, core::sync::atomic::Ordering::Relaxed);
                MOUSE_Y.store((fh / 2) as usize, core::sync::atomic::Ordering::Relaxed);
                crate::display::fb::claim_graphics();
                // Bisector v2 (s319): scheduler carimba o agente em curso
                // direto no FB — frame congelado mostra o agente travado.
                agent_core::set_tick_stamp_fn(Some(crate::display::fb::diag_stamp_agent));
                // Bisector v3 (s320): exceções (#UD/#GP/#PF) estampadas no FB —
                // o dump serial é invisível no metal; hlt loop = freeze.
                k_nano::interrupts::set_exception_fb_fn(Some(
                    crate::display::fb::diag_stamp_exception,
                ));
                // Bisector v4 (s321): sub-estágios do tick em curso (linha 2
                // de barras, y=32) — agentes marcam progresso via tick_stage.
                agent_core::set_tick_stage_fn(Some(crate::display::fb::diag_stage_row1));
                // 1º frame imediato: splash no tick 1; sem render+swap aqui depende do tick 2
                // (Hermes/LLM pode bloquear minutos — SESSION_168 / HW real freeze no splash).
                if let Some(ref mut desktop) = *COMPOSITOR.lock() {
                    desktop.invalidate_all();
                    // Animação usa tick do scheduler: se o IRQ timer falhar, a UI
                    // continua responsiva enquanto o runtime ainda progride.
                    desktop.render(tick, self.avatar.as_mut(), self.avatar_state_label);
                }
                k_nano::boot_logger::mark_ui_live();
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
        crate::display::fb::diag_mark(1);
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
            MOUSE_X.store(mx, core::sync::atomic::Ordering::Relaxed);
            MOUSE_Y.store(my, core::sync::atomic::Ordering::Relaxed);
            MOUSE_BUTTONS.store(btn, core::sync::atomic::Ordering::Relaxed);
            (mx, my, btn)
        };
        if mx != self.last_pointer_x || my != self.last_pointer_y {
            self.last_pointer_x = mx;
            self.last_pointer_y = my;
            if let Some(ref mut desktop) = *COMPOSITOR.lock() {
                desktop.invalidate_cursor();
            }
        }
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
        if sn < 2 {
            k_nano::interrupts::mouse_log_status("display_tick");
        }

        // Demo UI_SPEC removido do boot — viewport limpo (orb + HUD).
        // Cards sob demanda via EventBus UI_SPEC / instalador / atalhos.
        if !self.demo_ui_sent {
            ui_spec::mark_ui_ok();
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
                match &pkt {
                    hermes::stream_packet::StreamPacket::ReasoningStart
                    | hermes::stream_packet::StreamPacket::MessageStart { .. } => {
                        crate::display::console::set_llm_busy(true);
                    }
                    hermes::stream_packet::StreamPacket::Stop
                    | hermes::stream_packet::StreamPacket::Error { .. } => {
                        crate::display::console::set_llm_busy(false);
                    }
                    _ => {}
                }
                let mut cw = crate::display::chat_window::CHAT_WINDOW.lock();
                if crate::display::chat_window::chat_ui_enabled() {
                if cw.is_none() {
                    *cw = Some(crate::display::chat_window::ChatWindow::new(0));
                }
                if let Some(ref mut chat) = *cw {
                    chat.process_packet(pkt);
                }
                }
            }
        }

        // ── USER_INTENT: registra mensagem do usuário no ChatWindow ──
        while let Some(ev) = self.user_intent_receiver.try_receive() {
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            if !text.is_empty() && crate::display::chat_window::chat_ui_enabled() {
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
            if !text.is_empty() && crate::display::chat_window::chat_ui_enabled() {
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
        crate::display::fb::diag_mark(2);

        // ── RENDER_WINDOW: snapshot no overlay (render() pinta) ──
        for _ in 0..DRAIN_CAP {
            let Some(ev) = self.render_window_receiver.try_receive() else { break; };
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            if let Some((name, data)) = text.split_once('|') {
                let (w, h) = {
                    let comp = COMPOSITOR.lock();
                    comp.as_ref().map(|d| (d.w, d.h)).unwrap_or((1280, 800))
                };
                let rect = crate::display::tiling::Rect {
                    x: 60, y: 60,
                    width: w.saturating_sub(120) as u32,
                    height: h.saturating_sub(120) as u32,
                };
                crate::display::overlay::set_render_overlay(name, data.as_bytes(), rect);
                k_nano::slog_jarbas!("RENDER", "info", "overlay '{}' ({} bytes)", name, data.len());
            }
        }

        // Toast → NotificationQueue (pintado no render, SESSION_261).
        {
            let now = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
            for _ in 0..DRAIN_CAP {
                let Some(ev) = self.toast_receiver.try_receive() else { break; };
                let text = core::str::from_utf8(&ev.payload).unwrap_or("");
                if text.is_empty() { continue; }
                crate::clipboard_notify::toast_push(text);
                if let Some(ref mut desktop) = *COMPOSITOR.lock() {
                    desktop.notifications.push(
                        text,
                        "toast",
                        crate::display::notifications::Urgency::Normal,
                        None,
                        now,
                    );
                }
            }
        }

        // HITL / HERMES_RESPONSE / memory — teto por tick (fila EventBus unbounded).
        Self::drain_hermes_overlay(&mut self.avatar, &mut self.hitl_receiver, OverlayMode::HitlConfirm);
        Self::drain_hermes_overlay(&mut self.avatar, &mut self.hitl_term_receiver, OverlayMode::HitlTerminal);
        Self::drain_hermes_overlay(&mut self.avatar, &mut self.memory_nudge_receiver, OverlayMode::MemoryNudge);
        for _ in 0..DRAIN_CAP {
            let Some(ev) = self.receiver.try_receive() else { break; };
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            crate::display::console::set_llm_busy(false);
            if text.is_empty() { continue; }
            if !crate::display::chat_window::chat_ui_enabled() {
                continue;
            }
            if let Some(ref mut desktop) = *COMPOSITOR.lock() {
                desktop.show_app(AppId::HermesChat);
            }
            let mut cw = crate::display::chat_window::CHAT_WINDOW.lock();
            if cw.is_none() {
                *cw = Some(crate::display::chat_window::ChatWindow::new(0));
            }
            if let Some(ref mut chat) = *cw {
                chat.process_packet(hermes::stream_packet::StreamPacket::MessageStart {
                    pre_answer_seconds: None,
                });
                chat.process_packet(hermes::stream_packet::StreamPacket::MessageDelta {
                    content: alloc::string::String::from(text),
                });
                chat.process_packet(hermes::stream_packet::StreamPacket::Stop);
            }
        }

        // H4: avatar telemetria from LatentBus norm + H2/H5 viz
        if self.latent_receiver.is_none() {
            self.latent_receiver =
                Some(k_nano::LATENT_BUS.subscribe(event_bus::TOPIC_THOUGHT_LLM));
        }
        if let Some(ref rx) = self.latent_receiver {
            let mut drained = 0;
            while drained < DRAIN_CAP {
                let Some(pkt) = rx.try_receive() else { break; };
                drained += 1;
            let norm = f32::from_bits(pkt.norm_bits);
            if let Some(ref mut avatar8) = self.avatar {
                let st = if norm > 8.0 {
                    Avatar8State::Speaking
                } else if norm > 2.0 {
                    Avatar8State::Processing
                } else if norm > 0.1 {
                    Avatar8State::Listening
                } else {
                    Avatar8State::Idle
                };
                avatar8.set_state(st);
                self.avatar_state_label = Some(st.label());
                ui_spec::mark_avatar_telem();
            }
            if let Some(ref mut desktop) = *COMPOSITOR.lock() {
                let w = desktop.fb.info.width;
                let h = desktop.fb.info.height;
                let (x, y) = crate::display::embed_viz::latent_to_xy(&pkt.vec, w, h);
                crate::display::overlay::push_embed(crate::display::overlay::EmbedMark {
                    x, y, color: 0x00_7F_CF, splat: false,
                });
                crate::display::embed_viz::mark_h2();
                if norm > 0.5 {
                    crate::display::overlay::push_embed(crate::display::overlay::EmbedMark {
                        x, y, color: 0xCF_7F_00, splat: true,
                    });
                    crate::display::embed_viz::mark_h5();
                }
            }
            } // while pkt
        } // if latent_receiver

        // Mesh Health: drena MESH_HEALTH (JSON) → MESH_GRAPH (compositor renderiza).
        if self.mesh_health_receiver.is_none() {
            self.mesh_health_receiver = Some(EVENT_BUS.subscribe(TOPIC_MESH_HEALTH));
        }
        if let Some(ref rx) = self.mesh_health_receiver {
            let mut drained = 0;
            while drained < DRAIN_CAP {
                let Some(ev) = rx.try_receive() else { break; };
                drained += 1;
                let json_str = core::str::from_utf8(&ev.payload).unwrap_or("");
                let peers = mesh_health_json::parse(json_str);
                let mut graph = MESH_GRAPH.lock();
                graph.clear();
                for health in peers {
                    graph.push(MeshPeerNode {
                        node_id: health.node_id,
                        reachable: health.reachable,
                        avg_rtt: health.avg_rtt as u32,
                        p99_rtt: health.p99_rtt as u32,
                    });
                }
                drop(graph);
            }
        }
        crate::display::fb::diag_mark(3);

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
                        WmAction::OpenChat => { desktop.toggle_app(AppId::HermesChat); }
                        WmAction::PowerMenu => { desktop.open_power_dialog(); }
                        WmAction::ShowHelp => {
                            let decl = crate::display::card::UiDeclaration::new(
                                9999, "Keyboard Shortcuts", 60, 40, 480, 360,
                            ).push(crate::display::card::Widget::Text(
                                alloc::string::String::from(crate::display::shortcuts::help_text())
                            ));
                            desktop.spawn_card(decl);
                        }
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
            if crate::display::chat_window::chat_ui_enabled() {
            let mut cw = crate::display::chat_window::CHAT_WINDOW.lock();
            if let Some(ref mut chat) = *cw {
                chat.input_buffer = alloc::string::String::from(text);
                chat.input_cursor = text.len();
                chat.dirty = true;
            }
            }
        }

        // ── Mouse input: atualiza cursor e processa clique ──
        while let Some(ev) = self.mouse_receiver.try_receive() {
            if ev.payload.len() >= 4 {
                let mx = u16::from_le_bytes([ev.payload[0], ev.payload[1]]) as usize;
                let my = u16::from_le_bytes([ev.payload[2], ev.payload[3]]) as usize;
                MOUSE_X.store(mx, core::sync::atomic::Ordering::Relaxed);
                MOUSE_Y.store(my, core::sync::atomic::Ordering::Relaxed);
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
                desktop.window_drag_step(mx as i32, my as i32, (btn & 1) != 0);
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

        // ── SYS_INSTALL_UI: solicitação de UI de seleção de disco (ADR-0086 A5) ──
        if self.install_ui_receiver.is_none() {
            self.install_ui_receiver = Some(EVENT_BUS.subscribe(k_nano::installer_agent::TOPIC_SYS_INSTALL_UI));
        }
        if let Some(ref rx) = self.install_ui_receiver {
            while let Some(_ev) = rx.try_receive() {
                if let Some(ref mut desktop) = *COMPOSITOR.lock() {
                    let decl = crate::cards::disk_selection_card::disk_selection_card();
                    desktop.spawn_card(decl);
                    k_nano::slog_jarbas!("INSTALL", "info", "card de selecao de disco spawnado");
                }
            }
        }

        crate::display::fb::diag_mark(4);
        // Render desktop: orb circular no compositor (avatar partículas após clear interno)
        let mut comp = COMPOSITOR.lock();
        if let Some(ref mut desktop) = *comp {
            // Consome clique em botão de card (ex: seleção de disco do instalador).
            if let Some((card_id, btn_idx)) = desktop.take_card_hit_button() {
                self.handle_card_button(card_id, btn_idx);
            }
            // Render/liveness não dependem do LAPIC/PIT; relógio do dock lê
            // TIMER_TICKS separadamente. Assim mouse/orb não congelam se o
            // timer de parede degradar, mas o scheduler continuar acordando.
            desktop.render(tick, self.avatar.as_mut(), self.avatar_state_label);
        }
        drop(comp);

        self.input_buffer.clear();
        AgentTickResult::Pending
    }
}
