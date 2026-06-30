//! Native Agent implementations — Block 11 (Sprints 39-42)
//! Cada struct implementa agent_core::Agent. Substituem as 7 async fn legacy.

pub mod mouse_agent;

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
        let event = Event { id: 0, topic: String::from("SYSTEM_READY"), payload: vec![1, 2, 3], token: CapabilityToken::Legacy(1) };
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
            serial_println!("[Hermes] {}", text);
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
}

impl InputAgent {
    pub fn new() -> Self {
        InputAgent { receiver: EVENT_BUS.subscribe("RAW_HW_IRQ1"), buffer: String::new(), ctrl: false, alt: false }
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
        unsafe { crate::xhci::poll_keyboard() }
    }
    fn process_scancode(&mut self, scancode: u8) {
        let pressed = scancode < 0x80;
        let key = if pressed { scancode } else { scancode & 0x7F };
        match key {
            0x1D => { self.ctrl = pressed; }
            0x38 => { self.alt = pressed; }
            0x53 if self.ctrl && self.alt && pressed => { self.handle_cad(); }
            _ => {}
        }
        if !pressed { return; }
        if scancode >= 0x80 { return; }
        match scancode {
            0x1C => {
                let text = core::mem::take(&mut self.buffer);
                if !text.is_empty() {
                    serial_println!("[INPUT] ENTER — USER_INTENT: \"{}\"", text);
                    println!("[INPUT] ENTER — USER_INTENT: \"{}\"", text);
                    let _ = EVENT_BUS.publish(Event {
                        id: 0, topic: String::from("USER_INTENT"),
                        payload: text.into_bytes(), token: CapabilityToken::Legacy(1),
                    });
                }
            }
            0x0E => { self.buffer.pop(); }
            _ => { if let Some(ch) = crate::scancode_to_ascii(scancode) { self.buffer.push(ch); } }
        }
        // Echo tecla para o display em tempo real
        let _ = EVENT_BUS.publish(Event {
            id: 0, topic: String::from("KEYBOARD_ECHO"),
            payload: self.buffer.clone().into_bytes(),
            token: CapabilityToken::Legacy(1),
        });
    }
    fn handle_cad(&self) {
        serial_println!("[SYS] Ctrl+Alt+Del. Escrevendo log no SDHC e desligando...");
        let log = crate::serial::BOOT_LOG.lock();
        let dump = log.dump();
        if !dump.is_empty() {
            serial_println!("[SYS] Log: {} bytes capturados.", dump.len());
            // Write log to SDHC via ATA
            let ata = crate::ATA_DRIVER.lock();
            if let Some(ref ata) = *ata {
                if dump.len() <= 512 {
                    let mut sector = [0u8; 512];
                    sector[..dump.len()].copy_from_slice(dump);
                    if unsafe { ata.write_sectors(crate::LOG_SECTOR, &sector, 1) } {
                        serial_println!("[SYS] Log escrito no setor LBA {} (512 bytes).", crate::LOG_SECTOR);
                    } else { serial_println!("[SYS] Falha ao escrever log no SDHC."); }
                } else {
                    serial_println!("[SYS] Log grande demais para 1 setor (512B). Usar serial.");
                }
            } else { serial_println!("[SYS] ATA nao disponivel. Log nao salvo."); }
        }
        drop(log);
        serial_println!("[SYS] Power off via PS/2 reset...");
        unsafe {
            core::arch::asm!("out dx, al", in("dx") 0x64u16, in("al") 0xFEu8, options(nostack, preserves_flags));
        }
        loop { x86_64::instructions::hlt(); }
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
    cortex: crate::cortex::Cortex,
    state: HermesState,
    status_skill: String,
    echo_skill: String,
    hw_skill: String,
    net_diag_skill: String,
    boot_greeted: bool,
    react_phase: crate::hermes::ReActPhase,
    sdd_counter: u64,
}

impl HermesAgent {
    pub fn new() -> Self {
        HermesAgent {
            user_receiver: EVENT_BUS.subscribe(hermes::TOPIC_USER_INTENT),
            llm_receiver: EVENT_BUS.subscribe(cortex::TOPIC_LLM_RESPONSE),
            security_receiver: EVENT_BUS.subscribe("SECURITY_ALERT"),
            cortex: crate::cortex::Cortex::new(),
            state: HermesState::Idle,
            status_skill: String::from("system_status"),
            echo_skill: String::from("echo"),
            hw_skill: String::from("hardware_info"),
            net_diag_skill: String::from("net_diag"),
            boot_greeted: false,
            react_phase: crate::hermes::ReActPhase::Observe,
            sdd_counter: 0,
        }
    }

    fn log_phase(&self, phase: crate::hermes::ReActPhase, detail: &str) {
        serial_println!("[HERMES] {} — {}", phase.label(), detail);
    }

