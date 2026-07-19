//! JarvisAgent — persona JARVIS. Saudacao: LLM se fluente; senao template (soft-float 2B).

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use alloc::string::String;
use event_bus::{CapabilityToken, Event, Receiver};
use crate::jarvis::{JarvisEngine, Emotion, EmotionAnalysis};
use crate::audio::context::build_emotional_context;

const JARVIS_MANIFEST: AgentManifest = AgentManifest {
    name: "jarvis",
    kind: AgentKind::Console,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

fn is_fluent_boot_text(text: &str) -> bool {
    let t = text.trim();
    if t.len() < 12 {
        return false;
    }
    let letters = t.bytes().filter(|b| b.is_ascii_alphabetic()).count();
    let vowels = t
        .bytes()
        .filter(|b| matches!(b.to_ascii_lowercase(), b'a' | b'e' | b'i' | b'o' | b'u'))
        .count();
    let spaces = t.bytes().filter(|&b| b == b' ').count();
    let commas = t.bytes().filter(|&b| b == b',').count();
    if spaces == 0 {
        return false;
    }
    if vowels * 4 < letters {
        return false;
    }
    if commas >= 2 && spaces < 2 {
        return false;
    }
    true
}

fn compose_boot_greeting(mem_mb: u64, cpu_count: u8, agent_count: usize) -> String {
    let vibe = if mem_mb > 4096 {
        "I could run a small country with this much RAM"
    } else if mem_mb > 1024 {
        "systems are online and feeling powerful"
    } else if mem_mb >= 256 {
        "modest hardware, fully capable"
    } else {
        "small but mighty"
    };
    let army = if agent_count > 200 {
        ", managing a small army of agents"
    } else if agent_count > 20 {
        ", fleet standing by"
    } else {
        ""
    };
    alloc::format!(
        "Good day. JARVIS online — {}MB RAM, {} CPU core{}, {} agents{}. {}.",
        mem_mb,
        cpu_count,
        if cpu_count == 1 { "" } else { "s" },
        agent_count,
        army,
        vibe
    )
}

pub struct JarvisAgent {
    user_receiver: Receiver,
    llm_response: Receiver,
    engine: JarvisEngine,
    last_text_emotion: Option<Emotion>,
    greeted: bool,
    greeting_prompt_sent: bool,
    greet_mem_mb: u64,
    greet_cpu: u8,
    greet_agents: usize,
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
            greet_mem_mb: 0,
            greet_cpu: 1,
            greet_agents: 0,
        }
    }
}

impl Agent for JarvisAgent {
    fn manifest(&self) -> &AgentManifest { &JARVIS_MANIFEST }

    fn tick(&mut self, tick: u64, _count: u64) -> AgentTickResult {
        // ── Saudação inicial ──────────────────────────────────
        // tick>2: TCG soft-float 2B FWD não completa a tempo → template imediato.
        // WHPX/HW: LLM primeiro; se lixo → template (abaixo).
        if !self.greeted && !self.greeting_prompt_sent && tick > 2 {
            self.greeting_prompt_sent = true;

            self.greet_mem_mb = {
                let g = k_nano::memory::GLOBAL_ALLOCATOR.lock();
                g.as_ref()
                    .map(|a| (a.total_frames as u64 * 4096) / (1024 * 1024))
                    .unwrap_or(0)
            };
            self.greet_cpu = k_nano::smp::percpu::CPU_COUNT.load(core::sync::atomic::Ordering::Relaxed);
            self.greet_agents = {
                let tr = hermes::globals::TRINITY.lock();
                tr.agent_count()
            };

            let tcg = matches!(
                k_nano::platform_probe::hypervisor(),
                k_nano::platform_probe::HypervisorKind::Tcg
            );
            if tcg {
                self.greeted = true;
                let body =
                    compose_boot_greeting(self.greet_mem_mb, self.greet_cpu, self.greet_agents);
                let greeting = alloc::format!("[JARVIS] {}: {}", self.engine.soul.name, body);
                k_nano::slog_jarbas!(
                    "Jarbas",
                    "info",
                    "saudacao template TCG (skip LLM soft-float FWD)"
                );
                k_nano::slog_jarbas!("Log", "msg", "{}", greeting);
                let bytes = greeting.as_bytes().to_vec();
                let _ = k_nano::EVENT_BUS.publish(Event {
                    id: 0,
                    topic: String::from("HERMES_RESPONSE"),
                    payload: bytes.clone(),
                    token: CapabilityToken::Legacy(1),
                });
                let _ = k_nano::EVENT_BUS.publish(Event {
                    id: 0,
                    topic: String::from(crate::audio::TOPIC_TTS_CMD),
                    payload: bytes,
                    token: CapabilityToken::Legacy(1),
                });
            } else {
                let tts_mode = "formant";
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
                    self.greet_mem_mb, self.greet_cpu, self.greet_agents, tts_mode
                );
                k_nano::slog_jarbas!("Jarbas", "info", "Solicitando saudacao a LLM...");
                let _ = k_nano::EVENT_BUS.publish(Event {
                    id: 0,
                    topic: alloc::string::String::from("LLM_REQUEST"),
                    payload: prompt.into_bytes(),
                    token: CapabilityToken::Legacy(1),
                });
            }
        }

        // ── Resposta da LLM (saudacao ou conversa) ──────────
        while let Some(ev) = self.llm_response.try_receive() {
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            if text.is_empty() { continue; }

            if !self.greeted {
                self.greeted = true;
                let body = if is_fluent_boot_text(text) {
                    String::from(text.trim())
                } else {
                    k_nano::slog_jarbas!(
                        "Jarbas",
                        "info",
                        "saudacao LLM lixo — template specs"
                    );
                    compose_boot_greeting(self.greet_mem_mb, self.greet_cpu, self.greet_agents)
                };
                let greeting = alloc::format!("[JARVIS] {}: {}", self.engine.soul.name, body);
                k_nano::slog_jarbas!("Log", "msg", "{}", greeting);
                let bytes = greeting.as_bytes().to_vec();
                let _ = k_nano::EVENT_BUS.publish(Event {
                    id: 0, topic: String::from("HERMES_RESPONSE"),
                    payload: bytes.clone(), token: CapabilityToken::Legacy(1),
                });
                let _ = k_nano::EVENT_BUS.publish(Event {
                    id: 0, topic: String::from(crate::audio::TOPIC_TTS_CMD),
                    payload: bytes, token: CapabilityToken::Legacy(1),
                });
                continue;
            }

            let response = alloc::format!("[JARVIS] {}: {}", self.engine.soul.name, text);
            k_nano::slog_jarbas!("Log", "msg", "{}", response);
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
            k_nano::slog_jarbas!("Jarbas", "info", "\"{}\"", text);

            let text_emotion = EmotionAnalysis::analyze(text);
            self.last_text_emotion = Some(text_emotion.dominant());
            self.engine.process_input(text);

            let emotional_ctx = build_emotional_context(self.last_text_emotion);
            let enhanced_prompt = alloc::format!("{}\nUser: {}", emotional_ctx, text);
            k_nano::slog_jarbas!("Jarbas", "info", "Contexto emocional: {}", emotional_ctx);

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
