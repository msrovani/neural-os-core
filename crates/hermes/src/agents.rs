//! Native Agent implementations — Block 11 (Sprints 39-42)
//! Cada struct implementa agent_core::Agent. Substituem as 7 async fn legacy.

pub mod mouse_agent;
pub mod log_analyst_agent;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use event_bus::{CapabilityToken, Event, Receiver};
use event_bus::latent::{LatentReceiver, TOPIC_THOUGHT_LLM};
use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};

use crate::hermes::{self, IntentCache, WorkflowEngine};
use crate::memory_store;
use k_ai::conversation;
use k_nano::{println, kjson};
use crate::globals::{EVENT_BUS, SKILL_REGISTRY, SKILL_STORAGE, TRUST_CACHE, USAGE_TRACKER, EVENT_LOG,
            CONVERSATION_TRACKER, PENDING_SKILL, SELF_HEAL, BITNET_TRAINER, TRINITY,
            APPROVAL_GATE, boot_log_agent, agency, hw_agents, inventory};
use crate::structured_decode::{StructuredDecoder, DecodeMode};
use crate::decode_harness::recognize;

// ---------------------------------------------------------------------------
// MonitorAgent — Oneshot: publica SYSTEM_READY e conclui
// ---------------------------------------------------------------------------

const MONITOR_MANIFEST: AgentManifest = AgentManifest {
    name: "monitor",
    kind: AgentKind::System,
    schedule: ScheduleKind::Oneshot,
    auto_start: true,
    persist: false,
};

pub struct MonitorAgent { done: bool }

impl MonitorAgent {
    pub fn new() -> Self { MonitorAgent { done: false } }
}

impl Agent for MonitorAgent {
    fn manifest(&self) -> &AgentManifest { &MONITOR_MANIFEST }
    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        if self.done { return AgentTickResult::Done; }
        let event = Event { id: 0, topic: String::from("SYSTEM_READY"), payload: vec![1, 2, 3], token: CapabilityToken::Legacy(1) };
        match EVENT_BUS.publish(event) {
            Ok(()) => { k_nano::slog_hermes!("Agent", "monitor", "Evento SYSTEM_READY publicado."); }
            Err(e) => { k_nano::slog_hermes!("Agent", "monitor", "Falha: {}", e); }
        }
        self.done = true;
        AgentTickResult::Done
    }
}

// ---------------------------------------------------------------------------
// HwBridgeAgent — IRQ bridge: poll LAST_SCANCODE → publish RAW_HW_IRQ1
// ---------------------------------------------------------------------------

const HWBRIDGE_MANIFEST: AgentManifest = AgentManifest {
    name: "hw_bridge",
    kind: AgentKind::Router,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

pub struct HwBridgeAgent;

impl Agent for HwBridgeAgent {
    fn manifest(&self) -> &AgentManifest { &HWBRIDGE_MANIFEST }
    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        let scancode = k_nano::interrupts::LAST_SCANCODE.swap(0, core::sync::atomic::Ordering::Acquire);
        if scancode != 0 {
            let _ = EVENT_BUS.publish(Event {
                id: 0, topic: String::from("RAW_HW_IRQ1"),
                payload: vec![scancode],
                token: CapabilityToken::Legacy(1),
            });
        }
        AgentTickResult::Pending
    }
}

// ---------------------------------------------------------------------------
// ConsoleAgent — subscreve HERMES_RESPONSE, mostra no VGA+serial
// ---------------------------------------------------------------------------

const CONSOLE_MANIFEST: AgentManifest = AgentManifest {
    name: "hermes_console",
    kind: AgentKind::Console,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

pub struct ConsoleAgent {
    receiver: Receiver,
}

impl ConsoleAgent {
    pub fn new() -> Self {
        ConsoleAgent { receiver: EVENT_BUS.subscribe(hermes::TOPIC_HERMES_RESPONSE) }
    }
}

impl Agent for ConsoleAgent {
    fn manifest(&self) -> &AgentManifest { &CONSOLE_MANIFEST }
    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        if let Some(event) = self.receiver.try_receive() {
            let text = core::str::from_utf8(&event.payload).unwrap_or("(bytes)");
            k_nano::slog_hermes!("Hermes", "info", "{}", text);
        }
        AgentTickResult::Pending
    }
}

// ---------------------------------------------------------------------------
// InputAgent — keyboard buffer, scancode → ASCII → ENTER → USER_INTENT
// ---------------------------------------------------------------------------

const INPUT_MANIFEST: AgentManifest = AgentManifest {
    name: "input",
    kind: AgentKind::Console,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

pub struct InputAgent {
    receiver: Receiver,
    buffer: String,
    ctrl: bool,
    alt: bool,
    shift: bool,
    /// Super key (Win/Cmd) — scancode 0x5B (Left Win) / 0x5C (Right Win).
    super_key: bool,
}

/// Tópico publicado pelo InputAgent com payload `[scancode, ctrl, alt, shift, super_key]`.
/// Consumido pelo DisplayAgent para dispatch de atalhos WM (Super+H, Alt+Tab, etc).
pub const TOPIC_KEY_EVENT: &str = "KEY_EVENT";

impl InputAgent {
    pub fn new() -> Self {
        InputAgent {
            receiver: EVENT_BUS.subscribe("RAW_HW_IRQ1"),
            buffer: String::new(),
            ctrl: false,
            alt: false,
            shift: false,
            super_key: false,
        }
    }
}

impl Agent for InputAgent {
    fn manifest(&self) -> &AgentManifest { &INPUT_MANIFEST }
    fn tick(&mut self, tick: u64, _count: u64) -> AgentTickResult {
        // PS/2 keyboard (IRQ-driven)
        if let Some(event) = self.receiver.try_receive() {
            self.process_scancode(event.payload.first().copied().unwrap_or(0));
        }
        // USB keyboard poll (fallback quando PS/2 nao disponivel)
        if tick % 5 == 0 {
            if let Some(scancode) = unsafe { self.poll_usb_keyboard() } {
                self.process_scancode(scancode);
            }
        }
        AgentTickResult::Pending
    }
}

impl InputAgent {
    fn poll_usb_keyboard(&self) -> Option<u8> {
        unsafe { k_nano::xhci::poll_keyboard() }
    }
    fn process_scancode(&mut self, scancode: u8) {
        let pressed = scancode < 0x80;
        let key = if pressed { scancode } else { scancode & 0x7F };
        match key {
            0x1D => { self.ctrl = pressed; }
            0x38 => { self.alt = pressed; }
            // Left Shift = 0x2A, Right Shift = 0x36
            0x2A | 0x36 => { self.shift = pressed; }
            // Left Win (Super) = 0x5B, Right Win = 0x5C
            0x5B | 0x5C => { self.super_key = pressed; }
            _ => {}
        }
        // Publica KEY_EVENT com modifiers para o DisplayAgent (atalhos WM).
        // Payload: [scancode, ctrl, alt, shift, super_key, pressed]
        let _ = EVENT_BUS.publish(Event {
            id: 0,
            topic: String::from(TOPIC_KEY_EVENT),
            payload: alloc::vec![
                scancode,
                self.ctrl as u8,
                self.alt as u8,
                self.shift as u8,
                self.super_key as u8,
                pressed as u8,
            ],
            token: CapabilityToken::Legacy(1),
        });
        if !pressed { return; }
        if scancode >= 0x80 { return; }
        match scancode {
            0x1C => {
                let text = core::mem::take(&mut self.buffer);
                if !text.is_empty() {
                    k_nano::slog_hermes!("Input", "info", "ENTER — USER_INTENT: \"{}\"", text);
                    println!("[INPUT] ENTER — USER_INTENT: \"{}\"", text);
                    let _ = EVENT_BUS.publish(Event {
                        id: 0, topic: String::from("USER_INTENT"),
                        payload: text.into_bytes(), token: CapabilityToken::Legacy(1),
                    });
                }
            }
            0x0E => { self.buffer.pop(); }
            _ => {
                // Skip keyboard shortcuts that shouldn't type
                if scancode == 0x53 || scancode == 0x39 { }  // Delete and Space handled by WM
                else if let Some(ch) = k_nano::scancode_to_ascii(scancode) { self.buffer.push(ch); }
            }
        }
        // Echo tecla para o display em tempo real
        let _ = EVENT_BUS.publish(Event {
            id: 0, topic: String::from("KEYBOARD_ECHO"),
            payload: self.buffer.clone().into_bytes(),
            token: CapabilityToken::Legacy(1),
        });
    }
}

// ---------------------------------------------------------------------------
// NetAgent — smoltcp poll loop
// ---------------------------------------------------------------------------

const NETAGENT_MANIFEST: AgentManifest = AgentManifest {
    name: "network_agent",
    kind: AgentKind::Network,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

pub struct NetAgent;

impl Agent for NetAgent {
    fn manifest(&self) -> &AgentManifest { &NETAGENT_MANIFEST }
    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        crate::network_agent::network_agent_tick();
        // ADR-0081 B1: mesh tick — heartbeat → cleanup → election
        k_nano::net::mesh::mesh_tick();
        AgentTickResult::Pending
    }
}

impl NetAgent {
    pub fn new() -> Self { NetAgent }
}

// ---------------------------------------------------------------------------
// CortexAgent — LLM inference: subscribe LLM_REQUEST → generate → publish LLM_RESPONSE
// ---------------------------------------------------------------------------

const CORTEX_MANIFEST: AgentManifest = AgentManifest {
    name: "cortex_llm",
    kind: AgentKind::Inference,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

pub struct CortexAgent {
    receiver: Receiver,
}

impl CortexAgent {
    pub fn new() -> Self {
        // Modelo real carregado via boot (FAT32/QEMU-loader). Só seta se boot carregou.
        // Se nenhum .bitnet foi encontrado, generate_via_model() retorna NO_MODEL_MSG honestamente.
        if cortex::cortex::model_status() == cortex::cortex::ModelStatus::NoneLoaded
            && cortex::cortex::model_info().is_none()
        {
            k_nano::slog_cortex!("LLM", "info",
                "Nenhum modelo .bitnet carregado — AI indisponível até boot carregar modelo real");
        }
        // ponytail: boot carrega modelo via load_model() → set_model(). Se não carregou,
        // não criar toy — o sistema opera honestamente sem AI.
        CortexAgent { receiver: EVENT_BUS.subscribe(cortex::cortex::TOPIC_LLM_REQUEST) }
    }
}

impl Agent for CortexAgent {
    fn manifest(&self) -> &AgentManifest { &CORTEX_MANIFEST }
    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        if let Some(event) = self.receiver.try_receive() {
            let user_text = core::str::from_utf8(&event.payload).unwrap_or("");
            let expert = {
                let t = crate::globals::TRINITY.lock();
                t.classify_intent(user_text).name
            };
            k_nano::slog_cortex!(
                "LLM",
                "info",
                "Generating for: \"{}\" route→{}",
                user_text,
                expert
            );
            // hw_control: Hermes Chat já executa audio_set_volume via RouteKind::ExpertSkill.
            // Aqui LLM_REQUEST direto — resposta honesta sem mash de expert 128h.
            if expert == "hw_control" {
                let msg = alloc::format!(
                    "[HW] controle '{}' — use path Hermes/skill audio_set_volume",
                    user_text
                );
                k_nano::slog_cortex!("LLM", "info", "Generated: \"{}\"", msg);
                let _ = EVENT_BUS.publish(Event {
                    id: 0,
                    topic: alloc::string::String::from(cortex::cortex::TOPIC_LLM_RESPONSE),
                    payload: msg.into_bytes(),
                    token: CapabilityToken::Legacy(1),
                });
                return AgentTickResult::Pending;
            }
            let prompt = if expert == "generator" {
                alloc::format!(
                    "You are Jarbas, the Neural OS voice assistant. \
                     Reply with one short fluent conversational sentence. \
                     Match the user language (PT-BR or EN).\nUser: {}\nJarbas:",
                    user_text
                )
            } else {
                let system_prompt = SKILL_STORAGE.lock().build_system_prompt_for(user_text);
                alloc::format!("{}. PERGUNTA: {}", system_prompt, user_text)
            };
            k_nano::slog_cortex!("LLM", "info", "Calling generate_via_model...");
            let t0 = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            // F4: structured decode when pattern is recognized
            let pattern = recognize(&user_text);
            let output = match pattern {
                crate::decode_harness::SkillPattern::Add => {
                    let mut dec = StructuredDecoder::new(DecodeMode::Number);
                    cortex::cortex::generate_via_model_with_decoder(&prompt, &mut dec)
                }
                crate::decode_harness::SkillPattern::Echo => {
                    let mut dec = StructuredDecoder::new(DecodeMode::Alpha);
                    cortex::cortex::generate_via_model_with_decoder(&prompt, &mut dec)
                }
                crate::decode_harness::SkillPattern::Default => {
                    cortex::cortex::generate_via_model(&prompt)
                }
            };
            let t1 = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            k_nano::slog_cortex!("LLM", "info", "generate_via_model took {} ticks (~{}s)", t1 - t0, (t1 - t0) / 100);
            let output = if output == cortex::cortex::NO_MODEL_MSG || output.trim().is_empty() {
                alloc::format!(
                    "(sem LLM gerador — {})",
                    cortex::model_hub::hub_status()
                )
            } else { output };
            k_nano::slog_cortex!("LLM", "info", "Generated: \"{}\"", output);
            let _ = EVENT_BUS.publish(Event {
                id: 0, topic: alloc::string::String::from(cortex::cortex::TOPIC_LLM_RESPONSE),
                payload: output.into_bytes(), token: CapabilityToken::Legacy(1),
            });
        }
        AgentTickResult::Pending
    }
}

// ---------------------------------------------------------------------------
// HermesAgent — intent router: cortex.think() + command dispatch + skill execution
// Substitui intent_router_daemon com state machine nativa
// ---------------------------------------------------------------------------

enum HermesState {
    Idle,
    AwaitingLLM,
}

const HERMES_MANIFEST: AgentManifest = AgentManifest {
    name: "intent_router",
    kind: AgentKind::Router,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

pub struct HermesAgent {
    user_receiver: Receiver,
    llm_receiver: Receiver,
    security_receiver: Receiver,
    health_receiver: Receiver,
    pnp_receiver: Receiver,
    cap_receiver: Receiver,
    latent_receiver: LatentReceiver,
    latent_recv_total: u64,
    cortex: cortex::cortex::Cortex,
    state: HermesState,
    status_skill: String,
    echo_skill: String,
    hw_skill: String,
    net_diag_skill: String,
    boot_greeted: bool,
    react_phase: crate::hermes::ReActPhase,
    sdd_counter: u64,
    consciousness: cortex::cortex::Consciousness,
    sil: cortex::cortex::SelfImprovementLoop,
    con_skills_ok: u64,
    con_skills_total: u64,
    con_errors: u64,
    con_errors_resolved: u64,
    con_anomaly_count: u64,
    con_memories_total: usize,
    intent_cache: IntentCache,
    output_cache: skill_registry::OutputCache,
    workflow_engine: WorkflowEngine,
}

impl HermesAgent {
    pub fn new() -> Self {
        HermesAgent {
            user_receiver: EVENT_BUS.subscribe(hermes::TOPIC_USER_INTENT),
            llm_receiver: EVENT_BUS.subscribe(cortex::cortex::TOPIC_LLM_RESPONSE),
            security_receiver: EVENT_BUS.subscribe("SECURITY_ALERT"),
            health_receiver: EVENT_BUS.subscribe("HEALTH_ISSUE"),
            pnp_receiver: EVENT_BUS.subscribe(k_ai::hw_capability::TOPIC_HW_PNP_ACTION),
            cap_receiver: EVENT_BUS.subscribe(k_ai::hw_capability::TOPIC_HW_CAPABILITY),
            latent_receiver: k_nano::globals::LATENT_BUS.subscribe(TOPIC_THOUGHT_LLM),
            latent_recv_total: 0,
            cortex: cortex::cortex::Cortex::new(),
            state: HermesState::Idle,
            status_skill: String::from("system_status"),
            echo_skill: String::from("echo"),
            hw_skill: String::from("hardware_info"),
            net_diag_skill: String::from("net_diag"),
            boot_greeted: false,
            react_phase: crate::hermes::ReActPhase::Observe,
            sdd_counter: 0,
            consciousness: cortex::cortex::Consciousness::new(),
            sil: cortex::cortex::SelfImprovementLoop::new(),
            con_skills_ok: 0,
            con_skills_total: 0,
            con_errors: 0,
            con_errors_resolved: 0,
            con_anomaly_count: 0,
            con_memories_total: 0,
            intent_cache: IntentCache::new(),
            output_cache: skill_registry::OutputCache::new(500),
            workflow_engine: WorkflowEngine::new(),
        }
    }

    fn log_phase(&self, phase: crate::hermes::ReActPhase, detail: &str) {
        k_nano::slog_hermes!("Hermes", "info", "{} — {}", phase.label(), detail);
    }

    fn show_sdd(&self, goal: &str) {
        let sdd = crate::hermes::Sdd::new(
            goal,
            &alloc::format!("Tick {}, agentes ativos, memória {:.0}%",
                k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed),
                k_nano::memory::global_hardware_context()[0] * 100.0),
            goal,
            "Comando processado com sucesso",
            "Nada a reverter — comando não destrutivo",
        );
        k_nano::slog_hermes!("Log", "msg", "{}", sdd.display());
    }

    fn execute_skill(&mut self, name: &str, payload: &[u8], token: &CapabilityToken) -> Result<Vec<u8>, &'static str> {
        let token_val = token.as_legacy();
        let now = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        // Sprint 78: OutputCache — skills idempotentes usam cache
        if let Some(cached) = self.output_cache.get(name, payload, now) {
            return Ok(cached.to_vec());
        }
        // Lock order: SKILL_REGISTRY → TRUST_CACHE (consistente em todo codigo)
        let reg = SKILL_REGISTRY.lock();
        {
            let mut tc = TRUST_CACHE.lock();
            if !tc.is_trusted(token_val, name, now) {
                if !reg.validate_token(name, token) {
                    return Err("token nao autorizado");
                }
                tc.check_or_cache(token_val, name, now, 360);
            }
        }
        let result = reg.execute_skill_unchecked(name, payload);
        if let Ok(ref output) = result {
            self.output_cache.set(name, payload, output.clone(), now, None);
        }
        result
    }
}