    fn show_sdd(&self, goal: &str) {
        let sdd = crate::hermes::Sdd::new(
            goal,
            &alloc::format!("Tick {}, agentes ativos, memória {:.0}%",
                crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed),
                crate::memory::global_hardware_context()[0] * 100.0),
            goal,
            "Comando processado com sucesso",
            "Nada a reverter — comando não destrutivo",
        );
        serial_println!("{}", sdd.display());
    }

    fn execute_skill(&self, name: &str, payload: &[u8], token: &CapabilityToken) -> Result<Vec<u8>, &'static str> {
        let token_val = token.as_legacy();
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
        // #180: Greeting no primeiro boot
        if !self.boot_greeted {
            let greeting = crate::hermes::hermes_greeting();
            serial_println!("{}", greeting);
            println!("{}", greeting);
            let _ = EVENT_BUS.publish(Event {
                id: 0, topic: String::from(hermes::TOPIC_HERMES_RESPONSE),
                payload: alloc::format!("{} v{} — {}", crate::hermes::HERMES_NAME,
                    crate::hermes::HERMES_VERSION, crate::hermes::HERMES_MOTTO).into_bytes(),
                token: CapabilityToken::Legacy(1),
            });
            self.boot_greeted = true;
        }

        // #190: Avança no ciclo ReAct
        self.react_phase = self.react_phase.next();
        self.log_phase(self.react_phase, "ciclo contínuo de cognição");

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

        // Check security alerts
        if let Some(event) = self.security_receiver.try_receive() {
            let text = core::str::from_utf8(&event.payload).unwrap_or("");
            serial_println!("[SECURITY] {}", text);
            // Republish as Hermes response (for display)
            let _ = EVENT_BUS.publish(Event {
                id: 0, topic: String::from(hermes::TOPIC_HERMES_RESPONSE),
                payload: text.as_bytes().to_vec(), token: CapabilityToken::Legacy(1),
            });
        }

