//! JarvisAgent — a persona JARVIS que conversa com o usuario.
//! Injeta contexto emocional da voz no prompt da LLM para respostas
//! com empatia e tom adequados.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use event_bus::{CapabilityToken, Event, Receiver};
use crate::jarvis::{JarvisEngine, Emotion, EmotionAnalysis};
use crate::audio::context::build_emotional_context;
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
    last_text_emotion: Option<Emotion>,
}

impl JarvisAgent {
    pub fn new() -> Self {
        JarvisAgent {
            user_receiver: crate::EVENT_BUS.subscribe("USER_INTENT"),
            llm_response: crate::EVENT_BUS.subscribe("LLM_RESPONSE"),
            engine: JarvisEngine::new(),
            last_text_emotion: None,
        }
    }
}

impl Agent for JarvisAgent {
    fn manifest(&self) -> &AgentManifest { &JARVIS_MANIFEST }

    fn tick(&mut self, tick: u64, _count: u64) -> AgentTickResult {
        // ── Input do usuario (teclado ou STT) ────────────────
        while let Some(ev) = self.user_receiver.try_receive() {
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            serial_println!("[JARVIS] \"{}\"", text);

            // Analisa emoção no texto
            let text_emotion = EmotionAnalysis::analyze(text);
            self.last_text_emotion = Some(text_emotion.dominant());
            self.engine.process_input(text);

            // Prepara prompt com contexto emocional para a LLM
            let emotional_ctx = build_emotional_context(self.last_text_emotion);
            let enhanced_prompt = alloc::format!("{}\nUser: {}", emotional_ctx, text);

            serial_println!("[JARVIS] Prompt com contexto emocional: {}", emotional_ctx);

            let _ = crate::EVENT_BUS.publish(Event {
                id: 0,
                topic: alloc::string::String::from("HERMES_RESPONSE"),
                payload: alloc::format!("[JARVIS] {} 🤔 Pensando...", self.engine.soul.name).into_bytes(),
                token: CapabilityToken::Legacy(1),
            });

            let _ = crate::EVENT_BUS.publish(Event {
                id: 0,
                topic: alloc::string::String::from("LLM_REQUEST"),
                payload: enhanced_prompt.into_bytes(),
                token: CapabilityToken::Legacy(1),
            });
        }

        // ── Resposta do LLM → JARVIS fala ──────────────────
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