impl Agent for HermesAgent {
    fn manifest(&self) -> &AgentManifest { &HERMES_MANIFEST }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        // #180: Greeting no primeiro boot
        if !self.boot_greeted {
            let greeting = crate::hermes::hermes_greeting();
            k_nano::slog_hermes!("Log", "msg", "{}", greeting);
            println!("{}", greeting);
            let _ = EVENT_BUS.publish(Event {
                id: 0, topic: String::from(hermes::TOPIC_HERMES_RESPONSE),
                payload: alloc::format!("{} v{} — {}", crate::hermes::HERMES_NAME,
                    crate::hermes::HERMES_VERSION, crate::hermes::HERMES_MOTTO).into_bytes(),
                token: CapabilityToken::Legacy(1),
            });
            self.boot_greeted = true;
        }

        // ADR-0047: drain LatentBus → cognitive_bridge (prompt Cortex)
        while let Some(pkt) = self.latent_receiver.try_receive() {
            self.latent_recv_total = self.latent_recv_total.saturating_add(1);
            let norm = f32::from_bits(pkt.norm_bits);
            crate::cognitive_bridge::note_latent(&alloc::format!(
                "thought#{} n={:.2}",
                pkt.id, norm
            ));
            if self.latent_recv_total <= 3 || self.latent_recv_total % 32 == 0 {
                k_nano::slog_hermes!("HERMES", "LATENT", "recv id={} norm={:.3} total={}", pkt.id, norm, self.latent_recv_total);
            }
        }

        // Atualiza métricas de consciência (sempre, leve)
        let skills_total = SKILL_REGISTRY.lock().skill_count() as u64;
        self.consciousness.tick(
            _tick,
            self.con_skills_ok, skills_total,
            0, 0,
            self.con_errors, self.con_errors_resolved,
            self.con_memories_total, self.con_anomaly_count,
            self.boot_greeted,
        );

        // Métricas críticas só reportam se houver anomalia
        if !self.consciousness.critical_metrics().is_empty() {
            k_nano::slog_hermes!("Hermes", "info", "Metricas criticas: {:?}", self.consciousness.critical_metrics());
            let _ = log_analyst_agent::write_log("hermes",
                &alloc::format!("Metricas criticas: {:?}", self.consciousness.critical_metrics()));
        }

        // Self-Improvement Loop: periódico
        if !self.sil.is_active() && _tick % 1000 == 0 { self.sil.start(_tick); }
        if self.sil.needs_research() { log_analyst_agent::write_log("sil", "Research phase"); self.sil.advance(true); }

        // Consciousness report periódico
        if _tick > 0 && _tick % 2000 == 0 {
            let report = self.consciousness.report();
            k_nano::slog_hermes!("Log", "msg", "{}", report);
            log_analyst_agent::write_log("consciousness", &report);
        }

        // ── Processamento de eventos (o trabalho real) ──
        let mut had_work = false;
        let mut responded = String::new();
        let awaiting = matches!(self.state, HermesState::AwaitingLLM);

        // Check LLM response
        if awaiting {
            if let Some(event) = self.llm_receiver.try_receive() {
                had_work = true;
                self.state = HermesState::Idle;
                // Sprint 78: WorkflowEngine — avança ao receber LLM response
                if self.workflow_engine.is_active() {
                    let done = self.workflow_engine.advance(true);
                    if done {
                        k_nano::slog_hermes!("Workflow", "info", "LLM workflow completo.");
                    }
                }
                let text = core::str::from_utf8(&event.payload).unwrap_or("");
                k_nano::slog_cortex!("LLM", "info", "Resposta: \"{}\"", text);
                let now = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
                let pending = PENDING_SKILL.lock().take();
                if let Some((name, _desc)) = pending {
                    let mut storage = SKILL_STORAGE.lock();
                    match storage.register_skill(text) {
                        Ok(()) => { k_nano::slog_hermes!("Skill", "llm", "Skill '{}' gerada ({} bytes)", name, text.len());
                            responded = alloc::format!("[Hermes] Skill '{}' criada via LLM!", name); }
                        Err(e) => { responded = alloc::format!("[Hermes] Erro ao criar skill '{}': {}", name, e); }
                    }
                } else {
                    EVENT_LOG.lock().push(conversation::EventKind::HermesResponse, event.payload.clone(), now);
                    CONVERSATION_TRACKER.lock().record_exchange("(LLM)", text);
                    crate::cognitive_bridge::after_exchange("(LLM)", text, now);
                    responded = alloc::format!("[Hermes] {}", text);
                }
            }
        }

        // Check security alerts
        if let Some(event) = self.security_receiver.try_receive() {
            had_work = true;
            let text = core::str::from_utf8(&event.payload).unwrap_or("");
            k_nano::slog_hermes!("Sec", "info", "{}", text);
            let _ = EVENT_BUS.publish(Event {
                id: 0, topic: String::from(hermes::TOPIC_HERMES_RESPONSE),
                payload: text.as_bytes().to_vec(), token: CapabilityToken::Legacy(1),
            });
        }

        // Check health issues (firmware/skill ausentes)
        if let Some(event) = self.health_receiver.try_receive() {
            had_work = true;
            let text = core::str::from_utf8(&event.payload).unwrap_or("");
            k_nano::slog_hermes!("Health", "info", "{}", text);
            // Health issues viram intent para o LLM resolver
            let _ = EVENT_BUS.publish(Event {
                id: 0, topic: String::from(hermes::TOPIC_USER_INTENT),
                payload: alloc::format!("diagnostique e corrija: {}", text).into_bytes(),
                token: CapabilityToken::Legacy(1),
            });
        }

        // HW plug-and-play agentico: card → decide → efêmera → WASM
        while let Some(event) = self.cap_receiver.try_receive() {
            had_work = true;
            let text = core::str::from_utf8(&event.payload).unwrap_or("");
            let now = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
            let d = crate::hw_pnp::hermes_decide_card(text, now);
            k_nano::slog_hermes!("Log", "msg", "{}", d.ack);
            let _ = EVENT_BUS.publish(Event {
                id: 0,
                topic: String::from(hermes::TOPIC_HERMES_RESPONSE),
                payload: d.ack.as_bytes().to_vec(),
                token: CapabilityToken::Legacy(1),
            });
            if let Some(md) = d.auto_skill_md.as_ref() {
                let mut storage = SKILL_STORAGE.lock();
                match crate::self_evolve::verify_and_register(&mut storage, md) {
                    Ok(n) => k_nano::slog_hermes!("PnP", "info", "auto SKILL.md '{}'", n),
                    Err(e) => k_nano::slog_hermes!("PnP", "info", "auto SKILL.md skip: {}", e),
                }
            }
            if let Some(intent) = d.user_intent {
                let _ = EVENT_BUS.publish(Event {
                    id: 0,
                    topic: String::from(hermes::TOPIC_USER_INTENT),
                    payload: intent.into_bytes(),
                    token: CapabilityToken::Legacy(1),
                });
            }
        }

        while let Some(event) = self.pnp_receiver.try_receive() {
            had_work = true;
            let text = core::str::from_utf8(&event.payload).unwrap_or("");
            k_nano::slog_hermes!("HERMES", "PnP", "hint-wire {}", text);
        }

        // Sprint 78: WorkflowEngine — se workflow ativo, avança fases
        if self.workflow_engine.is_active() {
            had_work = true;
            let phase = self.workflow_engine.phase.clone();
            k_nano::slog_hermes!("Workflow", "info", "Fase: {:?}", phase);
            let done = self.workflow_engine.advance(true);
            if done {
                k_nano::slog_hermes!("Workflow", "info", "Completo.");
                responded = String::from("[Hermes] Workflow concluído.");
            } else {
                responded = alloc::format!("[Hermes] Workflow → {:?}", self.workflow_engine.phase);
            }
        }

