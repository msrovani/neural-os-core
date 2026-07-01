//! DisplayAgent — renderiza o desktop (Compositor) com dock + cursor + apps.
//! Keyboard echo em tempo real via KEYBOARD_ECHO topic.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use crate::hermes;
use crate::interrupts::TIMER_TICKS;
use crate::serial_println;
use crate::EVENT_BUS;
use crate::display::fb::{DoubleBuffer, GPU};
use crate::display::compositor::COMPOSITOR;

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
    fb: Option<DoubleBuffer>,
    gpu_inited: bool,
    input_buffer: alloc::string::String,
}

impl DisplayAgent {
    pub fn new() -> Self {
        DisplayAgent {
            receiver: EVENT_BUS.subscribe(hermes::TOPIC_HERMES_RESPONSE),
            echo_receiver: EVENT_BUS.subscribe("KEYBOARD_ECHO"),
            fb: None,
            gpu_inited: false,
            input_buffer: alloc::string::String::new(),
        }
    }
}

impl Agent for DisplayAgent {
    fn manifest(&self) -> &AgentManifest { &DISPLAY_MANIFEST }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        if !self.gpu_inited {
            let gpu = GPU.lock();
            if let Some(ref gpu_dev) = *gpu {
                let fb = DoubleBuffer::new(
                    gpu_dev.fb_addr as usize,
                    gpu_dev.fb_width as usize,
                    gpu_dev.fb_height as usize,
                    gpu_dev.fb_stride as usize,
                    gpu_dev.fb_bpp as usize,
                );
                serial_println!("[DISPLAY] Framebuffer {}x{} @{:x}",
                    gpu_dev.fb_width, gpu_dev.fb_height, gpu_dev.fb_addr);
                self.fb = Some(fb);
            }
            self.gpu_inited = true;
        }

        // Keyboard echo em tempo real
        while let Some(ev) = self.echo_receiver.try_receive() {
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            self.input_buffer = alloc::string::String::from(text);
        }

        // Render Desktop (Compositor)
        if let Some(ref mut fb) = self.fb {
            if let Some(ref mut comp) = *COMPOSITOR.lock() {
                comp.poll_mouse();
                let tick = TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
                // Atualiza input buffer no compositor
                comp.input_text = self.input_buffer.clone();
                comp.render(fb, tick);
                fb.swap();
            }
        }

        AgentTickResult::Pending
    }
}
