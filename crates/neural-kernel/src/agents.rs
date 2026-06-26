//! Native Agent implementations — Block 11 (Sprints 39-42)
//! Cada struct implementa agent_core::Agent. Substituem as 7 async fn legacy.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::pin::Pin;
use core::task::{Context, Poll};
use event_bus::{CapabilityToken, Event, EventBus, Receiver};
use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use crate::cortex;
use crate::hermes;
use crate::conversation;
use crate::{serial_println, println};
use crate::{EVENT_BUS, SKILL_REGISTRY, SKILL_STORAGE, TRUST_CACHE, USAGE_TRACKER, EVENT_LOG,
            CONVERSATION_TRACKER, PENDING_SKILL};

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
        let event = Event { id: 0, topic: String::from("SYSTEM_READY"), payload: vec![1, 2, 3], token: CapabilityToken(1) };
        match EVENT_BUS.publish(event) {
            Ok(()) => { serial_println!("[MONITOR] Evento SYSTEM_READY publicado."); }
            Err(e) => { serial_println!("[MONITOR] Falha: {}", e); }
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
        let scancode = crate::interrupts::LAST_SCANCODE.swap(0, core::sync::atomic::Ordering::Acquire);
        if scancode != 0 {
            let _ = EVENT_BUS.publish(Event {
                id: 0, topic: String::from("RAW_HW_IRQ1"),
                payload: vec![scancode],
                token: CapabilityToken(1),
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
            serial_println!("[Hermes] {}", text);
            println!("[Hermes] {}", text);
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
}

impl InputAgent {
    pub fn new() -> Self {
        InputAgent { receiver: EVENT_BUS.subscribe("RAW_HW_IRQ1"), buffer: String::new() }
    }
}

impl Agent for InputAgent {
    fn manifest(&self) -> &AgentManifest { &INPUT_MANIFEST }
    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        if let Some(event) = self.receiver.try_receive() {
            let scancode = event.payload.first().copied().unwrap_or(0);
            if scancode < 0x80 {
                match scancode {
                    0x1C => {
                        let text = core::mem::take(&mut self.buffer);
                        if !text.is_empty() {
                            serial_println!("[INPUT] ENTER — USER_INTENT: \"{}\"", text);
                            println!("[INPUT] ENTER — USER_INTENT: \"{}\"", text);
                            let _ = EVENT_BUS.publish(Event {
                                id: 0, topic: String::from("USER_INTENT"),
                                payload: text.into_bytes(), token: CapabilityToken(1),
                            });
                        }
                    }
                    0x0E => { self.buffer.pop(); }
                    _ => {
                        if let Some(ch) = crate::scancode_to_ascii(scancode) {
                            self.buffer.push(ch);
                        }
                    }
                }
            }
        }
        AgentTickResult::Pending
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
    model: crate::cortex::TransformerModel,
    receiver: Receiver,
}

impl CortexAgent {
    pub fn new() -> Self {
        let model_data = include_bytes!("../micro.bitnet");
        let model = crate::cortex::load_model(model_data).unwrap_or_else(|| {
            serial_println!("[CORTEX-LLM] Falha ao carregar modelo treinado. Usando random.");
            crate::cortex::TransformerModel::new()
        });
        serial_println!("[CORTEX-LLM] Transformer loaded. Skills via SKILL_STORAGE.");
        CortexAgent { model, receiver: EVENT_BUS.subscribe(cortex::TOPIC_LLM_REQUEST) }
    }
}

impl Agent for CortexAgent {
    fn manifest(&self) -> &AgentManifest { &CORTEX_MANIFEST }
    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        if let Some(event) = self.receiver.try_receive() {
            let user_text = core::str::from_utf8(&event.payload).unwrap_or("");
            serial_println!("[CORTEX-LLM] Generating for: \"{}\"", user_text);
            let system_prompt = SKILL_STORAGE.lock().build_system_prompt();
            let full_prompt = alloc::format!("{}. PERGUNTA: {}", system_prompt, user_text);
            let output = crate::cortex::generate_text(&self.model, &full_prompt);
            serial_println!("[CORTEX-LLM] Generated: \"{}\"", output);
            let _ = EVENT_BUS.publish(Event {
                id: 0, topic: alloc::string::String::from(cortex::TOPIC_LLM_RESPONSE),
                payload: output.into_bytes(), token: CapabilityToken(1),
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
    cortex: crate::cortex::Cortex,
    state: HermesState,
    status_skill: String,
    echo_skill: String,
    hw_skill: String,
    net_diag_skill: String,
}

impl HermesAgent {
    pub fn new() -> Self {
        HermesAgent {
            user_receiver: EVENT_BUS.subscribe(hermes::TOPIC_USER_INTENT),
            llm_receiver: EVENT_BUS.subscribe(cortex::TOPIC_LLM_RESPONSE),
            cortex: crate::cortex::Cortex::new(),
            state: HermesState::Idle,
            status_skill: String::from("system_status"),
            echo_skill: String::from("echo"),
            hw_skill: String::from("hardware_info"),
            net_diag_skill: String::from("net_diag"),
        }
    }

    fn execute_skill(&self, name: &str, payload: &[u8], token: &CapabilityToken) -> Result<Vec<u8>, &'static str> {
        let token_val = token.0;
        let now = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        {
            let mut tc = TRUST_CACHE.lock();
            if !tc.is_trusted(token_val, name, now) {
                let reg = SKILL_REGISTRY.lock();
                if !reg.validate_token(name, token) {
                    return Err("token nao autorizado");
                }
                tc.check_or_cache(token_val, name, now, 360);
            }
        }
        let reg = SKILL_REGISTRY.lock();
        reg.execute_skill_unchecked(name, payload)
    }
}

impl Agent for HermesAgent {
    fn manifest(&self) -> &AgentManifest { &HERMES_MANIFEST }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        // Check LLM response first (if awaiting)
        let mut responded = String::new();
        let mut awaiting = matches!(self.state, HermesState::AwaitingLLM);

        if awaiting {
            if let Some(event) = self.llm_receiver.try_receive() {
                awaiting = false;
                self.state = HermesState::Idle;
                let text = core::str::from_utf8(&event.payload).unwrap_or("");
                serial_println!("[CORTEX-LLM] Resposta: \"{}\"", text);
                let now = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;

                // Check if this is a skill creation response
                let pending = PENDING_SKILL.lock().take();
                if let Some((name, desc)) = pending {
                    let mut storage = SKILL_STORAGE.lock();
                    match storage.register_skill(text) {
                        Ok(()) => {
                            let msg = alloc::format!("[Hermes] Skill '{}' criada via LLM! Instrucoes: {}", name, desc);
                            serial_println!("[SKILL-LLM] Skill '{}' gerada ({} bytes)", name, text.len());
                            responded = msg;
                        }
                        Err(e) => {
                            let msg = alloc::format!("[Hermes] Erro ao criar skill '{}': {}", name, e);
                            responded = msg;
                        }
                    }
                } else {
                    // Normal chat response
                    EVENT_LOG.lock().push(conversation::EventKind::HermesResponse, event.payload.clone(), now);
                    CONVERSATION_TRACKER.lock().record_exchange("(LLM)", text);
                    responded = alloc::format!("[Hermes] {}", text);
                }
            }
        }

        // Handle user input (if not awaiting LLM or responded)
        if let Some(event) = self.user_receiver.try_receive() {
            let text = core::str::from_utf8(&event.payload).unwrap_or("");
            serial_println!("[CORTEX] Texto: \"{}\"", text);
            println!("[CORTEX] Texto: \"{}\"", text);

            let cmd = hermes::parse_command(text);
            let response = match cmd {
                hermes::Command::Status => {
                    match self.execute_skill(&self.status_skill, &event.payload, &event.token) {
                        Ok(_) => String::from("System status report executado."),
                        Err(e) => alloc::format!("Erro: {}", e),
                    }
                }
                hermes::Command::Echo(ref arg) => {
                    match self.execute_skill(&self.echo_skill, arg.as_bytes(), &event.token) {
                        Ok(output) => {
                            let rev = core::str::from_utf8(&output).unwrap_or("(bytes)");
                            alloc::format!("Echo reverso: \"{}\"", rev)
                        }
                        Err(e) => alloc::format!("Erro: {}", e),
                    }
                }
                hermes::Command::HardwareInfo => {
                    match self.execute_skill(&self.hw_skill, &event.payload, &event.token) {
                        Ok(output) => String::from(core::str::from_utf8(&output).unwrap_or("(binary)")),
                        Err(e) => alloc::format!("Erro: {}", e),
                    }
                }
                hermes::Command::NetDiag => {
                    match self.execute_skill(&self.net_diag_skill, &event.payload, &event.token) {
                        Ok(output) => String::from(core::str::from_utf8(&output).unwrap_or("(binary)")),
                        Err(e) => alloc::format!("Erro: {}", e),
                    }
                }
                hermes::Command::Fetch(ref url) => {
                    let parsed: Option<([u8; 4], u16, String)> = {
                        let url_str = url.trim();
                        if let Some(rest) = url_str.strip_prefix("http://") {
                            let without_slash = if let Some(pos) = rest.find('/') {
                                let (hp, p) = rest.split_at(pos);
                                (hp, alloc::string::ToString::to_string(p))
                            } else { (rest, String::from("/")) };
                            let (host_str, path) = without_slash;
                            let (host_only, port) = if let Some(pos) = host_str.find(':') {
                                let (h, p_str) = host_str.split_at(pos);
                                let p: u16 = p_str[1..].parse().unwrap_or(80);
                                (h, p)
                            } else { (host_str, 80u16) };
                            let parts: Vec<&str> = host_only.split('.').collect();
                            if parts.len() == 4 {
                                Some(([parts[0].parse().unwrap_or(0), parts[1].parse().unwrap_or(0),
                                       parts[2].parse().unwrap_or(0), parts[3].parse().unwrap_or(0)], port, path))
                            } else { None }
                        } else { None }
                    };
                    match parsed {
                        Some((ip, port, path)) => {
                            match unsafe { crate::net::http_get(ip, port, &path) } {
                                Some(body) => {
                                    let text = core::str::from_utf8(&body).unwrap_or("(binary)");
                                    let preview = if text.len() > 200 { &text[..200] } else { text };
                                    alloc::format!("Fetch OK ({} bytes):\n{}", body.len(), preview)
                                }
                                None => String::from("Fetch falhou: sem resposta"),
                            }
                        }
                        None => String::from("Formato: /fetch http://ip:port/path"),
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
                    let now = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
                    TRUST_CACHE.lock().trust_allow(token, skill, now);
                    alloc::format!("Trust permitido: token {} -> skill '{}'", token, skill)
                }
                hermes::Command::TrustDeny(token, ref skill) => {
                    TRUST_CACHE.lock().trust_deny(token, skill);
                    alloc::format!("Trust revogado: token {} -> skill '{}'", token, skill)
                }
                hermes::Command::Help => {
                    String::from("Comandos: /status, /echo <txt>, /hw, /netdiag, /usage, /conv, /ping <ip>, /fetch <url>, /trust allow <token> <skill>, /trust deny <token> <skill>, /show_skills, /add_skill <nome> <desc>, /rm_skill <name>, /reload_skills, /help")
                }
                hermes::Command::ShowSkills => {
                    let storage = SKILL_STORAGE.lock();
                    let list = storage.list_skills();
                    if list.is_empty() { String::from("Nenhuma skill carregada.") }
                    else {
                        let mut msg = alloc::format!("Skills ({}) carregadas:\n", list.len());
                        for (i, (n, d, b)) in list.iter().enumerate() {
                            msg.push_str(&alloc::format!("{}. {} - {} ({} bytes)\n", i+1, n, d, b));
                        }
                        msg
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
                        id: 0, topic: String::from(cortex::TOPIC_LLM_REQUEST),
                        payload: prompt.into_bytes(), token: CapabilityToken(1),
                    });
                    self.state = HermesState::AwaitingLLM;
                    String::from("...")
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
                hermes::Command::Chat(ref msg) => {
                    let intent = self.cortex.think(msg);
                    let intent_name = intent.skill_name();
                    serial_println!("[CORTEX] Intent: {} = {:?}", intent_name, intent);
                    match intent {
                        cortex::Intent::Greeting | cortex::Intent::Chat => {
                            serial_println!("[CORTEX-LLM] Enviando: \"{}\"", msg);
                            let _ = EVENT_BUS.publish(Event {
                                id: 0, topic: String::from(cortex::TOPIC_LLM_REQUEST),
                                payload: msg.as_bytes().to_vec(), token: CapabilityToken(1),
                            });
                            self.state = HermesState::AwaitingLLM;
                            String::from("...")
                        }
                        _ => {
                            match SKILL_REGISTRY.lock().has_skill(intent_name) {
                                true => {
                                    match self.execute_skill(intent_name, msg.as_bytes(), &event.token) {
                                        Ok(output) => {
                                            let text = core::str::from_utf8(&output).unwrap_or("(binary)");
                                            alloc::format!("[Cortex] {}: {}", intent_name, text)
                                        }
                                        Err(e) => alloc::format!("[Cortex] {} erro: {}", intent_name, e),
                                    }
                                }
                                false => alloc::format!("Hermes: sem skill para '{}'. /help", intent_name)
                            }
                        }
                    }
                }
            };

            let now = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;

            // If we already have a response (e.g., LLM skill creation), use responded
            let final_response = if !responded.is_empty() {
                &responded
            } else {
                &response
            };

            if !matches!(self.state, HermesState::AwaitingLLM) {
                USAGE_TRACKER.lock().record_call("intent_router", 1);
                EVENT_LOG.lock().push(conversation::EventKind::UserInput, event.payload.clone(), now);
                EVENT_LOG.lock().push(conversation::EventKind::HermesResponse, response.as_bytes().to_vec(), now);
                CONVERSATION_TRACKER.lock().record_exchange(text, &response);
                if CONVERSATION_TRACKER.lock().needs_compact() {
                    let compact_msg = CONVERSATION_TRACKER.lock().compact();
                    serial_println!("[HERMES] {}", compact_msg);
                    EVENT_LOG.lock().push(conversation::EventKind::ContextCompacted, compact_msg.into_bytes(), now);
                }
                let _ = EVENT_BUS.publish(Event {
                    id: 0, topic: String::from(hermes::TOPIC_HERMES_RESPONSE),
                    payload: response.into_bytes(), token: CapabilityToken(1),
                });
            } else {
                EVENT_LOG.lock().push(conversation::EventKind::UserInput, event.payload.clone(), now);
            }
        }

        AgentTickResult::Pending
    }
}

// network_agent_tick is called directly via crate::network_agent::network_agent_tick()