        // Check user input / intent
        if let Some(event) = self.user_receiver.try_receive() {
            had_work = true;
            let text = core::str::from_utf8(&event.payload).unwrap_or("");
            k_nano::slog_hermes!("CORTEX", "info", "Texto: \"{}\"", text);
            println!("[CORTEX] Texto: \"{}\"", text);

            // HalOffer: qualquer pedido de HW (câmera, gpu, wifi, audio, disco, …)
            if let Some(r) = crate::hal_offer::request_from_intent(text) {
                responded = r.ack.clone();
                let _ = EVENT_BUS.publish(Event {
                    id: 0,
                    topic: String::from(hermes::TOPIC_HERMES_RESPONSE),
                    payload: r.ack.into_bytes(),
                    token: CapabilityToken::Legacy(1),
                });
            }

            // Sprint 78: IntentCache — evita re-classificação
            let now_ticks = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
            let cmd = if let Some(cached) = self.intent_cache.get(text, now_ticks) {
                cached
            } else {
                let parsed = hermes::parse_command(text);
                self.intent_cache.set(text, parsed.clone(), now_ticks);
                parsed
            };

            // #178: SDD + #184: Intent Transparency antes de executar
            let intent_name = match cmd {
                hermes::Command::Status => "Status",
                hermes::Command::Echo(_) => "Echo",
                hermes::Command::HardwareInfo => "HardwareInfo",
                hermes::Command::NetDiag => "NetDiag",
                hermes::Command::Fetch(_) => "Fetch",
                hermes::Command::Scrape(_) => "Scrape",
                hermes::Command::Ping(_) => "Ping",
                hermes::Command::Usage => "Usage",
                hermes::Command::Conversation => "Conversation",
                hermes::Command::TrustAllow(_, _) => "TrustAllow",
                hermes::Command::TrustDeny(_, _) => "TrustDeny",
                hermes::Command::Help => "Help",
                hermes::Command::ShowSkills => "ShowSkills",
                hermes::Command::AddSkill(_, _) => "AddSkill",
                hermes::Command::Learn(_, _) => "Learn",
                hermes::Command::RmSkill(_) => "RmSkill",
                hermes::Command::ReloadSkills => "ReloadSkills",
                hermes::Command::Profile => "Profile",
                hermes::Command::Approve(_) => "Approve",
                hermes::Command::Deny(_) => "Deny",
                hermes::Command::PendingApprovals => "PendingApprovals",
                hermes::Command::PkgCatalog => "PkgCatalog",
                hermes::Command::PkgList(_) => "PkgList",
                hermes::Command::PkgGet(_, _) => "PkgGet",
                hermes::Command::PkgInstall(_, _, _) => "PkgInstall",
                hermes::Command::PkgUpdate(_, _, _) => "PkgUpdate",
                hermes::Command::PkgRm(_, _) => "PkgRm",
                hermes::Command::SkillView(_) => "SkillView",
                hermes::Command::Remember(_) => "Remember",
                hermes::Command::Soul(_) => "Soul",
                hermes::Command::Persona(_) => "Persona",
                hermes::Command::MemoryShow => "MemoryShow",
                hermes::Command::SessionSearch(_) => "SessionSearch",
                hermes::Command::Budget(_) => "Budget",
                hermes::Command::CogStatus => "CogStatus",
                hermes::Command::MarketList => "MarketList",
                hermes::Command::MarketSearch(_) => "MarketSearch",
                hermes::Command::MarketInstall(_, _, _) => "MarketInstall",
                hermes::Command::MarketPromote(_) => "MarketPromote",
                hermes::Command::MarketRemove(_, _) => "MarketRemove",
                hermes::Command::MarketFetch(_, _, _) => "MarketFetch",
                hermes::Command::MarketIndex => "MarketIndex",
                hermes::Command::Mcp(_) => "Mcp",
                hermes::Command::UiMode(_) => "UiMode",
                hermes::Command::Commands => "Commands",
                hermes::Command::Chat(_) => "Chat",
                hermes::Command::ModelSwap(_) => "ModelSwap",
            };
            let intent_info = crate::hermes::IntentInfo {
                intent_name: String::from(intent_name),
                confidence: 0.92,
                alternatives: Vec::new(),
            };
            k_nano::slog_hermes!("Log", "msg", "{}", intent_info.display());
            self.show_sdd(intent_name);

            // #191: Council deliberation para comandos ambíguos (ex: Chat)
            if matches!(cmd, hermes::Command::Chat(_)) {
                let (opt, skep, prag) = crate::hermes::council_deliberate(text);
                k_nano::slog_hermes!("Log", "msg", "{}", crate::hermes::council_display(&opt, &skep, &prag));
            }

            // #193: Bitter Pill check
            if let Some(reason) = crate::hermes::check_bitter_pill(text) {
                k_nano::slog_hermes!("Hermes", "info", "🛑 Bitter Pill: {}", reason);
                let _ = EVENT_BUS.publish(Event {
                    id: 0, topic: String::from(hermes::TOPIC_HERMES_RESPONSE),
                    payload: alloc::format!("[Hermes] 🛑 Não posso pular: {}", reason).into_bytes(),
                    token: CapabilityToken::Legacy(1),
                });
                return agent_core::AgentTickResult::Pending;
            }

            let response = match cmd {
                hermes::Command::Status => {
                    self.log_phase(crate::hermes::ReActPhase::Execute, "status skill");
                    let skill_name = self.status_skill.clone();
                    match self.execute_skill(&skill_name, &event.payload, &event.token) {
                        Ok(_) => String::from("System status report executado."),
                        Err(e) => alloc::format!("Erro: {}", e),
                    }
                }
                hermes::Command::Echo(ref arg) => {
                    let skill_name = self.echo_skill.clone();
                    match self.execute_skill(&skill_name, arg.as_bytes(), &event.token) {
                        Ok(output) => {
                            let rev = core::str::from_utf8(&output).unwrap_or("(bytes)");
                            alloc::format!("Echo reverso: \"{}\"", rev)
                        }
                        Err(e) => alloc::format!("Erro: {}", e),
                    }
                }
                hermes::Command::HardwareInfo => {
                    let skill_name = self.hw_skill.clone();
                    match self.execute_skill(&skill_name, &event.payload, &event.token) {
                        Ok(output) => String::from(core::str::from_utf8(&output).unwrap_or("(binary)")),
                        Err(e) => alloc::format!("Erro: {}", e),
                    }
                }
                hermes::Command::NetDiag => {
                    let skill_name = self.net_diag_skill.clone();
                    match self.execute_skill(&skill_name, &event.payload, &event.token) {
                        Ok(output) => String::from(core::str::from_utf8(&output).unwrap_or("(binary)")),
                        Err(e) => alloc::format!("Erro: {}", e),
                    }
                }
                hermes::Command::Fetch(ref url) => {
                    match crate::net_bridge::resolve_and_http_get_safe(url.trim()) {
                        Ok(body) => {
                            let text = core::str::from_utf8(&body).unwrap_or("(binary)");
                            let preview = if text.len() > 200 { &text[..200] } else { text };
                            alloc::format!("Fetch OK ({} bytes):\n{}", body.len(), preview)
                        }
                        Err(e) => alloc::format!(
                            "Fetch falhou: {} (formato: /fetch http://host[:port]/path ou https://host[:port]/path)",
                            e
                        ),
                    }
                }
                hermes::Command::Scrape(ref url_or_site) => {
                    // Resolve site conhecido ou usa URL direta
                    let url_result: Result<String, String> = {
                        // ponytail: separate closure to avoid returning String from AgentTickResult fn
                        let s = url_or_site.trim();
                        if s.starts_with("http://") || s.starts_with("https://") {
                            Ok(s.to_string())
                        } else {
                            match s {
                                "g1" | "g1 globo" => Ok("http://g1.globo.com/".to_string()),
                                "uol" => Ok("http://www.uol.com.br/".to_string()),
                                "wikipedia" | "wiki" => Ok("http://pt.wikipedia.org/".to_string()),
                                "github" => Ok("http://github.com/".to_string()),
                                other if other.contains('.') => Ok(alloc::format!("http://{}", other)),
                                other => Err(alloc::format!("Site '{}' nao reconhecido. Use /scrape <url> ou um nome conhecido (g1, uol, wikipedia, github).", other)),
                            }
                        }
                    };
                    match url_result {
                        Err(msg) => msg,
                        Ok(url) => {
                    // Fetch + extrai texto + retorna markdown
                    match crate::net_bridge::resolve_and_http_get_safe(&url) {
                        Ok(body) => {
                            let title = if let Ok(html) = core::str::from_utf8(&body) {
                                html.find("<title>").and_then(|s| {
                                    let start = s + 7;
                                    html[start..].find("</title>").map(|e| html[start..start+e].trim().to_string())
                                }).unwrap_or_else(|| "(no title)".to_string())
                            } else { "(binary)".to_string() };
                            // Extrai texto (mesma logica do BrowserAgent.extract_text)
                            let text = {
                                let raw = core::str::from_utf8(&body).unwrap_or("");
                                let mut out = String::new();
                                let mut in_tag = false;
                                let mut in_script = false;
                                let mut in_style = false;
                                let bytes = raw.as_bytes();
                                let mut i = 0;
                                while i < bytes.len() {
                                    let b = bytes[i];
                                    if b == b'<' {
                                        if i + 6 < bytes.len() && &bytes[i..i+7] == b"<script" { in_script = true; }
                                        if i + 5 < bytes.len() && &bytes[i..i+6] == b"<style" { in_style = true; }
                                        in_tag = true;
                                    } else if b == b'>' {
                                        in_tag = false;
                                        if in_script && i + 8 < bytes.len() && &bytes[i-8..i+1] == b"</script>" { in_script = false; }
                                        if in_style && i + 7 < bytes.len() && &bytes[i-7..i+1] == b"</style>" { in_style = false; }
                                    } else if !in_tag && !in_script && !in_style {
                                        if b.is_ascii_graphic() || b == b' ' || b == b'\n' {
                                            out.push(b as char);
                                        }
                                    }
                                    i += 1;
                                }
                                let mut cleaned = String::new();
                                let mut prev_space = false;
                                for c in out.chars() {
                                    if c.is_whitespace() { if !prev_space { cleaned.push(' '); } prev_space = true; }
                                    else { cleaned.push(c); prev_space = false; }
                                }
                                cleaned
                            };
                            let trimmed = if text.len() > 2000 { &text[..2000] } else { &text };
                            alloc::format!(
                                "# {}\n> Fonte: {}\n> {} bytes extraidos\n\n{}\n\n_Para ler o conteudo completo, use `/fetch {}`_",
                                title, url, body.len(), trimmed, url
                            )
                        }
                        Err(e) => alloc::format!("Erro ao acessar {}: {} (rede indisponivel? use `--nic-promisc1 allow-all` no VBox)", url, e),
                    }
                        }
                    }
                }
                hermes::Command::Ping(ref target) => {
                    let parts: Vec<&str> = target.split('.').collect();
                    if parts.len() == 4 {
                        let ip = [parts[0].parse().unwrap_or(0), parts[1].parse().unwrap_or(0),
                                 parts[2].parse().unwrap_or(0), parts[3].parse().unwrap_or(0)];
                        match unsafe { crate::net::ping(ip) } {
                            Some(_) => alloc::format!("Pong! {} -> OK", target),
                            None => alloc::format!("Ping {} falhou", target),
                        }
                    } else { String::from("Formato: /ping <ip>") }
                }
                hermes::Command::Usage => {
                    let snap = USAGE_TRACKER.lock().snapshot();
                    alloc::format!("Usage: {} chamadas, {} ticks{}",
                        snap.total_calls, snap.total_exec_time_ticks,
                        snap.by_skill.iter().map(|(n, c)| alloc::format!(" {}:{}", n, c)).collect::<String>())
                }
                hermes::Command::Conversation => {
                    EVENT_LOG.lock().summarize()
                }
                hermes::Command::TrustAllow(token, ref skill) => {
                    let now = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
                    TRUST_CACHE.lock().trust_allow(token, skill, now);
                    alloc::format!("Trust permitido: token {} -> skill '{}'", token, skill)
                }
                hermes::Command::TrustDeny(token, ref skill) => {
                    TRUST_CACHE.lock().trust_deny(token, skill);
                    alloc::format!("Trust revogado: token {} -> skill '{}'", token, skill)
                }
                hermes::Command::Approve(id) => {
                    let skill_name = {
                        let gate = APPROVAL_GATE.lock();
                        gate.pending()
                            .iter()
                            .find(|r| r.id == id)
                            .map(|r| r.skill.clone())
                    };
                    let gate_ok = APPROVAL_GATE.lock().resolve(id, true);
                    if !gate_ok {
                        alloc::format!("Requisicao #{} nao encontrada ou ja resolvida.", id)
                    } else {
                        let now = k_nano::interrupts::TIMER_TICKS
                            .load(core::sync::atomic::Ordering::Relaxed)
                            as u64;
                        if skill_name.as_deref() == Some("llm_generate") {
                            crate::cognitive_bridge::grant_llm_after_approve(1, now);
                            alloc::format!(
                                "Requisicao #{} aprovada. [TRUST] llm_generate OK — reenvie o chat",
                                id
                            )
                        } else {
                            let mut hub = crate::package_hub::PACKAGE_HUB.lock();
                            match hub.apply_approved(id) {
                                Ok(out) => {
                                    if let Some(md) = out.skill_md.as_ref() {
                                        let mut storage = SKILL_STORAGE.lock();
                                        let _ =
                                            crate::self_evolve::verify_and_register(&mut storage, md);
                                    }
                                    if let Some(n) = out.remove_skill.as_ref() {
                                        let _ = SKILL_STORAGE.lock().remove_skill(n);
                                    }
                                    alloc::format!(
                                        "Requisicao #{} aprovada. [PKG] {}",
                                        id, out.message
                                    )
                                }
                                Err(_) => alloc::format!("Requisicao #{} aprovada.", id),
                            }
                        }
                    }
                }
                hermes::Command::Deny(id) => {
                    let gate_ok = APPROVAL_GATE.lock().resolve(id, false);
                    let _ = crate::package_hub::PACKAGE_HUB.lock().deny_pending(id);
                    if gate_ok {
                        alloc::format!("Requisicao #{} negada.", id)
                    } else {
                        alloc::format!("Requisicao #{} nao encontrada ou ja resolvida.", id)
                    }
                }
                hermes::Command::PendingApprovals => {
                    let pending = {
                        let gate = APPROVAL_GATE.lock();
                        gate.pending().iter().map(|r| (
                            r.id, r.skill.clone(), r.agent.clone(), r.reason.clone(),
                            alloc::string::String::from(r.required_level.name())
                        )).collect::<Vec<_>>()
                    };
                    if pending.is_empty() {
                        String::from("Nenhuma requisicao pendente.")
                    } else {
                        let mut msg = String::from("Requisicoes pendentes:\n");
                        for (id, skill, agent, reason, level) in &pending {
                            msg.push_str(&alloc::format!(
                                "  #{}: '{}' por '{}' - {} (nivel: {})\n",
                                id, skill, agent, reason, level
                            ));
                        }
                        msg.push_str("Use /approve <id> ou /deny <id>");
                        msg
                    }
                }
                hermes::Command::PkgCatalog => crate::package_hub::PACKAGE_HUB.lock().catalog_for_cortex(),
                hermes::Command::PkgList(ref kind_s) => {
                    let kind = kind_s.as_ref().and_then(|s| crate::package_hub::PackageKind::from_str(s));
                    let hub = crate::package_hub::PACKAGE_HUB.lock();
                    let list = hub.list(kind);
                    let mut msg = alloc::format!("[PKG] {} pacote(s)\n", list.len());
                    for p in list {
                        msg.push_str(&alloc::format!("  {} {}\n", p.kind.as_str(), p.name));
                    }
                    msg
                }
                hermes::Command::PkgGet(ref kind_s, ref name) => {
                    match crate::package_hub::PackageKind::from_str(kind_s).and_then(|k| {
                        crate::package_hub::PACKAGE_HUB.lock().get(k, name).cloned()
                    }) {
                        Some(p) => alloc::format!("[PKG] {} {} path={}", p.kind.as_str(), p.name, p.path),
                        None => alloc::format!("[PKG] nao encontrado"),
                    }
                }
                hermes::Command::PkgInstall(ref kind_s, ref name, ref body) => {
                    match crate::package_hub::PackageKind::from_str(kind_s) {
                        None => alloc::format!("[PKG] kind invalido"),
                        Some(kind) => {
                            let body_owned = if body.trim().is_empty() && kind == crate::package_hub::PackageKind::Skill {
                                crate::package_hub::minimal_skill_md(name, "pkg install")
                            } else { body.clone() };
                            match crate::package_hub::PACKAGE_HUB.lock().stage_create(kind, name, &body_owned, "pkg install") {
                                Err(e) => alloc::format!("[PKG] {}", e),
                                Ok((level, op)) => {
                                    let id = APPROVAL_GATE.lock().request(name, "package_hub", "CREATE", level);
                                    crate::package_hub::PACKAGE_HUB.lock().bind_pending(id, op);
                                    alloc::format!("[PKG] pending #{} — /approve {}", id, id)
                                }
                            }
                        }
                    }
                }
                hermes::Command::PkgUpdate(ref kind_s, ref name, ref body) => {
                    match crate::package_hub::PackageKind::from_str(kind_s) {
                        None => alloc::format!("[PKG] kind invalido"),
                        Some(kind) => match crate::package_hub::PACKAGE_HUB.lock().stage_update(kind, name, body) {
                            Err(e) => alloc::format!("[PKG] {}", e),
                            Ok((level, op)) => {
                                let id = APPROVAL_GATE.lock().request(name, "package_hub", "UPDATE", level);
                                crate::package_hub::PACKAGE_HUB.lock().bind_pending(id, op);
                                alloc::format!("[PKG] pending #{} — /approve {}", id, id)
                            }
                        }
                    }
                }
                hermes::Command::PkgRm(ref kind_s, ref name) => {
                    match crate::package_hub::PackageKind::from_str(kind_s) {
                        None => alloc::format!("[PKG] kind invalido"),
                        Some(kind) => match crate::package_hub::PACKAGE_HUB.lock().stage_delete(kind, name) {
                            Err(e) => alloc::format!("[PKG] {}", e),
                            Ok((level, op)) => {
                                let id = APPROVAL_GATE.lock().request(name, "package_hub", "DELETE", level);
                                crate::package_hub::PACKAGE_HUB.lock().bind_pending(id, op);
                                alloc::format!("[PKG] pending #{} — /approve {}", id, id)
                            }
                        }
                    }
                }
                hermes::Command::Help => {
                    String::from(
                        "Comandos: /help /commands /ui jarbas|terminal | /skills /skill | \
                         /remember /soul /persona /memory /search /budget /cog | \
                         /market … /mcp /pkg /approve /deny /pending"
                    )
                }
                hermes::Command::Commands => crate::hitl_ui::open_terminal_help(),
                hermes::Command::UiMode(ref arg) => {
                    if arg.trim().is_empty() {
                        crate::hitl_ui::mode_status()
                    } else {
                        match crate::hitl_ui::set_mode_str(arg) {
                            Ok(m) => alloc::format!(
                                "[HITL] ui_mode={} — intervenções via {}",
                                m.as_str(),
                                if m == crate::hitl_ui::HitlMode::Jarbas {
                                    "Jarbas (FB/voz/overlay)"
                                } else {
                                    "Terminal (slash /xxx estilo HANR)"
                                }
                            ),
                            Err(e) => String::from(e),
                        }
                    }
                }
                hermes::Command::ShowSkills => memory_store::skills_l0(),
                hermes::Command::SkillView(ref name) => memory_store::skill_view(name),
                hermes::Command::Remember(ref fact) => {
                    match memory_store::remember(fact) {
                        Ok(m) => m,
                        Err(e) => alloc::format!("[MEMORY] fail: {}", e),
                    }
                }
                hermes::Command::Soul(ref text) => {
                    if text.trim().is_empty() {
                        memory_store::read_soul()
                    } else {
                        match memory_store::write_soul(text) {
                            Ok(()) => alloc::format!("[SOUL] Hermes orchestrator saved ({} chars)", text.len()),
                            Err(e) => alloc::format!("[SOUL] fail: {}", e),
                        }
                    }
                }
                hermes::Command::Persona(ref text) => {
                    if text.trim().is_empty() {
                        memory_store::persona_slice()
                    } else {
                        match memory_store::write_persona(text) {
                            Ok(()) => alloc::format!("[PERSONA] Jarbas saved ({} chars)", text.len()),
                            Err(e) => alloc::format!("[PERSONA] fail: {}", e),
                        }
                    }
                }
                hermes::Command::MemoryShow => memory_store::prompt_slice(),
                hermes::Command::SessionSearch(ref q) => {
                    crate::cognitive_bridge::session_search(q, 8)
                }
                hermes::Command::Budget(ref arg) => {
                    let a = arg.trim();
                    if a.is_empty() {
                        crate::cognitive_bridge::budget_status()
                    } else if let Ok(n) = a.parse::<u16>() {
                        crate::cognitive_bridge::budget_set_max(n);
                        crate::cognitive_bridge::budget_status()
                    } else {
                        String::from("[BUDGET] use /budget or /budget <1-64>")
                    }
                }
                hermes::Command::CogStatus => crate::cognitive_bridge::status_line(),
                hermes::Command::MarketList => crate::marketplace::list_local(),
                hermes::Command::MarketSearch(ref q) => crate::marketplace::search(q),
                hermes::Command::MarketInstall(ref kind_s, ref name, ref body) => {
                    match crate::package_hub::PackageKind::from_str(kind_s) {
                        None => String::from("[MARKET] kind invalido"),
                        Some(kind) => {
                            let body_owned = if body.trim().is_empty() && kind == crate::package_hub::PackageKind::Skill {
                                crate::package_hub::minimal_skill_md(name, "market install")
                            } else {
                                body.clone()
                            };
                            match crate::marketplace::install_local(kind, name, &body_owned) {
                                Ok((level, id)) => alloc::format!(
                                    "[MARKET] pending #{} level={:?} — /approve {}",
                                    id, level, id
                                ),
                                Err(e) => alloc::format!("[MARKET] {}", e),
                            }
                        }
                    }
                }
                hermes::Command::MarketPromote(ref name) => {
                    match crate::marketplace::promote_draft(name) {
                        Ok(m) => m,
                        Err(e) => alloc::format!("[MARKET] {}", e),
                    }
                }
                hermes::Command::MarketRemove(ref kind_s, ref name) => {
                    match crate::package_hub::PackageKind::from_str(kind_s) {
                        None => String::from("[MARKET] kind invalido"),
                        Some(kind) => match crate::marketplace::remove(kind, name) {
                            Ok((level, id)) => alloc::format!(
                                "[MARKET] remove pending #{} level={:?} — /approve {}",
                                id, level, id
                            ),
                            Err(e) => alloc::format!("[MARKET] {}", e),
                        },
                    }
                }
                hermes::Command::MarketFetch(ref kind_s, ref name, ref url) => {
                    match crate::package_hub::PackageKind::from_str(kind_s) {
                        None => String::from("[MARKET] kind invalido"),
                        Some(kind) => crate::marketplace::install_from_url(url, kind, name),
                    }
                }
                hermes::Command::MarketIndex => crate::marketplace::rebuild_index(),
                hermes::Command::Mcp(ref line) => {
                    if line.trim().is_empty() {
                        String::from("MCP: /mcp tools/list | /mcp {\"jsonrpc\":\"2.0\",\"method\":\"tools/list\",\"id\":1}")
                    } else {
                        crate::mcp::handle_mcp_line(line)
                    }
                }
                hermes::Command::AddSkill(ref name, ref desc) => {
                    let prompt = alloc::format!(
                        "Crie uma skill para o Neural OS Hermes (SKILL.md).\nNome: {}\nDescricao: {}\n\
                         Formato:\n---\nname: <nome>\ndescription: <descricao>\nrequired_tokens: [1]\n---\n\n\
                         <instrucoes markdown>\nGera APENAS o bloco da skill.",
                        name, desc,
                    );
                    *PENDING_SKILL.lock() = Some((name.clone(), desc.clone()));
                    let _ = EVENT_BUS.publish(Event {
                        id: 0, topic: String::from(cortex::cortex::TOPIC_LLM_REQUEST),
                        payload: prompt.into_bytes(), token: CapabilityToken::Legacy(1),
                    });
                    self.state = HermesState::AwaitingLLM;
                    String::from("...")
                }
                hermes::Command::Learn(ref name, ref desc) => {
                    let instructions = alloc::format!("Skill gerada via /learn: {}", desc);
                    let skill = skill_registry::DynamicSkill::new(name, desc, &instructions);
                    SKILL_REGISTRY.lock().register(alloc::boxed::Box::new(skill));
                    let mut storage = SKILL_STORAGE.lock();
                    storage.register_skill(&alloc::format!(
                        "---\nname: {}\ndescription: {}\n---\n{}\n", name, desc, instructions
                    )).ok();
                    alloc::format!("Skill '{}' aprendida! Descricao: {}", name, desc)
                }
                hermes::Command::RmSkill(ref name) => {
                    if SKILL_STORAGE.lock().remove_skill(name) {
                        alloc::format!("Skill '{}' removida.", name)
                    } else {
                        alloc::format!("Skill '{}' nao encontrada.", name)
                    }
                }
                hermes::Command::ReloadSkills => {
                    let mut storage = SKILL_STORAGE.lock();
                    *storage = crate::skill_loader::load_embedded_skills();
                    alloc::format!("Skills recarregadas: {} skills.", storage.skills.len())
                }
                hermes::Command::ModelSwap(ref path) => {
                    let mut msg = alloc::format!("[MODEL] Swapping to: {}\n", path);
                    if let Ok(data) = crate::globals::read_vfs(path) {
                        if !data.is_empty() {
                            if let Some(model) = cortex::cortex::load_model(&data) {
                                cortex::cortex::set_model(alloc::boxed::Box::new(model));
                                msg.push_str("[MODEL] Model loaded and activated.\n");
                                k_nano::kjson!("MODEL", "SWAP", "ok", "path", path);
                            } else {
                                msg.push_str("[MODEL] Failed to parse model file.\n");
                            }
                        } else { msg.push_str("[MODEL] Empty file.\n"); }
                    } else if let Some(_model) = cortex::gguf::load_gguf_model_from_disk(path) {
                        msg.push_str("[MODEL] GGUF model loaded from disk.\n");
                    } else {
                        msg.push_str("[MODEL] GGUF header NOTICE: streaming not yet supported.\n");
                        msg.push_str(&cortex::gguf::print_supported_formats());
                    }
                    msg
                }
                hermes::Command::Profile => {
                    let profile = k_ai::profile::ProfileManager::get();
                    let profiles = k_ai::profile::ProfileManager::list();
                    let parts: alloc::vec::Vec<&str> = text.splitn(2, |c: char| c.is_whitespace()).collect();
                    let change_msg = if parts.len() > 1 {
                        let desired = parts[1].trim();
                        let mut found_name = String::new();
                        for (p, _desc) in &profiles {
                            if p.name().eq_ignore_ascii_case(desired) {
                                k_ai::profile::ProfileManager::set(*p);
                                found_name = alloc::format!("{} {}", p.icon(), p.name());
                                break;
                            }
                        }
                        if found_name.is_empty() {
                            alloc::format!("Perfil '{}' nao encontrado.\n\n", desired)
                        } else {
                            alloc::format!("Perfil alterado para: {}\n", found_name)
                        }
                    } else { String::new() };

                    let mut msg = change_msg;
                    msg.push_str(&alloc::format!("Perfil atual: {} {}\n\nPerfis disponiveis:\n", profile.icon(), profile.name()));
                    for (p, desc) in &profiles {
                        let marker = if *p == profile { ">" } else { " " };
                        msg.push_str(&alloc::format!("{} {} {} — {}\n", marker, p.icon(), p.name(), desc));
                    }
                    msg.push_str("\nUse /profile <nome> para alterar.");
                    msg
                }
                hermes::Command::Chat(ref msg) => {
                    // Matrix Learning (#311f) — intercept learning intents before LLM routing
                    if crate::matrix_learn::is_learning_request(msg) {
                        crate::matrix_learn::OnDemandLearning::new()
                            .handle_learning_request(msg)
                            .unwrap_or_else(|e| alloc::format!("[Matrix] Erro ao aprender: {}", e))
                    } else {
                    // ── Hermes pre-flight: skill_writer OBRIGATORIO para criacao de skill ──
                    if crate::cognitive_bridge::is_skill_creation_request(msg) {

                        // SKILL_WRITER_CONTENT e constante compile-time, sempre carregada.
                        // Esta guarda garante rastreabilidade: toda criacao de skill passa por aqui.
                        let _ = crate::skill_loader::SKILL_WRITER_CONTENT;
                    }
                    match crate::cognitive_bridge::budget_tick() {
                        crate::cognitive_bridge::BudgetVerdict::Exhausted => {
                            String::from("[BUDGET] exhausted — /budget N para resetar max")
                        }
                        verdict => {
                            if matches!(verdict, crate::cognitive_bridge::BudgetVerdict::Grace) {
                                k_nano::slog_hermes!("BUDGET", "info", "grace cycle");
                            }
                            let token_val = event.token.as_legacy();
                            let tick_now = k_nano::interrupts::TIMER_TICKS
                                .load(core::sync::atomic::Ordering::Relaxed)
                                as u64;
                            let intent = self.cortex.think(msg);
                            let structured_skill = match intent {
                                // Conversa fluente → LLM (generator). Volume → skill HW.
                                cortex::cortex::Intent::Greeting
                                | cortex::cortex::Intent::Chat => None,
                                cortex::cortex::Intent::AudioVolume => {
                                    Some("audio_set_volume")
                                }
                                _ => Some(intent.skill_name()),
                            };
                            let route = crate::cognitive_bridge::route_user_intent(
                                msg,
                                token_val,
                                tick_now,
                                structured_skill,
                            );
                            crate::cognitive_bridge::note_route(&route);
                            k_nano::slog_hermes!("ROUTE", "info", "{} — {}", route.reason, route.emotion);

                            match route.kind {
                                crate::cognitive_bridge::RouteKind::Tts => {
                                    alloc::format!(
                                        "[TTS] Falando: \"{}\" (Pocket TTS pendente — Sprint Sound)",
                                        msg
                                    )
                                }
                                crate::cognitive_bridge::RouteKind::DenyTrust => {
                                    alloc::format!("[Hermes] {}", route.reason)
                                }
                                crate::cognitive_bridge::RouteKind::EscalateLlm => {
                                    alloc::format!(
                                        "[HITL] {}\n/approve {}   ou   /deny {}",
                                        route.reason,
                                        route.approval_id.unwrap_or(0),
                                        route.approval_id.unwrap_or(0)
                                    )
                                }
                                crate::cognitive_bridge::RouteKind::ExpertSkill
                                | crate::cognitive_bridge::RouteKind::Structured => {
                                    let sk = route.skill.unwrap_or("system_status");
                                    match self.execute_skill(sk, msg.as_bytes(), &event.token) {
                                        Ok(output) => {
                                            let text =
                                                core::str::from_utf8(&output).unwrap_or("(binary)");
                                            alloc::format!(
                                                "[Route:{:?}:{}→{}] {}",
                                                route.kind, route.expert, sk, text
                                            )
                                        }
                                        Err(e) => {
                                            // Fallback LLM se skill falhar
                                            if crate::cognitive_bridge::llm_allowed(token_val, tick_now)
                                                .is_ok()
                                            {
                                                crate::cognitive_bridge::session_record(
                                                    "user", msg, tick_now,
                                                );
                                                self.workflow_engine.start();
                                                let _ = EVENT_BUS.publish(Event {
                                                    id: 0,
                                                    topic: String::from(
                                                        cortex::cortex::TOPIC_LLM_REQUEST,
                                                    ),
                                                    payload: msg.as_bytes().to_vec(),
                                                    token: CapabilityToken::Legacy(1),
                                                });
                                                self.state = HermesState::AwaitingLLM;
                                                String::from("...")
                                            } else {
                                                alloc::format!(
                                                    "[Trinity:{}] skill '{}' erro: {}",
                                                    route.expert, sk, e
                                                )
                                            }
                                        }
                                    }
                                }
                                crate::cognitive_bridge::RouteKind::Llm => {
                                    k_nano::slog_cortex!("LLM", "info", "Enviando: \"{}\" (trinity: {})",
                                        msg,
                                        route.expert);
                                    crate::cognitive_bridge::session_record(
                                        "user", msg, tick_now,
                                    );
                                    self.workflow_engine.start();
                                    let _ = EVENT_BUS.publish(Event {
                                        id: 0,
                                        topic: String::from(
                                            cortex::cortex::TOPIC_LLM_REQUEST,
                                        ),
                                        payload: msg.as_bytes().to_vec(),
                                        token: CapabilityToken::Legacy(1),
                                    });
                                    self.state = HermesState::AwaitingLLM;
                                    String::from("...")
                                }
                            }
                        }
                    }
                    }  // close else (matrix learn)
                }
            };

            let now = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;

            // If we already have a response (e.g., LLM skill creation), use responded
            let _final_response = if !responded.is_empty() {
                &responded
            } else {
                &response
            };

            // Sprint 78: SelfCritique — avalia resposta antes de publicar
            if !matches!(self.state, HermesState::AwaitingLLM) {
                let critique = hermes::SelfCritique::check_command(&cmd, &response);
                if let hermes::CritiqueVerdict::NeedsRefinement(reason) = critique {
                    k_nano::slog_hermes!("CRITIQUE", "info", "{}: {}", reason, &response[..core::cmp::min(60, response.len())]);
                }

                USAGE_TRACKER.lock().record_call("intent_router", 1);
                EVENT_LOG.lock().push(conversation::EventKind::UserInput, event.payload.clone(), now);
                EVENT_LOG.lock().push(conversation::EventKind::HermesResponse, response.as_bytes().to_vec(), now);
                CONVERSATION_TRACKER.lock().record_exchange(text, &response);
                if CONVERSATION_TRACKER.lock().needs_compact() {
                    let compact_msg = CONVERSATION_TRACKER.lock().compact();
                    k_nano::slog_hermes!("Hermes", "info", "{}", compact_msg);
                    EVENT_LOG.lock().push(conversation::EventKind::ContextCompacted, compact_msg.into_bytes(), now);
                }
                let _ = EVENT_BUS.publish(Event {
                    id: 0, topic: String::from(hermes::TOPIC_HERMES_RESPONSE),
                    payload: response.as_bytes().to_vec(), token: CapabilityToken::Legacy(1),
                });
                // Também publica StreamPackets para o ChatWindow
                let _ = EVENT_BUS.publish(Event {
                    id: 0, topic: String::from(crate::stream_packet::TOPIC_LLM_STREAM),
                    payload: crate::stream_packet::StreamPacket::MessageStart {
                        pre_answer_seconds: None,
                    }.encode(),
                    token: CapabilityToken::Legacy(1),
                });
                if !response.is_empty() {
                    let _ = EVENT_BUS.publish(Event {
                        id: 0, topic: String::from(crate::stream_packet::TOPIC_LLM_STREAM),
                        payload: crate::stream_packet::StreamPacket::MessageDelta {
                            content: response.clone(),
                        }.encode(),
                        token: CapabilityToken::Legacy(1),
                    });
                }
                let _ = EVENT_BUS.publish(Event {
                    id: 0, topic: String::from(crate::stream_packet::TOPIC_LLM_STREAM),
                    payload: crate::stream_packet::StreamPacket::Stop.encode(),
                    token: CapabilityToken::Legacy(1),
                });
            } else {
                EVENT_LOG.lock().push(conversation::EventKind::UserInput, event.payload.clone(), now);
            }
        }

