//! DisplayAgent — renderiza JARVIS Avatar + Hermes Chat Console.
//! Port do JARVIS .NET MAUI para bare-metal: partículas, emoção, personalidade.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use crate::hermes;
use crate::jarvis::{JarvisEngine, AvatarState};
use crate::serial_println;
use crate::EVENT_BUS;
use crate::display::fb::{DoubleBuffer, GPU};
use crate::display::compositor::COMPOSITOR;
use crate::display::console::NeuralConsole;
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
    engine: Option<JarvisEngine>,
    last_response: alloc::string::String,
    is_thinking: bool,
    is_speaking: bool,
}

impl DisplayAgent {
    pub fn new() -> Self {
        DisplayAgent {
            receiver: EVENT_BUS.subscribe(hermes::TOPIC_HERMES_RESPONSE),
            echo_receiver: EVENT_BUS.subscribe("KEYBOARD_ECHO"),
            gpu_inited: false,
            input_buffer: alloc::string::String::new(),
            avatar: None,
            engine: Some(JarvisEngine::new()),
            last_response: alloc::string::String::new(),
            is_thinking: false,
            is_speaking: false,
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
                *COMPOSITOR.lock() = Some(NeuralConsole::new(fb));

                // Inicializa avatar JARVIS
                self.avatar = Some(JarvisAvatar::new(gpu_dev));
                serial_println!("[JARVIS] Avatar iniciado @ {}x{}",
                    gpu_dev.fb_width, gpu_dev.fb_height);
            }
            self.gpu_inited = true;
            return AgentTickResult::Pending;
        }

        // Keyboard echo
        while let Some(ev) = self.echo_receiver.try_receive() {
            self.input_buffer = core::str::from_utf8(&ev.payload).unwrap_or("").into();
        }

        // Hermes response — processa emoção + atualiza avatar
        while let Some(ev) = self.receiver.try_receive() {
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            self.last_response = alloc::string::String::from(text);
            self.is_speaking = true;

            // Engine JARVIS processa a resposta
            if let Some(ref mut engine) = self.engine {
                engine.process_input(text);
            }
        }

        // Renderiza JARVIS Avatar + Hermes Chat Console
        if let Some(ref mut console) = *COMPOSITOR.lock() {
            console.input_buffer = self.input_buffer.clone();

            // Determina estado do avatar
            let avatar_state = self.engine.as_ref().map_or(AvatarState::Idle, |e| {
                e.avatar_state_for(self.is_thinking, self.is_speaking)
            });

            // Atualiza e renderiza avatar
            if let Some(ref mut avatar) = self.avatar {
                avatar.set_state(avatar_state);
                avatar.render(&mut console.fb);
            }

            // Renderiza console (texto + métricas)
            let mem = crate::memory::global_hardware_context();
            console.render(tick, 0, mem[0], false, false);
            console.fb.swap();
        }

        // Reseta flags
        if tick % 10 == 0 { self.is_speaking = false; }
        if tick % 5 == 0 { self.is_thinking = false; }

        AgentTickResult::Pending
    }
}
