use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use event_bus::{CapabilityToken, Event, Receiver};
use crate::jarvis::JarvisEngine;
use crate::serial_println;

const JARVIS_MANIFEST: AgentManifest = AgentManifest {
    name: "jarvis",
    kind: AgentKind::Console,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

pub struct JarvisAgent {
    user_receiver: Receiver,
    llm_response: Receiver,
    engine: JarvisEngine,
}

impl JarvisAgent {
    pub fn new() -> Self {
        JarvisAgent {
            user_receiver: crate::EVENT_BUS.subscribe("USER_INTENT"),
            llm_response: crate::EVENT_BUS.subscribe("LLM_RESPONSE"),
            engine: JarvisEngine::new(),
        }
    }
}

impl Agent for JarvisAgent {
    fn manifest(&self) -> &AgentManifest { &JARVIS_MANIFEST }

    fn tick(&mut self, tick: u64, _count: u64) -> AgentTickResult {
        // Input do usuario (teclado ou STT)
        while let Some(ev) = self.user_receiver.try_receive() {
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            serial_println!("[JARVIS] \"{}\"", text);
            self.engine.process_input(text);

            let _ = crate::EVENT_BUS.publish(Event {
                id: 0,
                topic: alloc::string::String::from("HERMES_RESPONSE"),
                payload: alloc::format!("[JARVIS] {} 🤔 Pensando...", self.engine.soul.name).into_bytes(),
                token: CapabilityToken::Legacy(1),
            });

            let _ = crate::EVENT_BUS.publish(Event {
                id: 0,
                topic: alloc::string::String::from("LLM_REQUEST"),
                payload: text.as_bytes().to_vec(),
                token: CapabilityToken::Legacy(1),
            });
        }

        // Resposta do LLM -> JARVIS fala
        while let Some(ev) = self.llm_response.try_receive() {
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            if text.is_empty() { continue; }

            let response = alloc::format!("[JARVIS] {}: {}", self.engine.soul.name, text);
            serial_println!("{}", response);

            let bytes = response.as_bytes().to_vec();
            let _ = crate::EVENT_BUS.publish(Event {
                id: 0, topic: alloc::string::String::from("HERMES_RESPONSE"),
                payload: bytes.clone(), token: CapabilityToken::Legacy(1),
            });
            let _ = crate::EVENT_BUS.publish(Event {
                id: 0, topic: alloc::string::String::from(crate::audio::TOPIC_TTS_CMD),
                payload: bytes, token: CapabilityToken::Legacy(1),
            });
        }

        self.engine.tick(tick);
        AgentTickResult::Pending
    }
}
