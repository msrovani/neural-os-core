//! DisplayAgent — JARVIS Desktop com compositor multi-app.
//! Hermes Chat + Settings + Power + JARVIS avatar overlay.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use crate::hermes;
use crate::serial_println;
use crate::EVENT_BUS;
use crate::display::fb::{DoubleBuffer, GPU};
use crate::display::compositor::{COMPOSITOR, JarvisDesktop, AppId};
use crate::display::avatar::JarvisAvatar;

const DISPLAY_MANIFEST: AgentManifest = AgentManifest {
    name: "display",
    kind: AgentKind::Console,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

pub struct DisplayAgent {
    receiver: crate::Receiver,
    echo_receiver: crate::Receiver,
    gpu_inited: bool,
    input_buffer: alloc::string::String,
    avatar: Option<JarvisAvatar>,
}

impl DisplayAgent {
    pub fn new() -> Self {
        DisplayAgent {
            receiver: EVENT_BUS.subscribe(hermes::TOPIC_HERMES_RESPONSE),
            echo_receiver: EVENT_BUS.subscribe("KEYBOARD_ECHO"),
            gpu_inited: false,
            input_buffer: alloc::string::String::new(),
            avatar: None,
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
                desktop.register_app(AppId::HermesChat, "Hermes Chat");
                desktop.register_app(AppId::Settings, "Settings");
                desktop.register_app(AppId::Power, "Power");
                desktop.register_app(AppId::Ide, "BitNet IDE");
                desktop.register_app(AppId::Camera, "Camera");
                desktop.register_app(AppId::AudioViz, "Audio Visualizer");
                desktop.toggle_app(AppId::HermesChat);
                *COMPOSITOR.lock() = Some(desktop);
                self.avatar = Some(JarvisAvatar::new(gpu_dev));
                serial_println!("[JARVIS] Desktop iniciado @ {}x{}", gpu_dev.fb_width, gpu_dev.fb_height);
            }
            self.gpu_inited = true;
            return AgentTickResult::Pending;
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