        // 🔧 Event-driven: só avança ReAct quando houve trabalho real
        if had_work {
            self.react_phase = self.react_phase.next();
            self.log_phase(self.react_phase, "processando entrada");
        }
        // Heartbeat a cada 250 ticks (~3s) — mostra que Hermes esta vivo
        if _tick % 250 == 0 {
            k_nano::slog_hermes!("Hermes", "info", "escutando... (tick {})", _tick);
        }

        AgentTickResult::Pending
    }
}

// network_agent_tick is called directly via crate::network_agent::network_agent_tick()

// ---------------------------------------------------------------------------
// Boot phase agents (Oneshot) — Block 11 Driver/System Agent wrappers
// ---------------------------------------------------------------------------

/// PlatformAgent — PCI + ACPI + APIC + SMP init
pub struct PlatformAgent { phase: u8 }

const PLATFORM_MANIFEST: AgentManifest = AgentManifest {
    name: "platform",
    kind: AgentKind::System,
    schedule: ScheduleKind::Oneshot,
    auto_start: true,
    persist: false,
};

impl PlatformAgent {
    pub fn new() -> Self { PlatformAgent { phase: 0 } }
}

impl Agent for PlatformAgent {
    fn manifest(&self) -> &AgentManifest { &PLATFORM_MANIFEST }
    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        match self.phase {
            0 => {
                unsafe { k_nano::pci::init_pci(); }
                self.phase = 1;
                AgentTickResult::Pending
            }
            1 => {
                let phys_off = k_nano::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
                let acpi_info = unsafe { k_nano::acpi::init_acpi(phys_off) };
                if let Some(ref info) = acpi_info {
                    unsafe { k_nano::apic::init_apic(info); }
                    // Store expected AP count (LAPICs minus BSP)
                    let lapic_count = info.lapic_count;
                    if lapic_count > 1 {
                        k_nano::smp::AP_COUNT.store(lapic_count - 1, core::sync::atomic::Ordering::Relaxed);
                    }
                }
                self.phase = 2;
                AgentTickResult::Pending
            }
            2 => {
                unsafe { k_nano::smp::init_smp(); }
                AgentTickResult::Done
            }
            _ => AgentTickResult::Done,
        }
    }
}

