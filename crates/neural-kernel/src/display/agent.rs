use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use crate::hermes;
use crate::interrupts::TIMER_TICKS;
use crate::serial_println;
use crate::EVENT_BUS;
use crate::display::fb::{DoubleBuffer, GPU};

const DISPLAY_MANIFEST: AgentManifest = AgentManifest {
    name: "display",
    kind: AgentKind::Console,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

pub struct DisplayAgent {
    receiver: crate::Receiver,
    console: Option<NeuralConsole>,
    gpu_inited: bool,
    input_buffer: alloc::string::String,
}

impl DisplayAgent {
    pub fn new() -> Self {
        DisplayAgent {
            receiver: EVENT_BUS.subscribe(hermes::TOPIC_HERMES_RESPONSE),
            console: None,
            gpu_inited: false,
            input_buffer: alloc::string::String::new(),
        }
    }
}

use crate::display::console::NeuralConsole;

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
                serial_println!("[DISPLAY] DoubleBuffer {}x{} bpp={} @{:x}",
                    gpu_dev.fb_width, gpu_dev.fb_height, gpu_dev.fb_bpp, gpu_dev.fb_addr);
                self.console = Some(NeuralConsole::new(fb));
            }
            self.gpu_inited = true;
        }

        while let Some(event) = self.receiver.try_receive() {
            let text = core::str::from_utf8(&event.payload).unwrap_or("(bytes)");
            if let Some(ref mut console) = self.console {
                console.push_line(text);
                console.set_prompt_visible(true);
            } else {
                crate::serial_println!("[Hermes] {}", text);
                crate::print!("> ");
                crate::serial_print!("> ");
            }
        }

        if let Some(ref mut console) = self.console {
            let tick = TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            let mem = crate::memory::global_hardware_context();
            console.set_input_buffer(&self.input_buffer);
            console.render(tick as u64, 8, mem[0], false, false);
        }

        AgentTickResult::Pending
    }
}
