//! DisplayAgent — JARVIS Desktop com compositor multi-app.
//! Hermes Chat + Settings + Power + JARVIS avatar overlay.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use crate::hermes;
use crate::jarvis::{JarvisEngine, AvatarState};
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
                    gpu_dev.fb_bpp as usize,
                );
                let mut desktop = JarvisDesktop::new(fb);
                desktop.register_app(AppId::HermesChat, "Hermes Chat");
                desktop.register_app(AppId::Settings, "Settings");
                desktop.register_app(AppId::Power, "Power");
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
        if self.input_buffer.contains("[F2]") { if let Some(ref mut d) = *COMPOSITOR.lock() { d.toggle_app(AppId::Settings); }}
        if self.input_buffer.contains("[F3]") { if let Some(ref mut d) = *COMPOSITOR.lock() { d.toggle_app(AppId::Power); }}

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