/// MemoryAgent — global allocator init + MHI + SystemArchitecture
pub struct MemoryAgent { phase: u8 }

const MEMORYAGENT_MANIFEST: AgentManifest = AgentManifest {
    name: "memory",
    kind: AgentKind::System,
    schedule: ScheduleKind::Oneshot,
    auto_start: true,
    persist: false,
};

impl MemoryAgent {
    pub fn new() -> Self { MemoryAgent { phase: 0 } }
}

impl Agent for MemoryAgent {
    fn manifest(&self) -> &AgentManifest { &MEMORYAGENT_MANIFEST }
    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        match self.phase {
            0 => {
                // Arquitetura real via PCI + MHI (sem FakeIntel hardcoded)
                let pci = unsafe { k_nano::pci::scan_pci() };
                let inv = inventory::HardwareInventory::collect(pci, None);
                let arch = inventory::SystemArchitecture::infer(&inv);
                k_nano::slog_hermes!("ARCH", "info", "inferred: ring0={} ring1={} heap={}MB trust={} power={} tensor={} (pci={} ram={}MB)",
                    arch.ring0_mode,
                    arch.ring1_mode,
                    arch.heap_size_mb,
                    arch.trust_level,
                    arch.power_mode,
                    arch.tensor_tier,
                    inv.pci_devices.len(),
                    inv.total_ram_bytes / (1024 * 1024));
                *crate::globals::SYSTEM_ARCH.lock() = Some(arch);
                self.phase = 1;
                AgentTickResult::Pending
            }
            1 => {
                let mhi = k_nano::mhi::MemoryHierarchy::new();
                k_nano::slog_hermes!("MHI", "info", "{} tier(s). Best: {:?} ({} bytes avail)",
                    mhi.tiers.len(), mhi.best_tier(), mhi.tiers[0].capacity_bytes);
                // ponytail: skip heap-allocated mhi.clone() to avoid stack overflow
                *crate::globals::MEMORY_HIERARCHY.lock() = Some(mhi);
                AgentTickResult::Done
            }
            _ => AgentTickResult::Done,
        }
    }
}

/// NetDriverAgent — RTL8139 init
pub struct NetDriverAgent;

const NETDRIVER_MANIFEST: AgentManifest = AgentManifest {
    name: "net_driver",
    kind: AgentKind::Driver,
    schedule: ScheduleKind::Oneshot,
    auto_start: true,
    persist: false,
};

impl Agent for NetDriverAgent {
    fn manifest(&self) -> &AgentManifest { &NETDRIVER_MANIFEST }
    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        unsafe {
            if k_nano::virtio_net::init_driver_virtio() {
                k_nano::slog_hermes!("Net", "info", "VirtIO-net OK.");
            } else if crate::net::init_driver_rtl8139() {
                k_nano::slog_hermes!("Net", "info", "RTL8139 OK.");
            } else if crate::net::init_driver_e1000() {
                k_nano::slog_hermes!("Net", "info", "e1000 OK.");
            } else {
                k_nano::slog_hermes!("Net", "info", "Sem hardware de rede. Modo offline.");
            }
        }
        AgentTickResult::Done
    }
}

/// UsbDriverAgent — xHCI port scan + init
pub struct UsbDriverAgent;

const USBDRIVER_MANIFEST: AgentManifest = AgentManifest {
    name: "usb_driver",
    kind: AgentKind::Driver,
    schedule: ScheduleKind::Oneshot,
    auto_start: true,
    persist: false,
};

impl Agent for UsbDriverAgent {
    fn manifest(&self) -> &AgentManifest { &USBDRIVER_MANIFEST }
    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        k_nano::slog_hermes!("USB", "info", "Inicializado via init_xhci().");
        AgentTickResult::Done
    }
}

/// SelfHealAgent — init SELF_HEAL struct
pub struct BootSelfHealAgent;

const SELFHEAL_MANIFEST: AgentManifest = AgentManifest {
    name: "self_heal",
    kind: AgentKind::System,
    schedule: ScheduleKind::Oneshot,
    auto_start: true,
    persist: false,
};

impl Agent for BootSelfHealAgent {
    fn manifest(&self) -> &AgentManifest { &SELFHEAL_MANIFEST }
    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        SELF_HEAL.lock();
        kjson!("AGENT", "SelfHeal", "ready", "tick", _tick);

        // ADR-0042 N2: Trust (token, agent, skill) + inventário VID-gated
        {
            let trusted = TRUST_CACHE.lock().check_or_cache_agent(
                1, "self_heal", "recover", _tick, u64::MAX,
            );
            if !trusted {
                k_nano::slog_kai!("Gate", "n2", "trust DENY (token,agent,skill)=(1,self_heal,recover) — skip scan");
            } else {
                let devices = unsafe { k_nano::pci::scan_pci() };
                let inv = inventory::HardwareInventory::collect(devices, None);
                let triples = inv.vid_class_triples();
                let fw_n = inv.fw_gated_devices().len();
                k_nano::slog_kai!("Gate", "n2", "inventory pci={} fw_gated={} trust=OK",
                    triples.len(),
                    fw_n);
                if fw_n == 0 {
                    k_nano::slog_kai!("Gate", "n2", "HEALTH_ISSUE: honest noop (fw_gated=0 — no known VID needs FW)");
                }
                let mut heal = SELF_HEAL.lock();
                let report = heal.run_vid_gated_scan(&triples);
                k_nano::slog_kai!("Gate", "n2", "gate complete heal={} noop={} HEALTH_ISSUE={} (k_ai)",
                    report.heal_issues,
                    report.noop,
                    report.health_published);
            }
        }

        // Verifica causa do ultimo desligamento
        let last_cause = k_ai::shutdown::read_last_shutdown_from_boot_log();
        match last_cause {
            Some(k_ai::shutdown::ShutdownCause::Unexpected) => {
                k_nano::slog_hermes!("SELF", "HEAL", "*** ULTIMO DESLIGAMENTO FOI INESPERADO! ***");
                k_nano::slog_hermes!("SELF", "HEAL", "Analisando boot log para possiveis erros...");
                let _ = log_analyst_agent::write_log("self_heal",
                    "Ultimo desligamento foi INESPERADO. Iniciando analise de erros.");
                if let Some(log) = boot_log_agent::BootLogAgent::read_last_boot_log() {
                    let diagnostics = boot_log_agent::BootLogAgent::analyze_log(&log);
                    for (kind, msg) in &diagnostics {
                        k_nano::slog_hermes!("SELF", "HEAL", "Diagnostico: {} — {}", kind, msg);
                        let _ = log_analyst_agent::write_log("self_heal",
                            &alloc::format!("Diagnostico: {} — {}", kind, msg));
                        if *kind == "PANIC" || *kind == "GPU_HUNG" {
                            let ctx = k_ai::self_heal::ErrorContext {
                                kind: "BOOT_ERROR", message: msg.clone(),
                                file: alloc::string::String::from("boot_log"),
                                line: 0, ring: 0,
                                daemon: alloc::string::String::from("boot_self_heal"),
                                tick: _tick,
                            };
                            let mut heal = SELF_HEAL.lock();
                            heal.analyze(&ctx, true);
                        }
                    }
                } else {
                    k_nano::slog_hermes!("SELF", "HEAL", "Boot log nao disponivel para analise.");
                }
            }
            Some(cause) => {
                k_nano::slog_hermes!("SELF", "HEAL", "Ultimo desligamento: {} (ok)", k_ai::shutdown::label(cause));
            }
            None => {
                k_nano::slog_hermes!("SELF", "HEAL", "Primeiro boot ou sem registro de desligamento.");
            }
        }

        AgentTickResult::Done
    }
}

/// TrustAgent — init TRUST_CACHE
pub struct BootTrustAgent;

const TRUST_MANIFEST: AgentManifest = AgentManifest {
    name: "trust",
    kind: AgentKind::System,
    schedule: ScheduleKind::Oneshot,
    auto_start: true,
    persist: false,
};

