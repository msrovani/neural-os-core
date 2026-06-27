//! DisplayAgent — gerencia a saída visual (VGA text ou framebuffer).
//! Usa VirtIO-GPU framebuffer se disponível, fallback VGA text.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use crate::hermes;
use crate::interrupts::TIMER_TICKS;
use crate::{serial_println, println};
use crate::EVENT_BUS;
use crate::display::fb::{Framebuffer, GPU};
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
    console: Option<NeuralConsole>,
    gpu_inited: bool,
}

impl DisplayAgent {
    pub fn new() -> Self {
        DisplayAgent {
            receiver: EVENT_BUS.subscribe(hermes::TOPIC_HERMES_RESPONSE),
            console: None,
            gpu_inited: false,
        }
    }
}

impl Agent for DisplayAgent {
    fn manifest(&self) -> &AgentManifest { &DISPLAY_MANIFEST }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        // Init framebuffer from GPU on first tick
        if !self.gpu_inited {
            let gpu = GPU.lock();
            if let Some(ref gpu_dev) = *gpu {
                let phys_offset = crate::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
                let fb = Framebuffer::new(
                    (gpu_dev.fb_addr + phys_offset) as usize,
                    gpu_dev.fb_width as usize,
                    gpu_dev.fb_height as usize,
                    gpu_dev.fb_stride as usize,
                );
                self.console = Some(NeuralConsole::new(fb));
                serial_println!("[DISPLAY] Framebuffer VirtIO-GPU {}x{}",
                    gpu_dev.fb_width, gpu_dev.fb_height);
                println!("[DISPLAY] VirtIO-GPU framebuffer {}x{}",
                    gpu_dev.fb_width, gpu_dev.fb_height);
            }
            self.gpu_inited = true;
        }

        // Process HERMES_RESPONSE events
        while let Some(event) = self.receiver.try_receive() {
            let text = core::str::from_utf8(&event.payload).unwrap_or("(bytes)");
            if let Some(ref mut console) = self.console {
                console.push_line(text);
            } else {
                // Fallback: VGA text
                serial_println!("[Hermes] {}", text);
                println!("[Hermes] {}", text);
            }
        }

        // Render frame on GPU framebuffer
        if let Some(ref mut console) = self.console {
            let tick = TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            let mem = crate::memory::global_hardware_context();
            let agent_count = 8;
            let llm_busy = false;
            let net_online = crate::net::NET_CONFIG.lock().online;
            console.render(tick as u64, agent_count, mem[0], llm_busy, net_online);
        }

        AgentTickResult::Pending
    }
}
