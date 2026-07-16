//! JarvisAgent — a persona JARVIS que conversa com o usuario.
//! A saudacao inicial é gerada pela LLM com base nos recursos do sistema.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use event_bus::{CapabilityToken, Event, Receiver};
use crate::jarvis::{JarvisEngine, Emotion, EmotionAnalysis};
use crate::audio::context::build_emotional_context;
use k_nano::serial_println;

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
    greeted: bool,
    greeting_prompt_sent: bool,
}

impl JarvisAgent {
    pub fn new() -> Self {
        JarvisAgent {
            user_receiver: k_nano::EVENT_BUS.subscribe("USER_INTENT"),
            llm_response: k_nano::EVENT_BUS.subscribe("LLM_RESPONSE"),
            engine: JarvisEngine::new(),
            last_text_emotion: None,
            greeted: false,
            greeting_prompt_sent: false,
        }
    }
}

impl Agent for JarvisAgent {
    fn manifest(&self) -> &AgentManifest { &JARVIS_MANIFEST }

    fn tick(&mut self, tick: u64, _count: u64) -> AgentTickResult {
        // ── Saudação inicial gerada pela LLM ──────────────────
        if !self.greeted && !self.greeting_prompt_sent && tick > 15 {
            self.greeting_prompt_sent = true;

            let mem_mb = {
                let g = k_nano::memory::GLOBAL_ALLOCATOR.lock();
                g.as_ref().map(|a| (a.total_frames as u64 * 4096) / (1024 * 1024)).unwrap_or(0)
            };
            let _tick_rate = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            let cpu_count = k_nano::smp::percpu::CPU_COUNT.load(core::sync::atomic::Ordering::Relaxed);
            let agent_count = {
                let tr = hermes::globals::TRINITY.lock();
                tr.agent_count()
            };
            let tts_mode = "formant"; // skills::TTS_ENGINE é privado; manter simples

            let prompt = alloc::format!(
                "You are JARVIS, an AI operating system. Generate a single short sentence \
                 greeting the user. Include that the system has {}MB RAM, {} CPU cores, \
                 {} agents, running in {} TTS mode. Be creative and match the personality \
                 based on these specs:\n\
                 - If memory < 256MB: humble, 'small but mighty'\n\
                 - If 256-1024MB: modest, capable\n\
                 - If > 1024MB: confident, powerful\n\
                 - If > 4096MB: cocky, 'I could run a small country'\n\
                 - If agents > 200: 'managing a small army'\n\
                 Speak as JARVIS. One sentence only, no emojis, no markdown.",
                mem_mb, cpu_count, agent_count, tts_mode
            );

            serial_println!("[JARVIS] Solicitando saudacao a LLM...");
            let _ = k_nano::EVENT_BUS.publish(Event {
                id: 0, topic: alloc::string::String::from("LLM_REQUEST"),
                payload: prompt.into_bytes(), token: CapabilityToken::Legacy(1),
            });
        }

        // ── Resposta da LLM (saudacao ou conversa) ──────────
        while let Some(ev) = self.llm_response.try_receive() {
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            if text.is_empty() { continue; }

            if !self.greeted {
                self.greeted = true;
                let greeting = alloc::format!("[JARVIS] {}: {}", self.engine.soul.name, text);
                serial_println!("{}", greeting);
                let bytes = greeting.as_bytes().to_vec();
                let _ = k_nano::EVENT_BUS.publish(Event {
                    id: 0, topic: alloc::string::String::from("HERMES_RESPONSE"),
                    payload: bytes.clone(), token: CapabilityToken::Legacy(1),
                });
                let _ = k_nano::EVENT_BUS.publish(Event {
                    id: 0, topic: alloc::string::String::from(crate::audio::TOPIC_TTS_CMD),
                    payload: bytes, token: CapabilityToken::Legacy(1),
                });
                continue;
            }

            let response = alloc::format!("[JARVIS] {}: {}", self.engine.soul.name, text);
            serial_println!("{}", response);
            let bytes = response.as_bytes().to_vec();
            let _ = k_nano::EVENT_BUS.publish(Event {
                id: 0, topic: alloc::string::String::from("HERMES_RESPONSE"),
                payload: bytes.clone(), token: CapabilityToken::Legacy(1),
            });
            let _ = k_nano::EVENT_BUS.publish(Event {
                id: 0, topic: alloc::string::String::from(crate::audio::TOPIC_TTS_CMD),
                payload: bytes, token: CapabilityToken::Legacy(1),
            });
        }

        // ── Input do usuario ─────────────────────────────────
        while let Some(ev) = self.user_receiver.try_receive() {
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            serial_println!("[JARVIS] \"{}\"", text);

            let text_emotion = EmotionAnalysis::analyze(text);
            self.last_text_emotion = Some(text_emotion.dominant());
            self.engine.process_input(text);

            let emotional_ctx = build_emotional_context(self.last_text_emotion);
            let enhanced_prompt = alloc::format!("{}\nUser: {}", emotional_ctx, text);
            serial_println!("[JARVIS] Contexto emocional: {}", emotional_ctx);

            if !self.greeted {
                self.greeted = true;
            }

            let _ = k_nano::EVENT_BUS.publish(Event {
                id: 0, topic: alloc::string::String::from("LLM_REQUEST"),
                payload: enhanced_prompt.into_bytes(), token: CapabilityToken::Legacy(1),
            });
        }

        self.engine.tick(tick);
        AgentTickResult::Pending
    }
}