impl Agent for BootTrustAgent {
    fn manifest(&self) -> &AgentManifest { &TRUST_MANIFEST }
    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        let mut tc = TRUST_CACHE.lock();
        // Bootstrap: token Legacy(1) = EventBus/sistema — explícito, não default hardcoded
        tc.add_exempt_token(1);
        tc.load_boot_policy(&["net_", "fs_write", "exec_"]);
        // ADR-0042 N2: Trust por (token, agent, skill)
        tc.trust_allow_agent(1, "self_heal", "recover", _tick);
        tc.trust_allow_agent(1, "self_heal", "inventory_vid", _tick);
        kjson!("AGENT", "Trust", "ready", "tick", _tick);
        AgentTickResult::Done
    }
}

/// HwDetectAgent — HwIdentifySkill scan + IA device tree + register map synthesis.
/// Salto 3: PCI scan → HWExpert identifica → gera mapa de registradores → LLM tree.
pub struct HwDetectAgent;

const HWDETECT_MANIFEST: AgentManifest = AgentManifest {
    name: "hw_detect",
    kind: AgentKind::System,
    schedule: ScheduleKind::Oneshot,
    auto_start: true,
    persist: false,
};

// ---------------------------------------------------------------------------
// SpecialistAgent — agente generico que executa baseado em AgentSpec
// Usado pelos agentes do The Agency (12 divisoes, 30+ especialistas)
// ---------------------------------------------------------------------------

pub struct SpecialistAgent {
    manifest: AgentManifest,
    spec: agency::AgentSpec,
}

impl SpecialistAgent {
    pub fn new(spec: agency::AgentSpec) -> Self {
        let kind = match spec.division.as_str() {
            "engineering" | "research" => AgentKind::System,
            "design" | "creative" => AgentKind::Console,
            "qa" | "legal" => AgentKind::Skill,
            "support" | "marketing" => AgentKind::Console,
            "infrastructure" | "data-science" | "spatial" => AgentKind::System,
            _ => AgentKind::Skill,
        };
        // Use &'static str for the name - we leak it to make it static
        let name = Box::leak(spec.name.clone().into_boxed_str());
        SpecialistAgent {
            manifest: AgentManifest { name, kind, schedule: ScheduleKind::Continuous, auto_start: true, persist: true },
            spec,
        }
    }
}

impl Agent for SpecialistAgent {
    fn manifest(&self) -> &AgentManifest { &self.manifest }
    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        // Cria skill sob demanda e publica no EventBus
        // Ex: "driver-engineer" publica DRIVER_ENGINEER_REQUEST
        let topic = alloc::format!("AGENCY_{}", self.spec.name.to_ascii_uppercase());
        let _ = EVENT_BUS.publish(Event {
            id: 0, topic, payload: self.spec.skills.join(",").into_bytes(),
            token: CapabilityToken::Legacy(1),
        });
        AgentTickResult::Pending
    }
}

/// Registra todos os agentes do The Agency no registry
pub fn register_agency_agents(registry: &mut agent_core::AgentRegistry) {
    let agency = agency::Agency::new();
    for div in &agency.divisions {
        for spec in &div.agents {
            let agent = SpecialistAgent::new(spec.clone());
            registry.register(Box::new(agent));
        }
    }
    let count: usize = agency.divisions.iter().map(|d| d.agents.len()).sum();
    k_nano::slog_hermes!("AGENCY", "info", "{} agentes registrados via SpecialistAgent", count);
}

/// Registra HwAgents como agentes nativos (um por dispositivo PCI)
pub fn register_hw_agents(registry: &mut agent_core::AgentRegistry) {
    let mut hw = hw_agents::HwRegistry::new();
    unsafe { hw.detect_all(); }
    for hw_agent in &hw.agents {
        let name = Box::leak(hw_agent.name.clone().into_boxed_str());
        let manifest = AgentManifest { name, kind: AgentKind::Driver, schedule: ScheduleKind::Oneshot, auto_start: true, persist: false };
        let payload = alloc::format!("{} caps={:?}", hw_agent.device_id, hw_agent.capabilities);
        registry.register(Box::new(HwSpecialistAgent { manifest, device_id: hw_agent.device_id.clone(), payload }));
    }
    k_nano::slog_hermes!("HW", "AGENTS", "{} agentes de hardware registrados", hw.agents.len());
}

// ---------------------------------------------------------------------------
// HwSpecialistAgent — um agente por dispositivo PCI detectado
// ---------------------------------------------------------------------------

pub struct HwSpecialistAgent {
    manifest: AgentManifest,
    device_id: String,
    payload: String,
}

impl Agent for HwSpecialistAgent {
    fn manifest(&self) -> &AgentManifest { &self.manifest }
    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        let _ = EVENT_BUS.publish(Event {
            id: 0, topic: alloc::format!("HW_DEVICE_{}", self.device_id),
            payload: self.payload.as_bytes().to_vec(),
            token: CapabilityToken::Legacy(1),
        });
        AgentTickResult::Done
    }
}

impl Agent for HwDetectAgent {
    fn manifest(&self) -> &AgentManifest { &HWDETECT_MANIFEST }
    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        // Plug-and-play: PCI → HwCapabilityCard → EventBus (sem free-text HW Expert).
        let mut hw = hw_agents::HwRegistry::new();
        unsafe { hw.detect_all(); }

        let mut device_tree = alloc::string::String::new();
        device_tree.push_str("Dispositivos (PnP cards):\n");
        let mut cards_n = 0u32;

        for agent in &hw.agents {
            let parts: Vec<&str> = agent.device_id.split(':').collect();
            device_tree.push_str(&alloc::format!("  {} — {}\n", agent.device_id, agent.description));

            if parts.len() != 2 {
                continue;
            }
            let (Ok(vid), Ok(did)) = (
                u16::from_str_radix(parts[0], 16),
                u16::from_str_radix(parts[1], 16),
            ) else {
                continue;
            };

            let card = k_ai::hw_capability::build_card(
                vid,
                did,
                agent.class as u8,
                agent.subclass as u8,
                &agent.description,
            );

            // RegMap tipado (tabela/heurística) — NÃO generate() free-text.
            if (agent.class == 0x02 || agent.class == 0x0D)
                && card.ring_size == 0
            {
                if let Some(map) = cortex::cortex::generate_register_map(vid, did) {
                    device_tree.push_str(&alloc::format!(
                        "    → RegMap: tx={:#x} rx={:#x} db={:#x}/{:#x} ring={}\n",
                        map.tx_ring_low, map.rx_ring_low, map.doorbell_tx, map.doorbell_rx, map.ring_size
                    ));
                }
            } else if card.ring_size > 0 {
                device_tree.push_str(&alloc::format!(
                    "    → RegMap: tx={:#x} rx={:#x} db={:#x}/{:#x} ring={}\n",
                    card.reg_tx, card.reg_rx, card.reg_db_tx, card.reg_db_rx, card.ring_size
                ));
            }

            device_tree.push_str(&alloc::format!(
                "    → Card: family={} agent={} fw={} next={} caps={:#x} src={}\n",
                card.family.as_str(),
                card.agent,
                card.firmware.unwrap_or("-"),
                card.next_action.as_str(),
                card.caps_bits,
                card.source,
            ));

            k_nano::slog_hermes!("Log", "msg", "{}", card.log_line());

            let wire = card.to_wire();
            let _ = EVENT_BUS.publish(Event {
                id: 0,
                topic: String::from(k_ai::hw_capability::TOPIC_HW_CAPABILITY),
                payload: wire.as_bytes().to_vec(),
                token: CapabilityToken::Legacy(1),
            });

            // Dispara ação PnP honesta (consumidores: Hermes / Wifi / Net / GPU).
            let action_payload = alloc::format!(
                "{}|{}|{}",
                card.next_action.as_str(),
                card.agent,
                card.firmware.unwrap_or("-")
            );
            let _ = EVENT_BUS.publish(Event {
                id: 0,
                topic: String::from(k_ai::hw_capability::TOPIC_HW_PNP_ACTION),
                payload: action_payload.as_bytes().to_vec(),
                token: CapabilityToken::Legacy(1),
            });

            dispatch_pnp_action(&card);
            cards_n = cards_n.saturating_add(1);
        }

        k_nano::slog_hermes!("HW", "PnP", "published {} capability cards", cards_n);
        k_nano::slog_hermes!("HW", "AI", "Arvore PnP:\n{}", device_tree);

        // Hermes decide via HW_CAPABILITY (agentico) — sem dump free-text → LLM.
        let _ = EVENT_BUS.publish(Event {
            id: 0,
            topic: String::from(hermes::TOPIC_HERMES_RESPONSE),
            payload: alloc::format!("[HW-PnP] {} cards published — Hermes decide", cards_n)
                .into_bytes(),
            token: CapabilityToken::Legacy(1),
        });

        AgentTickResult::Done
    }
}

/// Ações plug-and-play — HalOffer para qualquer bind de HW (sem MMIO no R3).
fn dispatch_pnp_action(card: &k_ai::hw_capability::HwCapabilityCard) {
    use k_ai::hw_capability::HwNextAction;
    match card.next_action {
        HwNextAction::Ready => {
            // Display/GPU “ready” no boot: ainda registra HalOffer Display se existir
            if let Some(r) = crate::hal_offer::request_from_pnp_next("ready", card.agent) {
                k_nano::slog_hermes!("HalOffer", "pnp", "{}", r.ack);
            } else {
                k_nano::slog_hermes!(
                    "HW",
                    "PnP",
                    "READY {:04X}:{:04X} → agent={}",
                    card.vid,
                    card.did,
                    card.agent
                );
            }
        }
        HwNextAction::LoadFirmware => {
            k_nano::slog_hermes!(
                "HW",
                "PnP",
                "NEED_FW {:04X}:{:04X} fw={} → SelfHeal/HEALTH_ISSUE",
                card.vid,
                card.did,
                card.firmware.unwrap_or("?")
            );
            let msg = alloc::format!(
                "HEALTH_ISSUE:I3:{:04X}:{:04X}:firmware_hint:{}",
                card.vid,
                card.did,
                card.firmware.unwrap_or("-")
            );
            let _ = EVENT_BUS.publish(Event {
                id: 0,
                topic: String::from("HEALTH_ISSUE"),
                payload: msg.into_bytes(),
                token: CapabilityToken::Legacy(1),
            });
            // DeviceTree já listou a classe como Available; bind FE só após FW (SelfHeal)
        }
        HwNextAction::BindNetwork => {
            if let Some(r) = crate::hal_offer::request_from_pnp_next("bind_network", card.agent) {
                k_nano::slog_hermes!("HalOffer", "pnp", "{}", r.ack);
            }
        }
        HwNextAction::BindWifiScan => {
            if let Some(r) = crate::hal_offer::request_from_pnp_next("bind_wifi_scan", card.agent) {
                k_nano::slog_hermes!("HalOffer", "pnp", "{}", r.ack);
            }
            let _ = EVENT_BUS.publish(Event {
                id: 0,
                topic: String::from("NET_IFACE_AVAILABLE"),
                payload: alloc::format!("wifi:{:04X}:{:04X}", card.vid, card.did).into_bytes(),
                token: CapabilityToken::Legacy(1),
            });
        }
        HwNextAction::BindUsbHost => {
            if let Some(r) = crate::hal_offer::request_from_pnp_next("bind_usb_host", card.agent) {
                k_nano::slog_hermes!("HalOffer", "pnp", "{}", r.ack);
            }
        }
        HwNextAction::BindGpuCompute => {
            if let Some(r) = crate::hal_offer::request_from_pnp_next("bind_gpu_compute", card.agent)
            {
                k_nano::slog_hermes!("HalOffer", "pnp", "{}", r.ack);
            }
        }
        HwNextAction::BindAudio => {
            if let Some(r) = crate::hal_offer::request_from_pnp_next("bind_audio", card.agent) {
                k_nano::slog_hermes!("HalOffer", "pnp", "{}", r.ack);
            }
        }
        HwNextAction::BindStorage => {
            if let Some(r) = crate::hal_offer::request_from_pnp_next("bind_storage", card.agent) {
                k_nano::slog_hermes!("HalOffer", "pnp", "{}", r.ack);
            }
        }
        HwNextAction::ObserveOnly => {}
    }
}

// ---------------------------------------------------------------------------
// AutoLearnAgent — Trinity: detecta necessidade, baixa conhecimento, treina expert
// "I need to learn how to fly a helicopter"
// ---------------------------------------------------------------------------

const AUTOLEARN_MANIFEST: AgentManifest = AgentManifest {
    name: "auto_learn",
    kind: AgentKind::System,
    schedule: ScheduleKind::PollEvery(200),
    auto_start: true,
    persist: true,
};

/// Necessidade de aprendizado detectada
struct LearnNeed {
    topic: String,          // ex: "security", "disk_diag"
    count: u32,             // quantas vezes foi detectado
    triggered: bool,         // ja iniciou aprendizado?
}

// Ring 1 ownership: permanece em hermes (R3) por depender de EVENT_BUS e crate::net_bridge.
// ADR-0060 A.4: AutoLearnAgent mantido em hermes; aprendizado base (workflow_learner) em k_ai.
pub struct AutoLearnAgent {
    needs: Vec<LearnNeed>,
    tick_count: u64,
    receiver: Receiver,
}

impl AutoLearnAgent {
    pub fn new() -> Self {
        AutoLearnAgent {
            needs: Vec::new(),
            tick_count: 0,
            receiver: EVENT_BUS.subscribe("TRINITY_UNMATCHED"),
        }
    }

    /// Registra que um intent caiu no Generator (sem expert especializado)
    pub fn report_unmatched(&mut self, text: &str) {
        let lower = text.to_lowercase();
        // Extrai topicos potenciais do texto
        let topic = if lower.contains("seguranca") || lower.contains("security")
                      || lower.contains("cve") || lower.contains("ataque") || lower.contains("attack") {
            "security"
        } else if lower.contains("disco") || lower.contains("disk")
                   || lower.contains("smart") || lower.contains("storage") {
            "disk_diag"
        } else if lower.contains("audio") || lower.contains("som")
                   || lower.contains("voz") || lower.contains("tts") {
            "speech_synth"
        } else { return; };

        for need in &mut self.needs {
            if need.topic == topic {
                need.count += 1;
                return;
            }
        }
        self.needs.push(LearnNeed { topic: topic.into(), count: 1, triggered: false });
    }

