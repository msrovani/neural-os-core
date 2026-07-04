//! DisplayAgent — renderiza Hermes Chat Console (NousResearch-style).
//! Substitui o compositor multi-window bugado.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use crate::hermes;
use crate::serial_println;
use crate::EVENT_BUS;
use crate::display::fb::{DoubleBuffer, GPU};
use crate::display::compositor::COMPOSITOR;
use crate::display::console::NeuralConsole;

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
}

impl DisplayAgent {
    pub fn new() -> Self {
        DisplayAgent {
            receiver: EVENT_BUS.subscribe(hermes::TOPIC_HERMES_RESPONSE),
            echo_receiver: EVENT_BUS.subscribe("KEYBOARD_ECHO"),
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
                    gpu_dev.fb_addr as usize, gpu_dev.fb_width as usize,
                    gpu_dev.fb_height as usize, gpu_dev.fb_stride as usize,
                    gpu_dev.fb_bpp as usize,
                );
                *COMPOSITOR.lock() = Some(NeuralConsole::new(fb));
                serial_println!("[DISPLAY] Hermes Chat {}x{} @{:x}",
                    gpu_dev.fb_width, gpu_dev.fb_height, gpu_dev.fb_addr);
            }
            self.gpu_inited = true;
            return AgentTickResult::Pending;
        }

        // Keyboard echo
        while let Some(ev) = self.echo_receiver.try_receive() {
            self.input_buffer = core::str::from_utf8(&ev.payload).unwrap_or("").into();
        }

        // Renderiza Hermes Chat Console
        if let Some(ref mut console) = *COMPOSITOR.lock() {
            console.input_buffer = self.input_buffer.clone();
            let tick = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
            let mem = crate::memory::global_hardware_context();
            console.render(tick, 0, mem[0], false, false);
        }

        AgentTickResult::Pending
    }
}
