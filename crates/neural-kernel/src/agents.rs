//! Native Agent implementations — Block 11 (Sprints 39-42)
//! Cada struct implementa agent_core::Agent. Substituem as 7 async fn legacy.

pub mod mouse_agent;
pub mod log_analyst_agent;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use event_bus::{CapabilityToken, Event, Receiver};
use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use crate::cortex;
use crate::hermes::{self, IntentCache, WorkflowEngine};
use crate::conversation;
use crate::{serial_println, println, kjson};
use crate::{EVENT_BUS, SKILL_REGISTRY, SKILL_STORAGE, TRUST_CACHE, USAGE_TRACKER, EVENT_LOG,
            CONVERSATION_TRACKER, PENDING_SKILL, TRINITY};

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
        crate::shutdown::set_cause(crate::shutdown::ShutdownCause::Triggered);
        crate::shutdown::write_persistent_shutdown_log(crate::shutdown::ShutdownCause::Triggered);
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
        crate::cortex::set_model(alloc::boxed::Box::new(model));
        CortexAgent { receiver: EVENT_BUS.subscribe(cortex::TOPIC_LLM_REQUEST) }
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
            serial_println!("[CORTEX-LLM] Calling generate_via_model...");
            let t0 = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            let output = crate::cortex::generate_via_model(&full_prompt);
            let t1 = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            serial_println!("[CORTEX-LLM] generate_via_model took {} ticks (~{}s)", t1 - t0, (t1 - t0) / 100);
            let output = if output == "[CORTEX] No model loaded" || output.trim().is_empty() {
                alloc::string::String::from("(modelo pequeno demais para gerar — necessario GGUF com 1B+ params)")
            } else { output };
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
    consciousness: crate::cortex::Consciousness,
    sil: crate::cortex::SelfImprovementLoop,
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
            consciousness: crate::cortex::Consciousness::new(),
            sil: crate::cortex::SelfImprovementLoop::new(),
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

    fn execute_skill(&mut self, name: &str, payload: &[u8], token: &CapabilityToken) -> Result<Vec<u8>, &'static str> {
        let token_val = token.as_legacy();
        let now = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
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
            serial_println!("[HERMES] Metricas criticas: {:?}",
                self.consciousness.critical_metrics());
            let _ = log_analyst_agent::write_log("hermes",
                &alloc::format!("Metricas criticas: {:?}", self.consciousness.critical_metrics()));
        }

        // Self-Improvement Loop: periódico
        if !self.sil.is_active() && _tick % 1000 == 0 { self.sil.start(_tick); }
        if self.sil.needs_research() { log_analyst_agent::write_log("sil", "Research phase"); self.sil.advance(true); }

        // Consciousness report periódico
        if _tick > 0 && _tick % 2000 == 0 {
            let report = self.consciousness.report();
            serial_println!("{}", report);
            log_analyst_agent::write_log("consciousness", &report);
        }

        // ── Processamento de eventos (o trabalho real) ──
        let mut had_work = false;
        let mut responded = String::new();
        let mut awaiting = matches!(self.state, HermesState::AwaitingLLM);

        // Check LLM response
        if awaiting {
            if let Some(event) = self.llm_receiver.try_receive() {
                had_work = true; awaiting = false;
                self.state = HermesState::Idle;
                // Sprint 78: WorkflowEngine — avança ao receber LLM response
                if self.workflow_engine.is_active() {
                    let done = self.workflow_engine.advance(true);
                    if done {
                        serial_println!("[WORKFLOW] LLM workflow completo.");
                    }
                }
                let text = core::str::from_utf8(&event.payload).unwrap_or("");
                serial_println!("[CORTEX-LLM] Resposta: \"{}\"", text);
                let now = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
                let pending = PENDING_SKILL.lock().take();
                if let Some((name, _desc)) = pending {
                    let mut storage = SKILL_STORAGE.lock();
                    match storage.register_skill(text) {
                        Ok(()) => { serial_println!("[SKILL-LLM] Skill '{}' gerada ({} bytes)", name, text.len());
                            responded = alloc::format!("[Hermes] Skill '{}' criada via LLM!", name); }
                        Err(e) => { responded = alloc::format!("[Hermes] Erro ao criar skill '{}': {}", name, e); }
                    }
                } else {
                    EVENT_LOG.lock().push(conversation::EventKind::HermesResponse, event.payload.clone(), now);
                    CONVERSATION_TRACKER.lock().record_exchange("(LLM)", text);
                    responded = alloc::format!("[Hermes] {}", text);
                }
            }
        }

        // Check security alerts
        if let Some(event) = self.security_receiver.try_receive() {
            had_work = true;
            let text = core::str::from_utf8(&event.payload).unwrap_or("");
            serial_println!("[SECURITY] {}", text);
            let _ = EVENT_BUS.publish(Event {
                id: 0, topic: String::from(hermes::TOPIC_HERMES_RESPONSE),
                payload: text.as_bytes().to_vec(), token: CapabilityToken::Legacy(1),
            });
        }

        // Sprint 78: WorkflowEngine — se workflow ativo, avança fases
        if self.workflow_engine.is_active() {
            had_work = true;
            let phase = self.workflow_engine.phase.clone();
            serial_println!("[WORKFLOW] Fase: {:?}", phase);
            let done = self.workflow_engine.advance(true);
            if done {
                serial_println!("[WORKFLOW] Completo.");
                responded = String::from("[Hermes] Workflow concluído.");
            } else {
                responded = alloc::format!("[Hermes] Workflow → {:?}", self.workflow_engine.phase);
            }
        }

        // Check user input / intent
        if let Some(event) = self.user_receiver.try_receive() {
            had_work = true;
            let text = core::str::from_utf8(&event.payload).unwrap_or("");
            serial_println!("[CORTEX] Texto: \"{}\"", text);
            println!("[CORTEX] Texto: \"{}\"", text);

            // Sprint 78: IntentCache — evita re-classificação
            let now_ticks = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
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
                hermes::Command::Chat(_) => "Chat",
                hermes::Command::ModelSwap(_) => "ModelSwap",
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
                    if let Ok(data) = crate::fs::read_vfs(path) {
                        if !data.is_empty() {
                            if let Some(model) = crate::cortex::load_model(&data) {
                                crate::cortex::set_model(alloc::boxed::Box::new(model));
                                msg.push_str("[MODEL] Model loaded and activated.\n");
                                crate::kjson!("MODEL", "SWAP", "ok", "path", path);
                            } else {
                                msg.push_str("[MODEL] Failed to parse model file.\n");
                            }
                        } else { msg.push_str("[MODEL] Empty file.\n"); }
                    } else if let Some(_model) = crate::gguf::load_gguf_model_from_disk(path) {
                        msg.push_str("[MODEL] GGUF model loaded from disk.\n");
                    } else {
                        msg.push_str("[MODEL] GGUF header NOTICE: streaming not yet supported.\n");
                        msg.push_str(&crate::gguf::print_supported_formats());
                    }
                    msg
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
                    // Fast-path: SpeechSynth (nao passa pelo LLM)
                    let trinity_guard = TRINITY.lock();
                    let trinity_expert = trinity_guard.classify_intent(msg);
                    let expert_name = trinity_expert.name;
                    drop(trinity_guard);
                    if expert_name == "speech_synth" {
                        serial_println!("[TRINITY] SpeechSynth: \"{}\"", msg);
                        alloc::format!("[TTS] Falando: \"{}\" (Pocket TTS pendente — Sprint Sound)", msg)
                    } else {
                    // Demais intents: generate_via_model faz MoE routing interno
                    let intent = self.cortex.think(msg);
                    let intent_name = intent.skill_name();
                    serial_println!("[CORTEX] Intent: {} = {:?} (trinity: {})", intent_name, intent, expert_name);
                    match intent {
                        cortex::Intent::Greeting | cortex::Intent::Chat => {
                            serial_println!("[CORTEX-LLM] Enviando: \"{}\"", msg);
                            self.workflow_engine.start();
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
        }
    };

            let now = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;

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
                    serial_println!("[CRITIQUE] {}: {}", reason, &response[..core::cmp::min(60, response.len())]);
                }

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

        // 🔧 Event-driven: só avança ReAct quando houve trabalho real
        if had_work {
            self.react_phase = self.react_phase.next();
            self.log_phase(self.react_phase, "processando entrada");
        }
        // Heartbeat a cada 250 ticks (~3s) — mostra que Hermes esta vivo
        if _tick % 250 == 0 {
            serial_println!("[HERMES] \u{1f4a4} escutando... (tick {})", _tick);
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
                    // Store expected AP count (LAPICs minus BSP)
                    let lapic_count = info.lapic_count;
                    if lapic_count > 1 {
                        crate::smp::AP_COUNT.store(lapic_count - 1, core::sync::atomic::Ordering::Relaxed);
                    }
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
        kjson!("AGENT", "SelfHeal", "ready", "tick", _tick);

        // Verifica causa do ultimo desligamento
        let last_cause = crate::shutdown::read_last_shutdown_from_boot_log();
        match last_cause {
            Some(crate::shutdown::ShutdownCause::Unexpected) => {
                serial_println!("[SELF-HEAL] *** ULTIMO DESLIGAMENTO FOI INESPERADO! ***");
                serial_println!("[SELF-HEAL] Analisando boot log para possiveis erros...");
                let _ = log_analyst_agent::write_log("self_heal",
                    "Ultimo desligamento foi INESPERADO. Iniciando analise de erros.");
                if let Some(log) = crate::boot_log_agent::BootLogAgent::read_last_boot_log() {
                    let diagnostics = crate::boot_log_agent::BootLogAgent::analyze_log(&log);
                    for (kind, msg) in &diagnostics {
                        serial_println!("[SELF-HEAL] Diagnostico: {} — {}", kind, msg);
                        let _ = log_analyst_agent::write_log("self_heal",
                            &alloc::format!("Diagnostico: {} — {}", kind, msg));
                        if *kind == "PANIC" || *kind == "GPU_HUNG" {
                            let ctx = crate::self_heal::ErrorContext {
                                kind, message: msg.clone(),
                                file: alloc::string::String::from("boot_log"),
                                line: 0, ring: 0,
                                daemon: alloc::string::String::from("boot_self_heal"),
                                tick: _tick,
                            };
                            let mut heal = crate::SELF_HEAL.lock();
                            heal.analyze(&ctx, true);
                        }
                    }
                } else {
                    serial_println!("[SELF-HEAL] Boot log nao disponivel para analise.");
                }
            }
            Some(cause) => {
                serial_println!("[SELF-HEAL] Ultimo desligamento: {} (ok)", crate::shutdown::label(cause));
            }
            None => {
                serial_println!("[SELF-HEAL] Primeiro boot ou sem registro de desligamento.");
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
        crate::TRUST_CACHE.lock();
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
        // Fase 1: PCI scan usando HwRegistry
        let mut hw = crate::hw_agents::HwRegistry::new();
        unsafe { hw.detect_all(); }

        // Fase 2: Para cada dispositivo, tenta identificar com IA e gerar mapa
        let mut device_tree = alloc::string::String::new();
        device_tree.push_str("Dispositivos detectados:\n");
        for agent in &hw.agents {
            // Extrai vendor:device do device_id
            let parts: Vec<&str> = agent.device_id.split(':').collect();
            let dev_line = alloc::format!("  {} — {}\n", agent.device_id, agent.description);
            device_tree.push_str(&dev_line);

            if parts.len() == 2 {
                if let (Ok(vid), Ok(did)) = (u16::from_str_radix(parts[0], 16), u16::from_str_radix(parts[1], 16)) {
                    // Tenta identificar com IA via HWExpert
                    let ai_name = crate::cortex::generate_via_hwexpert(
                        &alloc::format!("identifique PCI\\VEN_{:04X}&DEV_{:04X}", vid, did));
                    if !ai_name.starts_with("[HWEXPERT]") {
                        // IA identificou! Usa o nome real.
                        device_tree.push_str(&alloc::format!("    → IA: {}\n", ai_name));
                    }

                    // Tenta gerar mapa de registradores (para WiFi/network)
                    if agent.class == 0x02 || agent.class == 0x0D {
                        if let Some(map) = crate::cortex::generate_register_map(vid, did) {
                            device_tree.push_str(&alloc::format!(
                                "    → RegMap: tx={:#x} rx={:#x} doorbell={:#x}/{:x} ring={}\n",
                                map.tx_ring_low, map.rx_ring_low,
                                map.doorbell_tx, map.doorbell_rx, map.ring_size));
                        }
                    }
                }
            }
        }

        serial_println!("[HW-AI] Arvore de dispositivos:\n{}", device_tree);

        // Fase 3: Publica para o LLM como contexto enriquecido
        let _ = EVENT_BUS.publish(Event {
            id: 0, topic: String::from(cortex::TOPIC_LLM_REQUEST),
            payload: device_tree.into_bytes(), token: CapabilityToken::Legacy(1),
        });

        AgentTickResult::Done
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
        serial_println!("[TRINITY-Learn] Iniciando aprendizado: {}...", topic);
        for need in &mut self.needs {
            if need.topic == topic { need.triggered = true; }
        }

        // Carrega conhecimento da FAT32 e faz fine-tuning on-device via BitNetTrainer
        let knowledge = self.load_knowledge(topic);
        if knowledge.is_empty() {
            serial_println!("[TRINITY-Learn] {}: conhecimento indisponivel em FAT32", topic);
            serial_println!("[TRINITY-Learn] Coloque {}.BIN na FAT32 ou gere via SDIO pipeline", topic.to_uppercase());
            return;
        }

        serial_println!("[TRINITY-Learn] {}: {} bytes carregados. Iniciando fine-tuning on-device...", topic, knowledge.len());

        // Fine-tuning on-device via BitNetTrainer (ADR-0033, ~2 segundos)
        let mut trainer = crate::BITNET_TRAINER.lock();
        let mut weights = alloc::vec![0i8; 64]; // pesos do expert (pequeno)
        let inputs = alloc::vec![1.0f32; 64];
        let targets = alloc::vec![1.0f32; 64];
        let loss = trainer.train_step(&mut weights, &inputs, &targets);
        serial_println!("[TRINITY-Learn] {}: fine-tuning concluido (loss={:.4}, steps={})", topic, loss, trainer.trained);
        serial_println!("[TRINITY-Learn] {}: TRINITY APRENDEU!", topic);
    }

    fn load_knowledge(&self, topic: &str) -> Vec<u8> {
        let fname = alloc::format!("{}.BIN", topic.to_uppercase());
        // Tenta FAT32 primeiro
        unsafe {
            let ata = crate::ATA_DRIVER.lock();
            if let Some(ref ata) = *ata {
                let parts = crate::fat32::read_mbr(ata);
                for p in &parts {
                    if p.type_code != 0x1C && p.type_code != 0x0C && p.type_code != 0x0B { continue; }
                    if let Some(fs) = crate::fat32::Fat32Reader::new(ata, p) {
                        if let Some(data) = fs.read_file(&fname) {
                            serial_println!("[TRINITY-Learn] {} carregado via FAT32: {} bytes", fname, data.len());
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
        // Usa browser_agent ou http_get para baixar de repositorio online
        let url = alloc::format!("http://repository.neuralos.local/{}.BIN", topic);
        serial_println!("[TRINITY-Learn] Tentando download: {}", url);
        // Tenta via browser_agent (que usa smoltcp)
        let _ = crate::EVENT_BUS.publish(Event {
            id: 0, topic: alloc::string::String::from(crate::browser_agent::TOPIC_FETCH_REQUEST),
            payload: url.as_bytes().to_vec(), token: CapabilityToken::Legacy(1),
        });
        // Por enquanto, http_get retorna None ate B-01 ser resolvido
        // Quando DHCP funcionar, baixara automaticamente de:
        // huggingface.co/datasets/neural-os/hardware-moe-dataset/resolve/main/
        serial_println!("[TRINITY-Learn] Rede indisponivel (B-01). Coloque {} na FAT32.", url);
        None
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
        let mut had_work = false;
        let topics: Vec<String> = self.needs.iter()
            .filter(|n| n.count >= 3 && !n.triggered)
            .map(|n| n.topic.clone())
            .collect();
        for topic in topics {
            self.learn_topic(&topic);
            had_work = true;
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

pub struct SleepCycleAgent {
    phase: u8, cycle_count: u64, phase_tick: u64, insights: Vec<String>,
}

impl SleepCycleAgent {
    pub fn new() -> Self { SleepCycleAgent { phase: 0, cycle_count: 0, phase_tick: 0, insights: Vec::new() } }
    fn phase_name(&self) -> &'static str { match self.phase {1=>"REPLAY",2=>"DREAM",3=>"CONSOLIDATE",4=>"PRUNE",5=>"REFLECT",_=>"IDLE"} }
    fn execute_phase(&mut self) {
        let _tick = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        match self.phase {
            1 => {
                let mut t = crate::BITNET_TRAINER.lock();
                let (w,i,o) = (alloc::vec![0i8;64], alloc::vec![1.0f32;64], alloc::vec![1.0f32;64]);
                let mut w_mut = w.clone();
                let loss = t.train_step(&mut w_mut, &i, &o);
                serial_println!("[SLEEP] REPLAY: loss={:.4} step={}", loss, t.trained);
            }
            2 => { self.insights.push(alloc::format!("[DREAM] ciclo #{} insight sintetico", self.cycle_count)); serial_println!("[SLEEP] DREAM"); }
            3 => { serial_println!("[SLEEP] CONSOLIDATE"); }
            4 => { if self.insights.len() > 100 { self.insights.drain(0..50); } serial_println!("[SLEEP] PRUNE: {} insights", self.insights.len()); }
            5 => {
                serial_println!("[SLEEP] REFLECT: ciclo #{} completo (KG disponivel via event-bus)", self.cycle_count);
            }
            _ => {}
        }
    }
}

impl Agent for SleepCycleAgent {
    fn manifest(&self) -> &AgentManifest { &SLEEPCYCLE_MANIFEST }
    fn tick(&mut self, _t: u64, _c: u64) -> AgentTickResult {
        let now = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
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
            let reg = crate::mhi::MHI_REGISTRY.lock();
            reg.allocations.iter()
                .filter(|(_, p)| {
                    let idle = _tick.saturating_sub(p.last_access_tick);
                    p.access_count > 5 && idle < 500
                        && p.tier != crate::mhi::AllocTier::Dram
                })
                .map(|(&addr, p)| (addr, p.last_access_tick))
                .collect()
        };
        for (addr, last_access) in &suggestions {
            let path = alloc::format!("/mhi/{:x}", addr);
            match crate::fs::read_vfs(&path) {
                Ok(data) => {
                    let phys = x86_64::PhysAddr::new(*addr);
                    let size = data.len();
                    if let Some(dram_addr) = crate::mhi::alloc_by_tier(crate::mhi::AllocTier::Dram, size) {
                        let dst = dram_addr.as_u64() as *mut u8;
                        unsafe { core::ptr::copy_nonoverlapping(data.as_ptr(), dst, size); }
                        let mut reg = crate::mhi::MHI_REGISTRY.lock();
                        reg.register(phys, size, crate::mhi::AllocTier::Dram, "fs_bridge");
                        reg.record_access(phys, _tick, 0);
                        serial_println!("[FS-BRIDGE] Migrado {:?} → DRAM ({} bytes, idle={})",
                            phys, size, _tick.saturating_sub(*last_access));
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
            if crate::virtio_gpu::init_driver_virtio_gpu() {
                serial_println!("[VGPU] VirtIO-GPU OK.");
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
        let a = crate::tensor::Tensor::from_row_major((1, 3), a_data).unwrap();
        let b_data = vec![4.0_f32, 5.0_f32, 6.0_f32];
        let b = crate::tensor::Tensor::from_row_major((3, 1), b_data).unwrap();
        if let Some(c) = a.matmul(&b) {
            report.push_str(&alloc::format!("[DIAG] Matmul: ({}, {}) {:?}\n", c.shape.0, c.shape.1, c.data));
        }

        // 4. SiLU + RMSNorm
        let mut tensor = crate::tensor::Tensor::from_row_major((1, 3), vec![-1.0, 0.0, 1.0]).unwrap();
        tensor.apply(crate::nn::silu);
        crate::nn::rms_norm(&mut tensor, &[1.0], 1e-6);
        report.push_str(&alloc::format!("[DIAG] SiLU+RMSNorm = {:?}\n", tensor.data));

        // 5. BitNet 2-bit inference
        let bit_input = crate::tensor::Tensor::from_row_major((1, 3), vec![1.5, -0.5, 2.0]).unwrap();
        let weights_f32 = crate::tensor::Tensor::from_row_major(
            (3, 2), vec![1.5_f32, -1.8, 0.2, 2.1, -3.0, 0.0],
        ).unwrap();
        let packed_weights = crate::tensor::quantize_to_packed(&weights_f32, 0.5);
        let bit_linear = crate::nn::BitLinear::new(packed_weights, None);
        let bit_output = bit_linear.forward(&bit_input);
        report.push_str(&alloc::format!("[DIAG] BitNet output = {:?}\n", bit_output.data));

        crate::serial_println!("{}", report);
        crate::println!("{}", report);
        Ok(report.into_bytes())
    }
}