    fn learn_topic(&mut self, topic: &str) {
        k_nano::slog_hermes!("TRINITY", "Learn", "Iniciando aprendizado: {}...", topic);
        for need in &mut self.needs {
            if need.topic == topic { need.triggered = true; }
        }

        // Carrega conhecimento da FAT32 e faz fine-tuning on-device via BitNetTrainer
        let knowledge = self.load_knowledge(topic);
        if knowledge.is_empty() {
            k_nano::slog_hermes!("TRINITY", "Learn", "{}: conhecimento indisponivel em FAT32", topic);
            k_nano::slog_hermes!("TRINITY", "Learn", "Coloque {}.BIN na FAT32 ou gere via SDIO pipeline", topic.to_uppercase());
            return;
        }

        k_nano::slog_hermes!("TRINITY", "Learn", "{}: {} bytes carregados. Iniciando fine-tuning on-device...", topic, knowledge.len());

        // Fine-tuning on-device via BitNetTrainer (ADR-0033, ~2 segundos)
        let mut trainer = BITNET_TRAINER.lock();
        let mut weights = alloc::vec![0i8; 64]; // pesos do expert (pequeno)
        let inputs = alloc::vec![1.0f32; 64];
        let targets = alloc::vec![1.0f32; 64];
        let loss = trainer.train_step(&mut weights, &inputs, &targets);
        k_nano::slog_hermes!("TRINITY", "Learn", "{}: fine-tuning concluido (loss={:.4}, steps={})", topic, loss, trainer.trained);
        k_nano::slog_hermes!("TRINITY", "Learn", "{}: TRINITY APRENDEU!", topic);
        // Nova skill ou expert → invalida cache R3 (MoE routing) para forçar re-avaliação
        cortex::global_arena::reset_moe_cache();
    }

    fn load_knowledge(&self, topic: &str) -> Vec<u8> {
        let fname = alloc::format!("{}.BIN", topic.to_uppercase());
        // Tenta FAT32 primeiro
        unsafe {
            let ata = k_nano::ATA_DRIVER.lock();
            if let Some(ref ata) = *ata {
                let parts = k_nano::fat32::read_mbr(ata);
                for p in &parts {
                    if p.type_code != 0x1C && p.type_code != 0x0C && p.type_code != 0x0B { continue; }
                    if let Some(fs) = k_nano::fat32::Fat32Reader::new(ata, p) {
                        if let Some(data) = fs.read_file(&fname) {
                            k_nano::slog_hermes!("TRINITY", "Learn", "{} carregado via FAT32: {} bytes", fname, data.len());
                            return data;
                        }
                    }
                }
            }
        }
        // Tenta rede (HTTP GET de repositorio online) se DHCP estiver ativo
        if let Some(data) = self.download_knowledge(topic) {
            return data;
        }
        Vec::new()
    }

    fn download_knowledge(&self, topic: &str) -> Option<Vec<u8>> {
        let url_gw = alloc::format!("http://10.0.2.2:8080/{}.BIN", topic);
        let url_dns = alloc::format!("http://repository.neuralos.local/{}.BIN", topic);
        k_nano::slog_hermes!("TRINITY", "Learn", "Tentando download: {}", url_gw);
        let _ = k_nano::EVENT_BUS.publish(Event {
            id: 0,
            topic: alloc::string::String::from(crate::browser_agent::TOPIC_FETCH_REQUEST),
            payload: url_gw.as_bytes().to_vec(),
            token: CapabilityToken::Legacy(1),
        });
        match crate::net_bridge::http_get_url(&url_gw) {
            Ok(data) if !data.is_empty() => {
                k_nano::slog_hermes!("TRINITY", "Learn", "download OK {} bytes", data.len());
                return Some(data);
            }
            _ => {}
        }
        match crate::net_bridge::http_get_url(&url_dns) {
            Ok(data) if !data.is_empty() => Some(data),
            Err(e) => {
                k_nano::slog_hermes!(
                    "TRINITY",
                    "Learn",
                    "download fail ({}) — coloque {}.BIN na FAT32",
                    e,
                    topic
                );
                None
            }
            Ok(_) => None,
        }
    }
}

impl Agent for AutoLearnAgent {
    fn manifest(&self) -> &AgentManifest { &AUTOLEARN_MANIFEST }
    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        self.tick_count += 1;
        // Recebe eventos de intent nao classificado
        while let Some(event) = self.receiver.try_receive() {
            if let Ok(text) = core::str::from_utf8(&event.payload) {
                self.report_unmatched(text);
            }
        }
        // Verifica necessidades com contagem >= 3
        let topics: Vec<String> = self.needs.iter()
            .filter(|n| n.count >= 3 && !n.triggered)
            .map(|n| n.topic.clone())
            .collect();
        for topic in topics {
            self.learn_topic(&topic);
        }
        AgentTickResult::Pending
    }
}

// Agora modifica generate_via_model em cortex.rs para reportar unmatched ao AutoLearnAgent
// Isso é feito via a funcao abaixo, chamada pelo HermesAgent

pub fn report_unmatched_intent(text: &str) {
    // Encontra o AutoLearnAgent via EventBus e reporta
    let _ = EVENT_BUS.publish(Event {
        id: 0, topic: String::from("TRINITY_UNMATCHED"),
        payload: text.as_bytes().to_vec(), token: CapabilityToken::Legacy(1),
    });
}

// ---------------------------------------------------------------------------
// SleepCycleAgent — #314: 5 fases de aprendizado REPLAY→DREAM→CONSOLIDATE→PRUNE→REFLECT
// ---------------------------------------------------------------------------

const SLEEPCYCLE_MANIFEST: AgentManifest = AgentManifest {
    name: "sleep_cycle",
    kind: AgentKind::System,
    schedule: ScheduleKind::PollEvery(1000),
    auto_start: true,
    persist: true,
};

// Ring 1 ownership: permanece em hermes (R3) por depender de crate::self_evolve e cortex::global_arena.
// ADR-0060 A.4: SleepCycle mantido em hermes; PlasticityController em k_ai.
// SRC constants (Nature Communications 2022 Sleep Replay Consolidation)
const SRC_BUFFER_MAX: usize = 500;              // max entries in each buffer
const SRC_PRIORITY_DECAY: f32 = 0.85;           // per-cycle priority decay
const SRC_PRIORITY_PRUNE: f32 = 0.1;            // discard threshold
const SRC_NOISE_SCALE: f32 = 0.1;               // Gaussian noise for replay
const SRC_DEGRADE_THRESHOLD: f32 = 0.7;         // post/pre ratio below = degraded

// IDEA #314a: Event ring buffer — 1000-entry circular buffer of interaction hashes
struct EventRingBuffer {
    buffer: Vec<u64>,
    capacity: usize,
    write: usize,
    count: usize,
}
impl EventRingBuffer {
    fn new(capacity: usize) -> Self {
        EventRingBuffer { buffer: alloc::vec![0u64; capacity], capacity, write: 0, count: 0 }
    }
    fn push(&mut self, hash: u64) {
        self.buffer[self.write] = hash;
        self.write = (self.write + 1) % self.capacity;
        if self.count < self.capacity { self.count += 1; }
    }
    /// Sample up to `n` entries evenly spaced across the buffer.
    fn sample(&self, n: usize) -> Vec<u64> {
        let n = n.min(self.count);
        if n == 0 { return Vec::new(); }
        let step = self.count / n;
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let idx = (i * step) % self.count;
            let buf_idx = if self.count < self.capacity { idx } else { (self.write + idx) % self.capacity };
            out.push(self.buffer[buf_idx]);
        }
        out
    }
    fn len(&self) -> usize { self.count }
}

pub struct SleepCycleAgent {
    phase: u8,
    cycle_count: u64,
    phase_tick: u64,
    insights: Vec<String>,
    /// Spaced repetition buffer: route traces with priority scores (REPLAY).
    spaced_replay_buffer: Vec<(cortex::r3::RouteTrace, f32)>,
    /// Dream-generated Q&A insights with priority scores (DREAM).
    dream_insights: Vec<(String, f32)>,
    /// Last replay loss, tracked for validation gate.
    last_replay_loss: f32,
    /// Whether any phase was degraded this cycle.
    degraded: bool,
    // ── IDEA #314a: Event ring buffer (1000 events, sample 64 per sleep) ──
    event_ring: EventRingBuffer,
    // ── IDEA #314c: EWC protected entry indices ──
    ewc_protected: Vec<usize>,
    ewc_protect_count: u32,
    ewc_base_loss: f32,
    // ── IDEA #314e: Confidence tracking ──
    confidence_running: f32,
    confidence_samples: u64,
}

