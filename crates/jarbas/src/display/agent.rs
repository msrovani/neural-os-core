//! DisplayAgent — JARVIS Desktop com compositor multi-app.
//! Hermes Chat + Settings + Power + JARVIS avatar overlay.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use hermes;
use k_nano::serial_println;
use k_nano::EVENT_BUS;
use crate::display::fb::{DoubleBuffer, GPU};
use crate::display::compositor::{COMPOSITOR, JarvisDesktop, AppId, Layer, MOUSE_X, MOUSE_Y, MOUSE_BUTTONS};
use crate::display::avatar::{AvatarState, JarvisAvatar};
use crate::display::ui_spec::{self, TOPIC_UI_SPEC};

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
    latent_receiver: event_bus::LatentReceiver,
    gpu_inited: bool,
    demo_ui_sent: bool,
    input_buffer: alloc::string::String,
    avatar: Option<JarvisAvatar>,
    dragging: bool,
    drag_id: AppId,
    drag_off_x: isize,
    drag_off_y: isize,
}

impl DisplayAgent {
    pub fn new() -> Self {
        DisplayAgent {
            receiver: EVENT_BUS.subscribe(hermes::hermes::TOPIC_HERMES_RESPONSE),
            echo_receiver: EVENT_BUS.subscribe("KEYBOARD_ECHO"),
            mouse_receiver: EVENT_BUS.subscribe(hermes::agents::mouse_agent::TOPIC_MOUSE_MOVED),
            click_receiver: EVENT_BUS.subscribe(hermes::agents::mouse_agent::TOPIC_MOUSE_CLICK),
            ui_receiver: EVENT_BUS.subscribe(TOPIC_UI_SPEC),
            latent_receiver: k_nano::LATENT_BUS.subscribe(event_bus::TOPIC_THOUGHT_LLM),
            gpu_inited: false,
            demo_ui_sent: false,
            input_buffer: alloc::string::String::new(),
            avatar: None,
            dragging: false,
            drag_id: AppId::None,
            drag_off_x: 0,
            drag_off_y: 0,
        }
    }

    fn apply_ui_spec(&mut self, json: &str) {
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
            serial_println!("[ADR-0047-H] ui_spec applied title={}", spec.title);
        }
    }
}

impl Agent for DisplayAgent {
    fn manifest(&self) -> &AgentManifest { &DISPLAY_MANIFEST }

    fn tick(&mut self, tick: u64, _count: u64) -> AgentTickResult {
        if !self.gpu_inited {
            let gpu = GPU.lock();
            if let Some(ref gpu_dev) = *gpu {
                let fb = DoubleBuffer::new(
                    gpu_dev.fb_addr as usize, gpu_dev.fb_width as usize,
                    gpu_dev.fb_height as usize, gpu_dev.fb_stride as usize,
                    gpu_dev.fb_bpp as usize, gpu_dev.rgb_order,
                );
                let mut desktop = JarvisDesktop::new(fb);
                desktop.register_app(AppId::HermesChat, "Hermes Chat", Layer::HermesOverlay);
                desktop.register_app(AppId::Settings, "Settings", Layer::AppWindows);
                desktop.register_app(AppId::Power, "Power", Layer::AppWindows);
                desktop.register_app(AppId::Ide, "BitNet IDE", Layer::AppWindows);
                desktop.register_app(AppId::Camera, "Camera", Layer::AppWindows);
                desktop.register_app(AppId::AudioViz, "Audio Visualizer", Layer::AppWindows);
                desktop.ensure_hermes_overlay();
                *COMPOSITOR.lock() = Some(desktop);
                self.avatar = Some(JarvisAvatar::new(gpu_dev));
                serial_println!("[JARVIS] Desktop iniciado @ {}x{}", gpu_dev.fb_width, gpu_dev.fb_height);
            }
            self.gpu_inited = true;
            return AgentTickResult::Pending;
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

        // H4: avatar telemetria from LatentBus norm + H2/H5 viz
        while let Some(pkt) = self.latent_receiver.try_receive() {
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
        }

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
        while let Some(ev) = self.click_receiver.try_receive() {
            if ev.payload.len() >= 3 {
                let btn = ev.payload[0];
                let cx = u16::from_le_bytes([ev.payload[1], ev.payload[2]]) as usize;
                let cy = if ev.payload.len() >= 5 {
                    u16::from_le_bytes([ev.payload[3], ev.payload[4]]) as usize
                } else { 0 };
                *MOUSE_BUTTONS.lock() = btn;

                let mut comp = COMPOSITOR.lock();
                if let Some(ref mut desktop) = *comp {
                    let apps_clone = desktop.apps.clone();
                    // Click na dock bar: toggle app
                    let dock_y = desktop.h.saturating_sub(36);
                    if cy >= dock_y {
                        for (idx, app) in apps_clone.iter().enumerate() {
                            if app.visible {
                                let rx = 10 + idx * 66;
                                if cx >= rx && cx <= rx + 60 {
                                    desktop.toggle_app(app.id);
                                }
                            }
                        }
                    } else if btn == 1 { // Left click — check close buttons
                        for app in &apps_clone {
                            if !app.visible { continue; }
                            let cx_btn = app.x + app.w - 20;
                            if cx >= cx_btn && cx <= cx_btn + 16 && cy >= app.y + 3 && cy <= app.y + 19 {
                                desktop.close_window(app.id);
                                break;
                            }
                            if cx >= app.x && cx <= app.x + app.w && cy >= app.y && cy <= app.y + 24 {
                                self.dragging = true;
                                self.drag_id = app.id;
                                self.drag_off_x = cx as isize - app.x as isize;
                                self.drag_off_y = cy as isize - app.y as isize;
                                break;
                            }
                        }
                    }
                }
                drop(comp);
            }
        }
        // Handle drag continuacao (enquanto mouse move sem novo clique)
        if self.dragging {
            let mx = *MOUSE_X.lock();
            let my = *MOUSE_Y.lock();
            let mut comp = COMPOSITOR.lock();
            if let Some(ref mut desktop) = *comp {
                let app = desktop.apps.iter_mut().find(|a| a.id == self.drag_id && a.visible);
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

        // Render desktop
        let mut comp = COMPOSITOR.lock();
        if let Some(ref mut desktop) = *comp {
            if let Some(ref mut avatar) = self.avatar {
                avatar.render(&mut desktop.fb);
            }
            desktop.render(tick);
        }
        drop(comp);

        self.input_buffer.clear();
        AgentTickResult::Pending
    }
}