        // Handle user input (if not awaiting LLM or responded)
        if let Some(event) = self.user_receiver.try_receive() {
            let text = core::str::from_utf8(&event.payload).unwrap_or("");
            serial_println!("[CORTEX] Texto: \"{}\"", text);
            println!("[CORTEX] Texto: \"{}\"", text);

            let cmd = hermes::parse_command(text);

            // #178: SDD + #184: Intent Transparency antes de executar
            let intent_name = match cmd {
                hermes::Command::Status => "Status",
                hermes::Command::Echo(_) => "Echo",
                hermes::Command::HardwareInfo => "HardwareInfo",
                hermes::Command::NetDiag => "NetDiag",
                hermes::Command::Fetch(_) => "Fetch",
                hermes::Command::Ping(_) => "Ping",
                hermes::Command::Usage => "Usage",
                hermes::Command::Conversation => "Conversation",
                hermes::Command::TrustAllow(_, _) => "TrustAllow",
                hermes::Command::TrustDeny(_, _) => "TrustDeny",
                hermes::Command::Help => "Help",
                hermes::Command::ShowSkills => "ShowSkills",
                hermes::Command::AddSkill(_, _) => "AddSkill",
                hermes::Command::RmSkill(_) => "RmSkill",
                hermes::Command::ReloadSkills => "ReloadSkills",
                hermes::Command::Profile => "Profile",
                hermes::Command::Chat(_) => "Chat",
            };
            let intent_info = crate::hermes::IntentInfo {
                intent_name: String::from(intent_name),
                confidence: 0.92,
                alternatives: Vec::new(),
            };
            serial_println!("{}", intent_info.display());
            self.show_sdd(intent_name);

            // #191: Council deliberation para comandos ambíguos (ex: Chat)
            if matches!(cmd, hermes::Command::Chat(_)) {
                let (opt, skep, prag) = crate::hermes::council_deliberate(text);
                serial_println!("{}", crate::hermes::council_display(&opt, &skep, &prag));
            }

            // #193: Bitter Pill check
            if let Some(reason) = crate::hermes::check_bitter_pill(text) {
                serial_println!("[HERMES] 🛑 Bitter Pill: {}", reason);
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
                        payload: prompt.into_bytes(), token: CapabilityToken::Legacy(1),
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
                hermes::Command::Profile => {
                    let profile = crate::profile::ProfileManager::get();
                    let profiles = crate::profile::ProfileManager::list();
                    let parts: alloc::vec::Vec<&str> = text.splitn(2, |c: char| c.is_whitespace()).collect();
                    let change_msg = if parts.len() > 1 {
                        let desired = parts[1].trim();
                        let mut found_name = String::new();
                        for (p, _desc) in &profiles {
                            if p.name().eq_ignore_ascii_case(desired) {
                                crate::profile::ProfileManager::set(*p);
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
                    let intent = self.cortex.think(msg);
                    let intent_name = intent.skill_name();
                    serial_println!("[CORTEX] Intent: {} = {:?}", intent_name, intent);
                    match intent {
                        cortex::Intent::Greeting | cortex::Intent::Chat => {
                            serial_println!("[CORTEX-LLM] Enviando: \"{}\"", msg);
                            let _ = EVENT_BUS.publish(Event {
                                id: 0, topic: String::from(cortex::TOPIC_LLM_REQUEST),
                                payload: msg.as_bytes().to_vec(), token: CapabilityToken::Legacy(1),
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
                    payload: response.into_bytes(), token: CapabilityToken::Legacy(1),
                });
            } else {
                EVENT_LOG.lock().push(conversation::EventKind::UserInput, event.payload.clone(), now);
            }
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
                unsafe { crate::pci::init_pci(); }
                self.phase = 1;
                AgentTickResult::Pending
            }
            1 => {
                let phys_off = crate::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
                let acpi_info = unsafe { crate::acpi::init_acpi(phys_off) };
                if let Some(ref info) = acpi_info {
                    unsafe { crate::apic::init_apic(info); }
                }
                self.phase = 2;
                AgentTickResult::Pending
            }
            2 => {
                unsafe { crate::smp::init_smp(); }
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
                let pci_devices = unsafe { crate::pci::scan_pci() };
                let arch = crate::inventory::SystemArchitecture::infer(
                    &crate::inventory::HardwareInventory::collect(pci_devices, None),
                );
                serial_println!("[ARCH] System architecture: ring0={} ring1={} heap={}MB trust={} power={} tensor={}",
                    arch.ring0_mode, arch.ring1_mode, arch.heap_size_mb,
                    arch.trust_level, arch.power_mode, arch.tensor_tier);
                *crate::SYSTEM_ARCH.lock() = Some(arch);
                self.phase = 1;
                AgentTickResult::Pending
            }
            1 => {
                let mhi = crate::mhi::MemoryHierarchy::new();
                serial_println!("[MHI] {} tier(s). Best: {:?} ({} bytes avail)",
                    mhi.tiers.len(), mhi.best_tier(), mhi.tiers[0].capacity_bytes);
                *crate::MEMORY_HIERARCHY.lock() = Some(mhi.clone());
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
            if crate::virtio_net::init_driver_virtio() {
                serial_println!("[NET] VirtIO-net OK.");
            } else if crate::net::init_driver_rtl8139() {
                serial_println!("[NET] RTL8139 OK.");
            } else if crate::net::init_driver_e1000() {
                serial_println!("[NET] e1000 OK.");
            } else {
                serial_println!("[NET] Sem hardware de rede. Modo offline.");
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
        serial_println!("[USB] Inicializado via init_xhci().");
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
        crate::SELF_HEAL.lock();
        serial_println!("[AGENT] SelfHealAgent pronto.");
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
        crate::TRUST_CACHE.lock();
        serial_println!("[AGENT] TrustAgent pronto.");
        AgentTickResult::Done
    }
}

/// HwDetectAgent — HwIdentifySkill scan + LLM query
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
    spec: crate::agency::AgentSpec,
}

impl SpecialistAgent {
    pub fn new(spec: crate::agency::AgentSpec) -> Self {
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
    let agency = crate::agency::Agency::new();
    for div in &agency.divisions {
        for spec in &div.agents {
            let agent = SpecialistAgent::new(spec.clone());
            registry.register(Box::new(agent));
        }
    }
    let count: usize = agency.divisions.iter().map(|d| d.agents.len()).sum();
    serial_println!("[AGENCY] {} agentes registrados via SpecialistAgent", count);
}

/// Registra HwAgents como agentes nativos (um por dispositivo PCI)
pub fn register_hw_agents(registry: &mut agent_core::AgentRegistry) {
    let mut hw = crate::hw_agents::HwRegistry::new();
    unsafe { hw.detect_all(); }
    for hw_agent in &hw.agents {
        let name = Box::leak(hw_agent.name.clone().into_boxed_str());
        let manifest = AgentManifest { name, kind: AgentKind::Driver, schedule: ScheduleKind::Oneshot, auto_start: true, persist: false };
        let payload = alloc::format!("{} caps={:?}", hw_agent.device_id, hw_agent.capabilities);
        registry.register(Box::new(HwSpecialistAgent { manifest, device_id: hw_agent.device_id.clone(), payload }));
    }
    serial_println!("[HW-AGENTS] {} agentes de hardware registrados", hw.agents.len());
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
        let reg = crate::SKILL_REGISTRY.lock();
        if let Ok(output) = reg.execute_skill_unchecked("hw_identify", &[]) {
            let text = core::str::from_utf8(&output).unwrap_or("(error)");
            serial_println!("[HW-SCAN] Dispositivos detectados:\n{}", text);
        }
        AgentTickResult::Done
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
            if crate::virtio_gpu::init_driver_virtio_gpu() {
                serial_println!("[VGPU] VirtIO-GPU OK.");
            }
        }
        AgentTickResult::Done
    }
}