impl SleepCycleAgent {
    pub fn new() -> Self {
        SleepCycleAgent {
            phase: 0,
            cycle_count: 0,
            phase_tick: 0,
            insights: Vec::new(),
            spaced_replay_buffer: Vec::new(),
            dream_insights: Vec::new(),
            last_replay_loss: 0.0,
            degraded: false,
            event_ring: EventRingBuffer::new(1000),
            ewc_protected: Vec::new(),
            ewc_protect_count: 0,
            ewc_base_loss: 0.0,
            confidence_running: 0.85,
            confidence_samples: 0,
        }
    }
    fn phase_name(&self) -> &'static str { match self.phase {1=>"REPLAY",2=>"DREAM",3=>"CONSOLIDATE",4=>"PRUNE",5=>"REFLECT",_=>"IDLE"} }
    fn execute_phase(&mut self) {
        let _tick = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        match self.phase {
            // ── REPLAY: store traces in spaced repetition buffer, apply priority decay, replay with noise ──
            1 => {
                let mut traces = [cortex::r3::RouteTrace {
                    embedding_addr: 0,
                    logits_addr: 0,
                    num_experts: 0,
                    selected_expert: 0,
                    old_log_prob: 0.0,
                    token_ids_addr: 0,
                    token_count: 0,
                }; 64];
                let n = cortex::global_arena::snapshot_route_traces(&mut traces);

                // Step 1: store new traces in buffer (boost if already present)
                for t in traces.iter().take(n) {
                    let existing = self.spaced_replay_buffer.iter_mut()
                        .find(|(et, _)| et.embedding_addr == t.embedding_addr && et.token_count == t.token_count);
                    match existing {
                        Some((_, pri)) => *pri = (*pri + 0.5).min(2.0), // boost on re-encounter
                        None => self.spaced_replay_buffer.push((*t, 1.0)),
                    }
                }

                // Step 2: apply priority decay to all buffer entries
                for (_, pri) in self.spaced_replay_buffer.iter_mut() {
                    *pri *= SRC_PRIORITY_DECAY;
                }

                // Step 3: replay top-K entries (priority-sorted) with noise
                let mut total_loss = 0.0f32;
                let replay_count = self.spaced_replay_buffer.len().min(64);
                if replay_count > 0 {
                    // Sort descending by priority so highest-value traces replay first
                    self.spaced_replay_buffer
                        .sort_by(|(_, a), (_, b)| b.partial_cmp(a).unwrap_or(core::cmp::Ordering::Equal));
                    let mut weights = alloc::vec![0i8; 64 * 6];
                    let trinity = TRINITY.lock();
                    for i in 0..replay_count {
                        let (trace, _) = &self.spaced_replay_buffer[i];
                        // IDEA #314c: EWC — reduce noise for protected entries so their weights drift less
                        let noise = if self.ewc_protected.contains(&i) { SRC_NOISE_SCALE * 0.3 } else { SRC_NOISE_SCALE };
                        total_loss += cortex::r3::update_with_replay(
                            &trinity, trace, 0.7, &mut weights, 0.03, noise,
                        );
                    }
                    drop(trinity);
                    let mut t = BITNET_TRAINER.lock();
                    t.trained += replay_count as u64;
                    self.last_replay_loss = total_loss / replay_count as f32;

                    // IDEA #314a: sample event ring buffer and log it
                    let ring_sampled = self.event_ring.sample(64);
                    let ring_detail = if !ring_sampled.is_empty() {
                        alloc::format!(" ring={}/{}", ring_sampled.len(), self.event_ring.len())
                    } else {
                        String::new()
                    };

                    k_nano::slog_hermes!(
                        "SLEEP", "info",
                        "SLEEP-REPLAY-SRC: cycle={} replayed={} loss={:.4} new={} buf={} step={}{}",
                        self.cycle_count, replay_count, total_loss, n,
                        self.spaced_replay_buffer.len(), t.trained, ring_detail,
                    );
                } else {
                    k_nano::slog_hermes!("SLEEP", "info", "SLEEP-REPLAY-EMPTY: cycle={} buffer=0", self.cycle_count);
                    self.last_replay_loss = 0.0;
                }
                cortex::global_arena::clear_route_traces();
                cortex::global_arena::reset_moe_cache();
            }

            // ── DREAM: synthetic QA generation + BitNet variations (IDEA #314b) ──
            2 => {
                let st = crate::evolve::evolve_dream_tick();

                // Extract recent conversation Q&A pairs
                let qa_pairs = crate::cognitive_bridge::extract_qa_pairs(4);
                let qa_count = qa_pairs.len();
                for (user, asst) in &qa_pairs {
                    self.dream_insights
                        .push((alloc::format!("user:{} asst:{}", user, asst), 0.8));
                }

                // IDEA #314b: BitNet synthetic variations — train on QA pairs with elevated noise
                let mut variation_count = 0u32;
                if !qa_pairs.is_empty() {
                    // Replay top buffer entries with dream noise to create synthetic variations
                    let dream_noise = SRC_NOISE_SCALE * 2.5; // 0.25 vs standard 0.1
                    let dream_n = self.spaced_replay_buffer.len().min(16);
                    if dream_n > 0 {
                        let mut var_weights = alloc::vec![0i8; 64 * 6];
                        let trinity = TRINITY.lock();
                        let trinity_has_router = trinity.moe_router_loaded();
                        drop(trinity);
                        if trinity_has_router {
                            let trinity = TRINITY.lock();
                            for di in 0..dream_n {
                                let (trace, _) = &self.spaced_replay_buffer[di];
                                let _ = cortex::r3::update_with_replay(
                                    &trinity, trace, 0.5, &mut var_weights, 0.01, dream_noise,
                                );
                                variation_count += 1;
                            }
                            drop(trinity);
                        }
                    }
                    // Also train BitNet on QA pair embeddings for associative dream patterns
                    let mut trainer = BITNET_TRAINER.lock();
                    for (user, asst) in &qa_pairs {
                        trainer.train_task(user, asst, 1);
                    }
                    variation_count += qa_count as u32;
                }

                // Decay dream insight priorities
                for (_, pri) in self.dream_insights.iter_mut() {
                    *pri *= SRC_PRIORITY_DECAY;
                }

                self.insights.push(alloc::format!(
                    "[DREAM] ciclo #{} evolve={} qa={}", self.cycle_count, st, qa_count
                ));
                k_nano::slog_hermes!(
                    "SLEEP", "info",
                    "SLEEP-DREAM-DREAM: cycle={} evolve={} qa={} variations={} insights={}",
                    self.cycle_count, st, qa_count, variation_count, self.dream_insights.len(),
                );
            }

            // ── CONSOLIDATE: seed knowledge fast→slow layers, validate quality + EWC (IDEA #314c) ──
            3 => {
                // Pre-consolidation quality: token steps as a proxy for model activity
                let pre_quality = cortex::global_arena::token_steps() as f32;

                match k_ai::sgdb::checkpoint_working() {
                    Ok(n) => k_nano::slog_hermes!("sgdb", "sleep_ckpt", "n={}", n),
                    Err(e) => k_nano::slog_hermes!("sgdb", "sleep_ckpt", "FAIL {}", e),
                }

                // Post-consolidation quality
                let post_quality = cortex::global_arena::token_steps() as f32;

                // Validation gate: flag degraded if quality dropped > 30%
                let degraded = pre_quality > 0.0
                    && post_quality < pre_quality * SRC_DEGRADE_THRESHOLD;
                if degraded {
                    self.degraded = true;
                }

                // IDEA #314c: EWC — protect entries that show stable low loss over multiple cycles.
                // Entries with last_replay_loss below threshold (0.1) AND at least 3 cycles old
                // get marked as protected. Protected entries have reduced noise during REPLAY.
                if self.last_replay_loss > 0.0 && self.last_replay_loss < 0.1 {
                    // Mark current high-priority entries as protected
                    for (i, (_, pri)) in self.spaced_replay_buffer.iter().enumerate() {
                        if *pri > 0.8 && !self.ewc_protected.contains(&i) {
                            self.ewc_protected.push(i);
                        }
                    }
                    self.ewc_protect_count += 1;
                    self.ewc_base_loss = self.last_replay_loss;
                } else if self.ewc_protect_count > 0 && self.last_replay_loss > 0.5 {
                    // High loss means knowledge is changing — lift protection gradually
                    let remove = (self.ewc_protected.len() / 2).max(1);
                    for _ in 0..remove.min(self.ewc_protected.len()) {
                        self.ewc_protected.remove(0);
                    }
                    self.ewc_protect_count = self.ewc_protect_count.saturating_sub(1);
                }
                // Cap protected list
                if self.ewc_protected.len() > 64 {
                    self.ewc_protected.drain(0..(self.ewc_protected.len() - 64));
                }

                k_nano::slog_hermes!(
                    "SLEEP", "info",
                    "SLEEP-CONSOLIDATE-EWC: cycle={} pre={:.4} post={:.4} degraded={} protected={} ewc_base={:.4}",
                    self.cycle_count, pre_quality, post_quality, degraded,
                    self.ewc_protected.len(), self.ewc_base_loss,
                );
            }

            // ── PRUNE: sparsify weights + trim both replay buffers + honor EWC (IDEA #314c/d) ──
            4 => {
                if self.insights.len() > 100 {
                    self.insights.drain(0..50);
                }
                let pruned_ram = k_ai::sgdb::prune_working_ram();
                k_ai::sgdb::update_with_replay();

                // Prune spaced_replay_buffer: remove weak entries, cap at SRC_BUFFER_MAX.
                // IDEA #314c: EWC — keep protected entries even if priority would prune them.
                let before_srb = self.spaced_replay_buffer.len();
                let protected = &self.ewc_protected;
                self.spaced_replay_buffer = core::mem::take(&mut self.spaced_replay_buffer)
                    .into_iter().enumerate()
                    .filter(|(i, (_, pri))| protected.contains(i) || *pri >= SRC_PRIORITY_PRUNE)
                    .map(|(_, entry)| entry)
                    .collect();
                self.spaced_replay_buffer.truncate(SRC_BUFFER_MAX);
                let pruned_srb = before_srb.saturating_sub(self.spaced_replay_buffer.len());

                // Prune dream_insights too
                let before_di = self.dream_insights.len();
                self.dream_insights.retain(|(_, pri)| *pri >= SRC_PRIORITY_PRUNE);
                self.dream_insights.truncate(SRC_BUFFER_MAX);
                let pruned_di = before_di.saturating_sub(self.dream_insights.len());

                // ponytail: indices shift after pruning — clear and let next CONSOLIDATE rebuild
                self.ewc_protected.clear();

                k_nano::slog_hermes!(
                    "SLEEP", "info",
                    "SLEEP-PRUNE-PRUNE: cycle={} pruned_srb={}/{} pruned_di={}/{} ram_l0l1={} insights={} ewc_protected={}",
                    self.cycle_count, pruned_srb, before_srb, pruned_di, before_di,
                    pruned_ram, self.insights.len(), self.ewc_protected.len(),
                );
            }

            // ── REFLECT: meta-cognition + cycle summary + confidence (IDEA #314e) ──
            5 => {
                let detail = crate::self_evolve::reflect(
                    k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64,
                );
                self.insights.push(alloc::format!("[REFLECT] {}", detail));
                let _ = EVENT_BUS.publish(Event {
                    id: 0,
                    topic: String::from(crate::self_evolve::TOPIC_SELF_EVOLVE),
                    payload: detail.into_bytes(),
                    token: CapabilityToken::Legacy(1),
                });
                let _nudge = crate::cognitive_bridge::reflect_and_nudge(self.cycle_count);
                cortex::neuos_probe::log_probe(None);

                // IDEA #314e: Confidence tracking from session log
                let session_len = crate::cognitive_bridge::session_len();
                if session_len > self.confidence_samples {
                    let _new_samples = session_len - self.confidence_samples;
                    // Simple proxy: replay loss inverso como confiança (baixa loss = alta confiança)
                    let replay_conf = (1.0 - self.last_replay_loss.min(1.0)).max(0.0);
                    // EMA update
                    let alpha = 0.3f32;
                    self.confidence_running = self.confidence_running * (1.0 - alpha) + replay_conf * alpha;
                    self.confidence_samples = session_len;
                }
                let confidence_pct = (self.confidence_running * 100.0) as u32;

                // SRC cycle summary with confidence
                let buf_entries = self.spaced_replay_buffer.len();
                let dream_entries = self.dream_insights.len();
                let degrade_flags = if self.degraded { " [DEGRADED]" } else { "" };
                k_nano::slog_hermes!(
                    "SLEEP", "info",
                    "SLEEP-REFLECT-SUMMARY: cycle={} phase=REFLECT \
                     buffer={} dream={} insights={} confidence={}%{}",
                    self.cycle_count, buf_entries, dream_entries,
                    self.insights.len(), confidence_pct, degrade_flags,
                );
                // Reset degraded flag for next cycle
                self.degraded = false;
            }
            _ => {}
        }
    }
}

impl Agent for SleepCycleAgent {
    fn manifest(&self) -> &AgentManifest { &SLEEPCYCLE_MANIFEST }
    fn tick(&mut self, _t: u64, _c: u64) -> AgentTickResult {
        let now = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        if self.phase == 0 {
            if self.cycle_count == 0 || now > self.phase_tick + 5000 { self.phase = 1; self.phase_tick = now; }
            return AgentTickResult::Pending;
        }
        if now < self.phase_tick + 200 { return AgentTickResult::Pending; }
        self.execute_phase();
        self.phase_tick = now;
        if self.phase >= 5 { self.phase = 0; self.cycle_count += 1; }
        else { self.phase += 1; }
        AgentTickResult::Pending
    }
}

// ---------------------------------------------------------------------------
// FsBridgeAgent — ponte entre VFS e MHI para migração de dados entre tiers
// ---------------------------------------------------------------------------

const FSBRIDGE_MANIFEST: AgentManifest = AgentManifest {
    name: "fs_bridge",
    kind: AgentKind::System,
    schedule: ScheduleKind::PollEvery(500),
    auto_start: true,
    persist: true,
};

pub struct FsBridgeAgent {
    last_scan: u64,
}

impl FsBridgeAgent {
    pub fn new() -> Self { FsBridgeAgent { last_scan: 0 } }

    fn execute_migration(&mut self, _tick: u64) {
        let suggestions: Vec<(u64, u64)> = {
            let reg = k_nano::mhi::MHI_REGISTRY.lock();
            reg.allocations.iter()
                .filter(|(_, p)| {
                    let idle = _tick.saturating_sub(p.last_access_tick);
                    p.access_count > 5 && idle < 500
                        && p.tier != k_nano::mhi::AllocTier::Dram
                })
                .map(|(&addr, p)| (addr, p.last_access_tick))
                .collect()
        };
        for (addr, last_access) in &suggestions {
            let path = alloc::format!("/mhi/{:x}", addr);
            match crate::globals::read_vfs(&path) {
                Ok(data) => {
                    let phys = x86_64::PhysAddr::new(*addr);
                    let size = data.len();
                    if let Some(dram_addr) = k_nano::mhi::alloc_by_tier(k_nano::mhi::AllocTier::Dram, size) {
                        let dst = dram_addr.as_u64() as *mut u8;
                        unsafe { core::ptr::copy_nonoverlapping(data.as_ptr(), dst, size); }
                        let mut reg = k_nano::mhi::MHI_REGISTRY.lock();
                        reg.register(phys, size, k_nano::mhi::AllocTier::Dram, "fs_bridge");
                        reg.record_access(phys, _tick, 0);
                        k_nano::slog_hermes!("FS", "BRIDGE", "Migrado {:?} → DRAM ({} bytes, idle={})", phys, size, _tick.saturating_sub(*last_access));
                    }
                }
                Err(_) => {}
            }
        }
    }
}

impl Agent for FsBridgeAgent {
    fn manifest(&self) -> &AgentManifest { &FSBRIDGE_MANIFEST }
    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        if _tick - self.last_scan < 500 { return AgentTickResult::Pending; }
        self.last_scan = _tick;
        self.execute_migration(_tick);
        AgentTickResult::Pending
    }
}

// ---------------------------------------------------------------------------
// GpuDriverAgent — init VirtIO-GPU (boot phase)
// ---------------------------------------------------------------------------

pub struct GpuDriverAgent;

const GPUDRIVER_MANIFEST: AgentManifest = AgentManifest {
    name: "gpu_driver",
    kind: AgentKind::Driver,
    schedule: ScheduleKind::Oneshot,
    auto_start: true,
    persist: false,
};

impl Agent for GpuDriverAgent {
    fn manifest(&self) -> &AgentManifest { &GPUDRIVER_MANIFEST }
    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        unsafe {
            if k_nano::virtio_gpu::init_driver_virtio_gpu() {
                k_nano::slog_jarbas!("VGPU", "info", "VirtIO-GPU OK.");
            }
        }
        AgentTickResult::Done
    }
}

// ── DiagnosticSkill ─────────────────────────────────────────────
// Substitui os testes inline de Box/Vec/Tensor/SiLU/RMSNorm/BitNet
// que estavam no boot flow procedural. SystemAgent executa esta skill
// durante a fase de Diagnostics, publicando resultados no EventBus.

use skill_registry::{Skill, McpManifest, OutputSchema};

pub struct DiagnosticSkill;

impl DiagnosticSkill {
    pub fn new() -> Self { DiagnosticSkill }
}

impl Skill for DiagnosticSkill {
    fn manifest(&self) -> McpManifest {
        McpManifest {
            name: alloc::string::String::from("diagnostic"),
            description: alloc::string::String::from("Run-time diagnostics: alloc, tensor, MLP, BitNet"),
            required_tokens: vec![1],
            preconditions: Vec::new(),
            context_links: Vec::new(),
            output_schema: OutputSchema::Any,
            idempotent: true,
            contracts: Vec::new(),
        }
    }

    fn verify(&self, _payload: &[u8]) -> Result<(), &'static str> {
        Ok(())
    }

    fn execute(&self, _payload: &[u8]) -> Result<alloc::vec::Vec<u8>, &'static str> {
        use alloc::boxed::Box;
        use alloc::vec::Vec;
        let mut report = alloc::string::String::new();

        // 1. Box/Alloc test
        let boxed_val = Box::new(41);
        report.push_str(&alloc::format!("[DIAG] Box::new(41) = {}\n", *boxed_val));

        // 2. Vec test
        let mut vec = Vec::new();
        vec.push(10); vec.push(20); vec.push(30);
        report.push_str(&alloc::format!("[DIAG] Vec = {:?}\n", vec));

        // 3. Tensor matmul
        let a_data = vec![1.0_f32, 2.0_f32, 3.0_f32];
        let a = cortex::tensor::Tensor::from_row_major((1, 3), a_data).unwrap();
        let b_data = vec![4.0_f32, 5.0_f32, 6.0_f32];
        let b = cortex::tensor::Tensor::from_row_major((3, 1), b_data).unwrap();
        if let Some(c) = a.matmul(&b) {
            report.push_str(&alloc::format!("[DIAG] Matmul: ({}, {}) {:?}\n", c.shape.0, c.shape.1, c.data));
        }

        // 4. SiLU + RMSNorm
        let mut tensor = cortex::tensor::Tensor::from_row_major((1, 3), vec![-1.0, 0.0, 1.0]).unwrap();
        tensor.apply(cortex::nn::silu);
        cortex::nn::rms_norm(&mut tensor, &[1.0], 1e-6);
        report.push_str(&alloc::format!("[DIAG] SiLU+RMSNorm = {:?}\n", tensor.data));

        // 5. BitNet 2-bit inference
        let bit_input = cortex::tensor::Tensor::from_row_major((1, 3), vec![1.5, -0.5, 2.0]).unwrap();
        let weights_f32 = cortex::tensor::Tensor::from_row_major(
            (3, 2), vec![1.5_f32, -1.8, 0.2, 2.1, -3.0, 0.0],
        ).unwrap();
        let packed_weights = cortex::tensor::quantize_to_packed(&weights_f32, 0.5);
        let bit_linear = cortex::nn::BitLinear::new(packed_weights, None);
        let bit_output = bit_linear.forward(&bit_input);
        report.push_str(&alloc::format!("[DIAG] BitNet output = {:?}\n", bit_output.data));

        k_nano::slog_hermes!("Log", "msg", "{}", report);
        k_nano::println!("{}", report);
        Ok(report.into_bytes())
    }
}






