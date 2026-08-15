//! JarbasAgent — persona JARBAS.
//!
//! Saudação de boot no espírito do suit-online (MCU Iron Man / JARVIS):
//! confirma upload, HUD/fleet prontos, "à sua disposição" — texto original do
//! Neural OS (não cita filme). LLM se fluente; senão template honesto (soft-float).

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use alloc::string::String;
use event_bus::{CapabilityToken, Event, Receiver};
use core::sync::atomic::{AtomicBool, Ordering};
use crate::jarvis::{JarbasEngine, Emotion, EmotionAnalysis};
use crate::audio::context::build_emotional_context;
use crate::audio::voice::PLAYBACK_RING;

/// Saudacao HW emitida no register (K44) — evita depender do scheduler (hang pos-K44).
static HW_GREET_EMITTED: AtomicBool = AtomicBool::new(false);

const JARVIS_MANIFEST: AgentManifest = AgentManifest {
    name: "JARBAS",
    kind: AgentKind::Console,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

/// Soft-float BitNet 2B + max_gen=6 produz mash BPE (ex: "LOA,BLOA,BLOA,BL") — nao e frase.
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
    // Mash tipico: poucas vogais, sem espacos, muitas virgulas / repeticao
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

/// Template suit-boot (fallback honesto quando LLM mash / soft-float).
/// Tom: upload confirmado + HUD + fleet + serviço — original Neural OS.
fn compose_boot_greeting(mem_mb: u64, cpu_count: u8, agent_count: usize) -> String {
    let fleet = if agent_count > 200 {
        "full agent fleet online"
    } else if agent_count > 20 {
        "agent fleet standing by"
    } else {
        "core agents standing by"
    };
    let power = if mem_mb > 4096 {
        "ample headroom"
    } else if mem_mb > 1024 {
        "power reserves strong"
    } else if mem_mb >= 256 {
        "control surfaces nominal"
    } else {
        "compact but fully armed"
    };
    alloc::format!(
        "Upload complete. JARBAS online and ready — {}MB RAM, {} CPU core{}, {} ({} agents), {}. HUD engaged. At your service.",
        mem_mb,
        cpu_count,
        if cpu_count == 1 { "" } else { "s" },
        fleet,
        agent_count,
        power
    )
}

fn compose_boot_llm_prompt(mem_mb: u64, cpu_count: u8, agent_count: usize) -> String {
    alloc::format!(
        "You are JARBAS, the Neural OS companion AI coming online after boot — \
         like a calm suit AI confirming upload into the armor HUD. \
         Speak one or two short sentences: confirm you are online and ready, \
         mention that the HUD is engaged and the agent fleet is standing by, \
         include the live specs ({}MB RAM, {} CPU cores, {} agents), \
         and end that you are at the user's service. \
         Be witty and confident, not verbose. No movie quotes, no emojis, no markdown.",
        mem_mb, cpu_count, agent_count
    )
}

/// Estado da síntese TTS em streaming.
/// Gera o buffer completo em uma chamada e drena em chunks de ~50ms no tick().
enum StreamingTtsState {
    Idle,
    Streaming { buffer: alloc::vec::Vec<i16>, pos: usize },
}

pub struct JarbasAgent {
    user_receiver: Receiver,
    llm_response: Receiver,
    hermes_response: Receiver,
    engine: JarbasEngine,
    last_text_emotion: Option<Emotion>,
    greeted: bool,
    greeting_prompt_sent: bool,
    greet_mem_mb: u64,
    greet_cpu: u8,
    greet_agents: usize,
    /// Streaming TTS state machine.
    stream_tts: StreamingTtsState,
    /// Histórico de conversa: pares (user, assistant).
    conversation: alloc::vec::Vec<(alloc::string::String, alloc::string::String)>,
    /// Máximo de turnos no histórico.
    max_conversation: usize,
}

impl JarbasAgent {
    pub fn new() -> Self {
        JarbasAgent {
            user_receiver: k_nano::EVENT_BUS.subscribe("USER_INTENT"),
            llm_response: k_nano::EVENT_BUS.subscribe("LLM_RESPONSE"),
            hermes_response: k_nano::EVENT_BUS.subscribe("HERMES_RESPONSE"),
            engine: JarbasEngine::new(),
            last_text_emotion: None,
            greeted: false,
            greeting_prompt_sent: false,
            greet_mem_mb: 0,
            greet_cpu: 1,
            greet_agents: 0,
            stream_tts: StreamingTtsState::Idle,
            conversation: alloc::vec::Vec::new(),
            max_conversation: 10,
        }
    }

    fn publish_greeting(&self, body: &str) {
        let greeting = alloc::format!("[JARBAS] {}: {}", self.engine.soul.name, body);
        k_nano::slog_bin!("Log", "msg", "{}", greeting);
        crate::display::compositor::announce_welcome(body);
        crate::display::console::set_llm_busy(false);
        let _ = k_nano::EVENT_BUS.publish(Event {
            id: 0,
            topic: String::from("HERMES_RESPONSE"),
            payload: greeting.into_bytes(),
            token: CapabilityToken::Legacy(1),
        });
    }
}

/// Chamado logo apos `register(JarbasAgent)` no boot HW.
/// Emite template + tenta BOOT.LOG (MSC/ATA); **nunca soft-reboot** — Runtime segue.
pub fn emit_hw_greeting_at_register() {
    if HW_GREET_EMITTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let bare = matches!(
        k_nano::platform_probe::hypervisor(),
        k_nano::platform_probe::HypervisorKind::None
    );
            let no_fat = k_nano::globals::USB_MSC.lock().is_none() && k_nano::ATA_DRIVER.lock().is_none();
    if !(bare || no_fat) {
        return;
    }

    let mem_mb = {
        // SESSÃO_260 (AIOS): mostra a RAM REAL detectada no memory map, não o
        // total gerenciado pelo frame allocator (que era limitado ao bitmap).
        let real = k_nano::memory::TOTAL_RAM_MB.load(core::sync::atomic::Ordering::Relaxed);
        if real > 0 {
            real
        } else {
            let g = k_nano::memory::GLOBAL_ALLOCATOR.lock();
            g.as_ref()
                .map(|a| (a.total_frames as u64 * 4096) / (1024 * 1024))
                .unwrap_or(0)
        }
    };
    let cpu = k_nano::smp::percpu::CPU_COUNT.load(Ordering::Relaxed);
    let agents = {
        let tr = hermes::globals::TRINITY.lock();
        tr.agent_count()
    };
    let body = compose_boot_greeting(mem_mb, cpu, agents);
    let line = alloc::format!("[JARBAS] JARBAS: {}", body);
    k_nano::slog_jarbas!(
        "Jarbas",
        "info",
        "saudacao suit-boot @register K44 (bare={} no_fat={})",
        bare,
        no_fat
    );
    crate::display::compositor::announce_welcome(&body);
    crate::display::fb::console_print(&line);
    crate::display::fb::boot_ckpt(50, "jarvis greet OK");
    let _ = k_nano::EVENT_BUS.publish(Event {
        id: 0,
        topic: String::from("HERMES_RESPONSE"),
        payload: line.into_bytes(),
        token: CapabilityToken::Legacy(1),
    });
}

impl Agent for JarbasAgent {
    fn manifest(&self) -> &AgentManifest {
        &JARVIS_MANIFEST
    }

    fn tick(&mut self, tick: u64, _count: u64) -> AgentTickResult {
        // Ja saudou no register (HW) — nao repetir.
        if HW_GREET_EMITTED.load(Ordering::Relaxed) {
            self.greeted = true;
            self.greeting_prompt_sent = true;
        }
        // tick>2: BareMetal/sem FAT → template (LLM soft-float nao completa sem modelo).
        if !self.greeted && !self.greeting_prompt_sent && tick > 2 {
            self.greeting_prompt_sent = true;

            self.greet_mem_mb = {
                // AIOS: RAM real detectada no memory map (TOTAL_RAM_MB), não o
                // total gerenciado pelo frame allocator.
                let real = k_nano::memory::TOTAL_RAM_MB.load(core::sync::atomic::Ordering::Relaxed);
                if real > 0 {
                    real
                } else {
                    let g = k_nano::memory::GLOBAL_ALLOCATOR.lock();
                    g.as_ref()
                        .map(|a| (a.total_frames as u64 * 4096) / (1024 * 1024))
                        .unwrap_or(0)
                }
            };
            self.greet_cpu = k_nano::smp::percpu::CPU_COUNT.load(core::sync::atomic::Ordering::Relaxed);
            self.greet_agents = {
                let tr = hermes::globals::TRINITY.lock();
                tr.agent_count()
            };

            let bare = matches!(
                k_nano::platform_probe::hypervisor(),
                k_nano::platform_probe::HypervisorKind::None
            );
    let no_fat = k_nano::globals::USB_MSC.lock().is_none() && k_nano::ATA_DRIVER.lock().is_none();
            if bare || no_fat {
                self.greeted = true;
                HW_GREET_EMITTED.store(true, Ordering::Relaxed);
                let body =
                    compose_boot_greeting(self.greet_mem_mb, self.greet_cpu, self.greet_agents);
                k_nano::slog_jarbas!(
                    "Jarbas",
                    "info",
                    "saudacao template HW (skip LLM; bare={} no_fat={})",
                    bare,
                    no_fat
                );
                self.publish_greeting(&body);
                let line = alloc::format!("[JARBAS] {}: {}", self.engine.soul.name, body);
                crate::display::fb::console_print(&line);
                crate::display::fb::boot_ckpt(50, "jarvis greet OK");
                // boot_logger skipped (jarbas crate)
                let _ = true;
                return AgentTickResult::Pending;
            }

            let prompt = compose_boot_llm_prompt(
                self.greet_mem_mb,
                self.greet_cpu,
                self.greet_agents,
            );

            k_nano::slog_jarbas!("Jarbas", "info", "Solicitando saudacao suit-boot a LLM...");
            crate::display::console::set_llm_busy(true);
            crate::display::compositor::announce_welcome(
                "Engaging HUD — calibrating virtual environment...",
            );
            let _ = k_nano::EVENT_BUS.publish(Event {
                id: 0,
                topic: String::from("LLM_REQUEST"),
                payload: prompt.into_bytes(),
                token: CapabilityToken::Legacy(1),
            });
        }

        // --- Streaming TTS: push next chunk to PLAYBACK_RING ---
        if let StreamingTtsState::Streaming { ref buffer, ref mut pos } = self.stream_tts {
            const CHUNK: usize = 2560;
            let n = buffer.len().saturating_sub(*pos).min(CHUNK);
            if n > 0 {
                let pushed = PLAYBACK_RING.push(&buffer[*pos..*pos + n]);
                *pos += pushed;
            }
            if *pos >= buffer.len() {
                self.stream_tts = StreamingTtsState::Idle;
            }
        }

        while let Some(ev) = self.llm_response.try_receive() {
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            if text.is_empty() {
                continue;
            }

            if !self.greeted {
                self.greeted = true;
                // Decode constrito greeting (logits a quente). Template so se vazio/total mash.
                let body = if cortex::bpe::text_is_greetingish(text) || is_fluent_boot_text(text) {
                    k_nano::slog_jarbas!("Jarbas", "info", "saudacao LLM a quente");
                    String::from(text.trim())
                } else if text.trim().is_empty() {
                    k_nano::slog_jarbas!("Jarbas", "info", "saudacao vazia — template specs");
                    compose_boot_greeting(self.greet_mem_mb, self.greet_cpu, self.greet_agents)
                } else {
                    k_nano::slog_jarbas!(
                        "Jarbas",
                        "info",
                        "saudacao mash ('{}') — template specs",
                        text.chars().take(48).collect::<String>()
                    );
                    compose_boot_greeting(self.greet_mem_mb, self.greet_cpu, self.greet_agents)
                };
                self.publish_greeting(&body);
                continue;
            }

            let response = alloc::format!("[JARBAS] {}: {}", self.engine.soul.name, text);
            k_nano::slog_bin!("Log", "msg", "{}", response);
            let _ = k_nano::EVENT_BUS.publish(Event {
                id: 0,
                topic: String::from("HERMES_RESPONSE"),
                payload: response.into_bytes(),
                token: CapabilityToken::Legacy(1),
            });
        }

        // --- HERMES_RESPONSE → streaming TTS ---
        while let Some(ev) = self.hermes_response.try_receive() {
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            if text.is_empty()
                || text.starts_with("[JARBAS] Escutando")
                || text.starts_with("[JARBAS] 🎤")
            {
                continue;
            }

            let clean = text
                .trim_start_matches("[JARBAS] ")
                .trim_start_matches("JARVIS: ");

            // Gera buffer TTS completo (uma chamada bloqueante), depois drena em chunks.
            let pcm = crate::audio::skills::synthesize_tts(clean);
            let total = pcm.len();
            if total > 0 {
                const CHUNK: usize = 2560;
                let n = total.min(CHUNK);
                let _ = PLAYBACK_RING.push(&pcm[..n]);
                if total > n {
                    self.stream_tts = StreamingTtsState::Streaming {
                        buffer: pcm,
                        pos: n,
                    };
                }
                k_nano::slog_jarbas!("Jarbas", "info", "TTS streaming: {} frames, chunk {}", total, CHUNK);
            }
        }

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
                id: 0,
                topic: String::from("LLM_REQUEST"),
                payload: enhanced_prompt.into_bytes(),
                token: CapabilityToken::Legacy(1),
            });
        }

        self.engine.tick(tick);
        AgentTickResult::Pending
    }
}
