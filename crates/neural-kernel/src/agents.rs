//! Native Agent implementations — Block 11 (Sprints 39-42)
//! Cada struct implementa agent_core::Agent. Substituem as 7 async fn legacy.

pub mod mouse_agent;
pub mod sysinfo_agent;
pub mod log_analyst_agent;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use event_bus::{CapabilityToken, Event, Receiver};
use event_bus::latent::{LatentReceiver, TOPIC_THOUGHT_LLM};
use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use crate::cortex;
use crate::hermes::{self, IntentCache, WorkflowEngine};
use crate::conversation;
use crate::{println, kjson};
use crate::{EVENT_BUS, SKILL_STORAGE, TRUST_CACHE, USAGE_TRACKER, EVENT_LOG,
            CONVERSATION_TRACKER, PENDING_SKILL, TRINITY};
// P001: SKILL_REGISTRY canônico agora em k_nano::globals (cross-crate).
use k_nano::SKILL_REGISTRY;
use jarbas_crate::vconsole;

/// Input pendente aguardando resposta do LLM — alimenta o SelfLearningAgent
/// (k_ai) com o par (input → resposta) quando o LLM responder.
static PENDING_LEARNER_INPUT: spin::Mutex<Option<String>> = spin::Mutex::new(None);

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
            Ok(()) => { k_nano::slog_bin!("Agent", "monitor", "Evento SYSTEM_READY publicado."); }
            Err(e) => { k_nano::slog_bin!("Agent", "monitor", "Falha: {}", e); }
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
    caps: bool,
}

impl InputAgent {
    pub fn new() -> Self {
        InputAgent { receiver: EVENT_BUS.subscribe("RAW_HW_IRQ1"), buffer: String::new(), ctrl: false, alt: false, shift: false, caps: false }
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
            // Left Shift = 0x2A, Right Shift = 0x36 (break 0xAA/0xB6 clears)
            0x2A | 0x36 => { self.shift = pressed; }
            // CapsLock: toggle only on make (0x3A); break 0xBA no-op
            0x3A if pressed => { self.caps = !self.caps; }
            0x53 if self.ctrl && self.alt && pressed => { self.handle_cad(); }
            // F1-F6 (0x3B-0x40) with Ctrl+Alt for virtual console switching
            k @ 0x3B..=0x40 if self.ctrl && self.alt && pressed => {
                let fn_idx = k - 0x3B; // F1=0, F2=1, ..., F6=5
                if fn_idx < 6 {
                    vconsole::on_ctrl_alt_fn(fn_idx);
                }
            }
            _ => {}
        }
        if !pressed { return; }
        if scancode >= 0x80 { return; }
        match scancode {
            0x1C => {
                let text = core::mem::take(&mut self.buffer);
                if !text.is_empty() {
                    k_nano::slog_bin!("Input", "info", "ENTER — USER_INTENT: \"{}\"", text);
                    println!("[INPUT] ENTER — USER_INTENT: \"{}\"", text);
                    let _ = EVENT_BUS.publish(Event {
                        id: 0, topic: String::from("USER_INTENT"),
                        payload: text.into_bytes(), token: CapabilityToken::Legacy(1),
                    });
                }
            }
            0x0E => { self.buffer.pop(); }
            _ => { if let Some(ch) = k_nano::scancode_to_ascii(scancode, self.shift, self.caps) { self.buffer.push(ch); } }
        }
        // Echo tecla para o display em tempo real
        let _ = EVENT_BUS.publish(Event {
            id: 0, topic: String::from("KEYBOARD_ECHO"),
            payload: self.buffer.clone().into_bytes(),
            token: CapabilityToken::Legacy(1),
        });
    }
    fn handle_cad(&self) {
        crate::shutdown::begin_orderly_shutdown(crate::shutdown::ShutdownCause::Triggered);
    }
}

// ---------------------------------------------------------------------------
// NetAgent — smoltcp poll loop (Continuous). Gate Sprint Net = e1000 [smoltcp/NIC], não SLIP.
// Tick pós init_phase (SelfHeal/Disk Done) → network_agent_tick; log `[NET] tick` cedo.
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
        // Modelo real só após load (ramdisk/FAT32/QEMU loader) — não declarar aqui
        k_nano::slog_cortex!("LLM", "info", "CortexAgent ativo; aguardando load do modelo.");
        CortexAgent { receiver: EVENT_BUS.subscribe(cortex::TOPIC_LLM_REQUEST) }
    }
}

impl Agent for CortexAgent {
    fn manifest(&self) -> &AgentManifest { &CORTEX_MANIFEST }
    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        if let Some(event) = self.receiver.try_receive() {
            let user_text = core::str::from_utf8(&event.payload).unwrap_or("");
            // Classifica SÓ o utterance (nunca o envelope de skills).
            let expert = {
                let t = crate::TRINITY.lock();
                t.classify_intent(user_text).name
            };
            k_nano::slog_cortex!(
                "LLM",
                "info",
                "Generating for: \"{}\" route→{}",
                user_text,
                expert
            );
            let _ = crate::global_arena::take_pending_route();
            k_nano::slog_cortex!("LLM", "info", "Calling generate_via_model...");
            let t0 = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            let output = match expert {
                "hw_control" => {
                    // Volume/mute/brilho — skill/HW, sem LLM.
                    crate::cortex::generate_via_model_with_route(user_text, "hw_control")
                }
                "generator" => {
                    // Chat fluente: prompt limpo (sem dump L0 que sequestrava MoE).
                    let chat = alloc::format!(
                        "You are Jarbas, the Neural OS voice assistant. \
                         Reply with one short fluent conversational sentence. \
                         Match the user language (PT-BR or EN).\nUser: {}\nJarbas:",
                        user_text
                    );
                    crate::cortex::generate_via_model_with_route(&chat, "generator")
                }
                other => {
                    let system_prompt = SKILL_STORAGE.lock().build_system_prompt_for(user_text);
                    let full_prompt =
                        alloc::format!("{}. PERGUNTA: {}", system_prompt, user_text);
                    crate::cortex::generate_via_model_with_route(&full_prompt, other)
                }
            };
            let t1 = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            k_nano::slog_cortex!("LLM", "info", "generate_via_model took {} ticks (~{}s)", t1 - t0, (t1 - t0) / 100);
            let output = if output == "[CORTEX] No model loaded" || output.trim().is_empty() {
                alloc::format!(
                    "(sem LLM gerador — {})",
                    crate::model_hub::hub_status()
                )
            } else {
                output
            };
            k_nano::slog_cortex!("LLM", "info", "Generated: \"{}\"", output);
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
    health_receiver: Receiver,
    pnp_receiver: Receiver,
    cap_receiver: Receiver,
    shutdown_receiver: Receiver,
    reboot_receiver: Receiver,
    latent_receiver: LatentReceiver,
    latent_recv_total: u64,
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
            health_receiver: EVENT_BUS.subscribe("HEALTH_ISSUE"),
            pnp_receiver: EVENT_BUS.subscribe(k_ai::hw_capability::TOPIC_HW_PNP_ACTION),
            cap_receiver: EVENT_BUS.subscribe(k_ai::hw_capability::TOPIC_HW_CAPABILITY),
            shutdown_receiver: EVENT_BUS.subscribe(crate::shutdown::TOPIC_SYSTEM_SHUTDOWN),
            reboot_receiver: EVENT_BUS.subscribe(crate::shutdown::TOPIC_SYSTEM_REBOOT),
            latent_receiver: crate::LATENT_BUS.subscribe(TOPIC_THOUGHT_LLM),
            latent_recv_total: 0,
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
        k_nano::slog_hermes!("Hermes", "info", "{} — {}", phase.label(), detail);
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
        k_nano::slog_bin!("Log", "msg", "{}", sdd.display());
    }

    fn execute_skill(&mut self, name: &str, payload: &[u8], token: &CapabilityToken) -> Result<Vec<u8>, &'static str> {
        let token_val = token.as_legacy();
        let now = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        // P3: skills de rede exigem Cap::RING_OP (além do CapabilityToken legado).
        // Token Legacy(1) = boot/trustado → concede RING_OP; demais = Cap vazia.
        let held = if matches!(token, CapabilityToken::Legacy(1)) {
            crate::syscall::Cap::RING_OP
                .union(crate::syscall::Cap::PING)
        } else {
            crate::syscall::Cap::EMPTY
        };
        let lower = name.to_ascii_lowercase();
        if lower.contains("net") || lower.contains("http") || lower.contains("tcp")
            || lower.contains("wifi") || name == "aios_send_tcp"
        {
            crate::capability_gate::check(
                crate::capability_gate::HOST_FN_SEND_TCP,
                held,
            )?;
        }
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
            k_nano::slog_bin!("Log", "msg", "{}", greeting);
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
            hermes_crate::cognitive_bridge::note_latent(&alloc::format!(
                "thought#{} n={:.2}",
                pkt.id, norm
            ));
            if self.latent_recv_total <= 3 || self.latent_recv_total % 32 == 0 {
                k_nano::slog_bin!("HERMES", "LATENT", "recv id={} norm={:.3} total={}", pkt.id, norm, self.latent_recv_total);
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

        // Self-Improvement Loop: periódico — Sprint 108 fecha Create/Improve/Verify
        if !self.sil.is_active() && _tick % 1000 == 0 {
            if self.sil.start(_tick) {
                k_nano::slog_bin!("S108", "SIL", "start Research @ tick {}", _tick);
            }
        }
        if self.sil.needs_research() {
            // Research: padrões observados ≥3 → oportunidade
            let pending = crate::skill_observer::pending_observations().len();
            let found = pending > 0 || crate::self_evolve::counters().0 > 0
                || !crate::skill_observer::count_by_skill().is_empty();
            log_analyst_agent::write_log("sil", if found { "Research: patterns found" } else { "Research: idle" });
            self.sil.advance(found);
        }
        if self.sil.needs_create() {
            let mut storage = SKILL_STORAGE.lock();
            let n = crate::self_evolve::auto_generate_pending(&mut storage);
            drop(storage);
            k_nano::slog_bin!("S108", "SIL", "Create: {} skills", n);
            self.sil.advance(n > 0);
        }
        if self.sil.needs_improve() {
            let mut storage = SKILL_STORAGE.lock();
            let n = crate::self_evolve::improve_failed(&mut storage);
            drop(storage);
            k_nano::slog_bin!("S108", "SIL", "Improve: {} skills", n);
            self.sil.advance(true); // ciclo avança; fila vazia = noop ok
        }
        if self.sil.needs_verify() {
            // Verificação: rejeições vs oks
            let (_g, ok, rej, _i, _r) = crate::self_evolve::counters();
            let pass = rej == 0 || ok >= rej;
            k_nano::slog_bin!("S108", "SIL", "Verify: ok={} rej={} pass={}", ok, rej, pass);
            self.sil.advance(pass);
            if pass {
                k_nano::slog_bin!("Log", "msg", "{}", crate::self_evolve::status_line());
            }
        }

        // Consciousness report periódico
        if _tick > 0 && _tick % 2000 == 0 {
            let report = self.consciousness.report();
            k_nano::slog_bin!("Log", "msg", "{}", report);
            log_analyst_agent::write_log("consciousness", &report);
            k_nano::slog_bin!("Log", "msg", "{}", crate::self_evolve::status_line());
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
                        k_nano::slog_bin!("Workflow", "info", "LLM workflow completo.");
                    }
                }
                let text = core::str::from_utf8(&event.payload).unwrap_or("");
                k_nano::slog_cortex!("LLM", "info", "Resposta: \"{}\"", text);
                let now = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
                let pending = PENDING_SKILL.lock().take();
                if let Some((name, _desc)) = pending {
                    // Sign FIRST (verify_and_register) → verificação estrita
                    // ADR-0052 → register. Sem pre-check em conteúdo cru.
                    let mut storage = SKILL_STORAGE.lock();
                    match crate::self_evolve::verify_and_register(&mut storage, text) {
                        Ok(n) => {
                            k_nano::slog_bin!("Skill", "llm", "Skill '{}' gerada+verified ({} bytes)", n, text.len());
                            responded = alloc::format!("[Hermes] Skill '{}' criada via LLM!", n);
                            if self.sil.needs_create() || self.sil.needs_verify() {
                                self.sil.advance(true);
                            }
                            crate::self_evolve::record_outcome(&n, true, now);
                        }
                        Err(e) => {
                            responded = alloc::format!("[Hermes] Erro ao criar skill '{}': {}", name, e);
                            if self.sil.is_active() { self.sil.advance(false); }
                            crate::self_evolve::record_outcome(&name, false, now);
                        }
                    }
                } else {
                    EVENT_LOG.lock().push(conversation::EventKind::HermesResponse, event.payload.clone(), now);
                    CONVERSATION_TRACKER.lock().record_exchange("(LLM)", text);
                    hermes_crate::cognitive_bridge::after_exchange("(LLM)", text, now);
                    responded = alloc::format!("[Hermes] {}", text);
                    // ── LEARNER feedback: aprende o par (input pendente → resposta
                    // LLM) via singleton; fine-tune + persistência throttled (1x/200
                    // ticks). O fleet (PollEvery 5000) também roda o ciclo.
                    if let Some(input) = PENDING_LEARNER_INPUT.lock().take() {
                        let mut g = k_ai::self_learning::learner_global().lock();
                        if let Some(a) = g.as_mut() {
                            a.remember(&input, text);
                        }
                        drop(g);
                        k_nano::slog_bin!(
                            "LEARNER", "info",
                            "feedback par ({}B → {}B) memória atualizada",
                            input.len(), text.len()
                        );
                    }
                    static LEARN_TICK_LAST: core::sync::atomic::AtomicU64 =
                        core::sync::atomic::AtomicU64::new(0);
                    let lt_now = crate::interrupts::TIMER_TICKS
                        .load(core::sync::atomic::Ordering::Relaxed) as u64;
                    let lt_last = LEARN_TICK_LAST.load(core::sync::atomic::Ordering::Relaxed);
                    if lt_last == 0 || lt_now.wrapping_sub(lt_last) >= 200 {
                        LEARN_TICK_LAST.store(lt_now, core::sync::atomic::Ordering::Relaxed);
                        let loss = k_ai::self_learning::learn_tick_global();
                        if loss > 0.0 {
                            k_nano::slog_bin!("LEARNER", "info", "learn tick loss={:.4}", loss);
                        }
                    }
                }
            }
        }

        // Check security alerts
        if let Some(event) = self.security_receiver.try_receive() {
            had_work = true;
            let text = core::str::from_utf8(&event.payload).unwrap_or("");
            k_nano::slog_bin!("Sec", "info", "{}", text);
            let _ = EVENT_BUS.publish(Event {
                id: 0, topic: String::from(hermes::TOPIC_HERMES_RESPONSE),
                payload: text.as_bytes().to_vec(), token: CapabilityToken::Legacy(1),
            });
        }

        // Check health issues (firmware/skill ausentes)
        if let Some(event) = self.health_receiver.try_receive() {
            had_work = true;
            let text = core::str::from_utf8(&event.payload).unwrap_or("");
            hermes_crate::runtime_observe::ingest_health_issue(text);
        }

        // HW plug-and-play agentico: card completo → Hermes decide → efêmera → WASM
        while let Some(event) = self.cap_receiver.try_receive() {
            had_work = true;
            let text = core::str::from_utf8(&event.payload).unwrap_or("");
            let now = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
            let d = crate::hw_pnp::hermes_decide_card(text, now);
            k_nano::slog_bin!("Log", "msg", "{}", d.ack);
            let _ = EVENT_BUS.publish(Event {
                id: 0,
                topic: String::from(hermes::TOPIC_HERMES_RESPONSE),
                payload: d.ack.as_bytes().to_vec(),
                token: CapabilityToken::Legacy(1),
            });
            if let Some(md) = d.auto_skill_md.as_ref() {
                // Promote → PackageHub (HITL se unsigned)
                let hub = crate::package_hub::PACKAGE_HUB.lock();
                if let Ok((level, op)) = hub.stage_create(
                    crate::package_hub::PackageKind::Skill,
                    &d.skill_key,
                    md,
                    "auto-skill S108 / hw_pnp",
                ) {
                    drop(hub);
                    let id = crate::APPROVAL_GATE.lock().request(
                        &d.skill_key,
                        "hw_pnp",
                        "CREATE skill auto",
                        level,
                    );
                    crate::package_hub::PACKAGE_HUB.lock().bind_pending(id, op);
                    k_nano::slog_bin!("PnP", "info", "auto SKILL.md '{}' → pending #{} ({})",
                        d.skill_key,
                        id,
                        level.name());
                } else {
                    drop(hub);
                    let mut storage = SKILL_STORAGE.lock();
                    match crate::self_evolve::verify_and_register(&mut storage, md) {
                        Ok(n) => k_nano::slog_bin!("PnP", "info", "auto SKILL.md '{}'", n),
                        Err(e) => k_nano::slog_bin!("PnP", "info", "auto SKILL.md skip: {}", e),
                    }
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

        // HW_PNP_ACTION: hint curto já coberto pelo card — drena sem recontar SkillOpt.
        while let Some(event) = self.pnp_receiver.try_receive() {
            had_work = true;
            let text = core::str::from_utf8(&event.payload).unwrap_or("");
            k_nano::slog_bin!("HERMES", "PnP", "hint-wire {}", text);
        }

        // Sprint 78: WorkflowEngine — se workflow ativo, avança fases
        if self.workflow_engine.is_active() {
            had_work = true;
            let phase = self.workflow_engine.phase.clone();
            k_nano::slog_bin!("Workflow", "info", "Fase: {:?}", phase);
            let done = self.workflow_engine.advance(true);
            if done {
                k_nano::slog_bin!("Workflow", "info", "Completo.");
                responded = String::from("[Hermes] Workflow concluído.");
            } else {
                responded = alloc::format!("[Hermes] Workflow → {:?}", self.workflow_engine.phase);
            }
        }

        // Check user input / intent
        if let Some(event) = self.user_receiver.try_receive() {
            had_work = true;
            let text = core::str::from_utf8(&event.payload).unwrap_or("");
            k_nano::slog_bin!("CORTEX", "info", "Texto: \"{}\"", text);
            println!("[CORTEX] Texto: \"{}\"", text);

            // Soft power: desligar/reiniciar/hibernar — sem LLM
            if let Some(msg) = crate::shutdown::handle_power_phrase(text) {
                responded = alloc::format!("[Jarbas] {}", msg);
                k_nano::slog_bin!("SHUTDOWN", "info", "soft phrase → {}", msg);
                let _ = EVENT_BUS.publish(Event {
                    id: 0,
                    topic: String::from(hermes::TOPIC_HERMES_RESPONSE),
                    payload: responded.clone().into_bytes(),
                    token: CapabilityToken::Legacy(1),
                });
            } else {

            // Sprint 108: observar intent para auto-skill
            let now_obs = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
            crate::self_evolve::observe_intent(text, now_obs);

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
                hermes::Command::Scrape(_) => "Scrape",
            };
            let intent_info = crate::hermes::IntentInfo {
                intent_name: String::from(intent_name),
                confidence: 0.92,
                alternatives: Vec::new(),
            };
            k_nano::slog_bin!("Log", "msg", "{}", intent_info.display());
            self.show_sdd(intent_name);

            // #191: Council deliberation para comandos ambíguos (ex: Chat)
            if matches!(cmd, hermes::Command::Chat(_)) {
                let (opt, skep, prag) = crate::hermes::council_deliberate(text);
                k_nano::slog_bin!("Log", "msg", "{}", crate::hermes::council_display(&opt, &skep, &prag));
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

            // ── LEARNER (k_ai::self_learning) — recall associativo fast-path ──
            // Só para intents conversacionais; comandos de sistema (Status/Echo/…)
            // seguem no dispatch normal. Hit → resposta da memória sem custo de LLM.
            if matches!(cmd, hermes::Command::Chat(_) | hermes::Command::Conversation) {
                if let Some(learned) = k_ai::self_learning::recall(text) {
                    k_nano::slog_bin!("LEARNER", "info", "recall hit -> \"{}\"", learned);
                    let _ = EVENT_BUS.publish(Event {
                        id: 0,
                        topic: String::from(hermes::TOPIC_HERMES_RESPONSE),
                        payload: learned.into_bytes(),
                        token: CapabilityToken::Legacy(1),
                    });
                    // Skip LLM: resposta da memória associativa (fast-path).
                    return agent_core::AgentTickResult::Pending;
                }
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
                    match crate::net::resolve_and_http_get_safe(url.trim()) {
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
                // ponytail: Scrape = Fetch + HTML extração stubs (futuro, hermes crate tem real)
                hermes::Command::Scrape(ref target) => {
                    let url = if target.trim().starts_with("http://") || target.trim().starts_with("https://") {
                        target.trim().to_string()
                    } else {
                        alloc::format!("http://{}", target.trim())
                    };
                    match crate::net::resolve_and_http_get_safe(&url) {
                        Ok(body) => {
                            let text = core::str::from_utf8(&body).unwrap_or("(binary)");
                            let preview = if text.len() > 200 { &text[..200] } else { text };
                            alloc::format!("Scrape OK ({} bytes):\n{}\n\n_Use hermes crate Scrape para extração completa com parse HTML._", body.len(), preview)
                        }
                        Err(e) => alloc::format!("Scrape falhou: {} (use hermes crate para parse HTML real)", e),
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
                hermes::Command::Approve(id) => {
                    let skill_name = {
                        let gate = crate::APPROVAL_GATE.lock();
                        gate.pending()
                            .iter()
                            .find(|r| r.id == id)
                            .map(|r| r.skill.clone())
                    };
                    let gate_ok = crate::APPROVAL_GATE.lock().resolve(id, true);
                    if !gate_ok {
                        alloc::format!("Requisicao #{} nao encontrada ou ja resolvida.", id)
                    } else {
                        let now = crate::interrupts::TIMER_TICKS
                            .load(core::sync::atomic::Ordering::Relaxed)
                            as u64;
                        if skill_name.as_deref() == Some("llm_generate") {
                            hermes_crate::cognitive_bridge::grant_llm_after_approve(1, now);
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
                    let gate_ok = crate::APPROVAL_GATE.lock().resolve(id, false);
                    let _ = crate::package_hub::PACKAGE_HUB.lock().deny_pending(id);
                    if gate_ok {
                        alloc::format!("Requisicao #{} negada.", id)
                    } else {
                        alloc::format!("Requisicao #{} nao encontrada ou ja resolvida.", id)
                    }
                }
                hermes::Command::PendingApprovals => {
                    let pending = {
                        let gate = crate::APPROVAL_GATE.lock();
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
                hermes::Command::PkgCatalog => {
                    crate::package_hub::PACKAGE_HUB.lock().catalog_for_cortex()
                }
                hermes::Command::PkgList(ref kind_s) => {
                    let kind = kind_s.as_ref().and_then(|s| crate::package_hub::PackageKind::from_str(s));
                    let hub = crate::package_hub::PACKAGE_HUB.lock();
                    let list = hub.list(kind);
                    if list.is_empty() {
                        String::from("[PKG] nenhum pacote")
                    } else {
                        let mut msg = alloc::format!("[PKG] {} pacote(s):\n", list.len());
                        for p in list {
                            msg.push_str(&alloc::format!(
                                "  {} {} path={} signed={} purpose={}\n",
                                p.kind.as_str(), p.name, p.path, p.signed, p.purpose
                            ));
                        }
                        msg
                    }
                }
                hermes::Command::PkgGet(ref kind_s, ref name) => {
                    match crate::package_hub::PackageKind::from_str(kind_s) {
                        None => alloc::format!("[PKG] kind invalido: {}", kind_s),
                        Some(kind) => {
                            let hub = crate::package_hub::PACKAGE_HUB.lock();
                            match hub.get(kind, name) {
                                None => alloc::format!("[PKG] nao encontrado: {} {}", kind_s, name),
                                Some(p) => alloc::format!(
                                    "[PKG] {} '{}'\npath: {}\nsigned: {}\nhash: {}\ncaps: {}\npurpose: {}\n---\n{}",
                                    p.kind.as_str(), p.name, p.path, p.signed, p.content_hash,
                                    p.caps_hint, p.purpose, p.body
                                ),
                            }
                        }
                    }
                }
                hermes::Command::PkgInstall(ref kind_s, ref name, ref body) => {
                    match crate::package_hub::PackageKind::from_str(kind_s) {
                        None => alloc::format!("[PKG] kind invalido: {}", kind_s),
                        Some(kind) => {
                            let body_owned = if body.trim().is_empty() && kind == crate::package_hub::PackageKind::Skill {
                                crate::package_hub::minimal_skill_md(name, "installed via /pkg install")
                            } else {
                                body.clone()
                            };
                            let hub = crate::package_hub::PACKAGE_HUB.lock();
                            match hub.stage_create(kind, name, &body_owned, "user /pkg install") {
                                Err(e) => alloc::format!("[PKG] install recusado: {}", e),
                                Ok((level, op)) => {
                                    drop(hub);
                                    let id = crate::APPROVAL_GATE.lock().request(
                                        name, "package_hub",
                                        &alloc::format!("CREATE {}", kind.as_str()),
                                        level,
                                    );
                                    crate::package_hub::PACKAGE_HUB.lock().bind_pending(id, op);
                                    alloc::format!(
                                        "[PKG] CREATE {} '{}' pendente #{} nivel={} — /approve {} ou /deny {}",
                                        kind.as_str(), name, id, level.name(), id, id
                                    )
                                }
                            }
                        }
                    }
                }
                hermes::Command::PkgUpdate(ref kind_s, ref name, ref body) => {
                    match crate::package_hub::PackageKind::from_str(kind_s) {
                        None => alloc::format!("[PKG] kind invalido: {}", kind_s),
                        Some(kind) => {
                            let hub = crate::package_hub::PACKAGE_HUB.lock();
                            match hub.stage_update(kind, name, body) {
                                Err(e) => alloc::format!("[PKG] update recusado: {}", e),
                                Ok((level, op)) => {
                                    drop(hub);
                                    let id = crate::APPROVAL_GATE.lock().request(
                                        name, "package_hub",
                                        &alloc::format!("UPDATE {}", kind.as_str()),
                                        level,
                                    );
                                    crate::package_hub::PACKAGE_HUB.lock().bind_pending(id, op);
                                    alloc::format!(
                                        "[PKG] UPDATE {} '{}' pendente #{} — /approve {}",
                                        kind.as_str(), name, id, id
                                    )
                                }
                            }
                        }
                    }
                }
                hermes::Command::PkgRm(ref kind_s, ref name) => {
                    match crate::package_hub::PackageKind::from_str(kind_s) {
                        None => alloc::format!("[PKG] kind invalido: {}", kind_s),
                        Some(kind) => {
                            let hub = crate::package_hub::PACKAGE_HUB.lock();
                            match hub.stage_delete(kind, name) {
                                Err(e) => alloc::format!("[PKG] rm recusado: {}", e),
                                Ok((level, op)) => {
                                    drop(hub);
                                    let id = crate::APPROVAL_GATE.lock().request(
                                        name, "package_hub",
                                        &alloc::format!("DELETE {}", kind.as_str()),
                                        level,
                                    );
                                    crate::package_hub::PACKAGE_HUB.lock().bind_pending(id, op);
                                    alloc::format!(
                                        "[PKG] DELETE {} '{}' pendente #{} — /approve {}",
                                        kind.as_str(), name, id, id
                                    )
                                }
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
                hermes::Command::Commands => hermes_crate::hitl_ui::open_terminal_help(),
                hermes::Command::UiMode(ref arg) => {
                    if arg.trim().is_empty() {
                        hermes_crate::hitl_ui::mode_status()
                    } else {
                        match hermes_crate::hitl_ui::set_mode_str(arg) {
                            Ok(m) => alloc::format!(
                                "[HITL] ui_mode={} — intervenções via {}",
                                m.as_str(),
                                if m == hermes_crate::hitl_ui::HitlMode::Jarbas {
                                    "Jarbas"
                                } else {
                                    "Terminal HANR"
                                }
                            ),
                            Err(e) => String::from(e),
                        }
                    }
                }
                hermes::Command::ShowSkills => hermes_crate::memory_store::skills_l0(),
                hermes::Command::SkillView(ref name) => hermes_crate::memory_store::skill_view(name),
                hermes::Command::Remember(ref fact) => {
                    match hermes_crate::memory_store::remember(fact) {
                        Ok(m) => m,
                        Err(e) => alloc::format!("[MEMORY] fail: {}", e),
                    }
                }
                hermes::Command::Soul(ref text) => {
                    if text.trim().is_empty() {
                        hermes_crate::memory_store::read_soul()
                    } else {
                        match hermes_crate::memory_store::write_soul(text) {
                            Ok(()) => alloc::format!("[SOUL] Hermes orchestrator saved ({} chars)", text.len()),
                            Err(e) => alloc::format!("[SOUL] fail: {}", e),
                        }
                    }
                }
                hermes::Command::Persona(ref text) => {
                    if text.trim().is_empty() {
                        hermes_crate::memory_store::persona_slice()
                    } else {
                        match hermes_crate::memory_store::write_persona(text) {
                            Ok(()) => alloc::format!("[PERSONA] Jarbas saved ({} chars)", text.len()),
                            Err(e) => alloc::format!("[PERSONA] fail: {}", e),
                        }
                    }
                }
                hermes::Command::MemoryShow => hermes_crate::memory_store::prompt_slice(),
                hermes::Command::SessionSearch(ref q) => {
                    hermes_crate::cognitive_bridge::session_search(q, 8)
                }
                hermes::Command::Budget(ref arg) => {
                    let a = arg.trim();
                    if a.is_empty() {
                        hermes_crate::cognitive_bridge::budget_status()
                    } else if let Ok(n) = a.parse::<u16>() {
                        hermes_crate::cognitive_bridge::budget_set_max(n);
                        hermes_crate::cognitive_bridge::budget_status()
                    } else {
                        String::from("[BUDGET] use /budget or /budget <1-64>")
                    }
                }
                hermes::Command::CogStatus => hermes_crate::cognitive_bridge::status_line(),
                hermes::Command::MarketList => hermes_crate::marketplace::list_local(),
                hermes::Command::MarketSearch(ref q) => hermes_crate::marketplace::search(q),
                hermes::Command::MarketInstall(ref kind_s, ref name, ref body) => {
                    match crate::package_hub::PackageKind::from_str(kind_s) {
                        None => String::from("[MARKET] kind invalido"),
                        Some(kind) => {
                            let body_owned = if body.trim().is_empty()
                                && kind == crate::package_hub::PackageKind::Skill
                            {
                                crate::package_hub::minimal_skill_md(name, "market install")
                            } else {
                                body.clone()
                            };
                            match hermes_crate::marketplace::install_local(kind, name, &body_owned) {
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
                    match hermes_crate::marketplace::promote_draft(name) {
                        Ok(m) => m,
                        Err(e) => alloc::format!("[MARKET] {}", e),
                    }
                }
                hermes::Command::MarketRemove(ref kind_s, ref name) => {
                    match crate::package_hub::PackageKind::from_str(kind_s) {
                        None => String::from("[MARKET] kind invalido"),
                        Some(kind) => match hermes_crate::marketplace::remove(kind, name) {
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
                        Some(kind) => {
                            hermes_crate::marketplace::install_from_url(url, kind, name)
                        }
                    }
                }
                hermes::Command::MarketIndex => hermes_crate::marketplace::rebuild_index(),
                hermes::Command::Mcp(ref line) => {
                    if line.trim().is_empty() {
                        String::from("MCP: /mcp tools/list")
                    } else {
                        hermes_crate::mcp::handle_mcp_line(line)
                    }
                }
                hermes::Command::AddSkill(ref name, ref desc) => {
                    let prompt = crate::self_evolve::llm_skill_prompt(name, desc);
                    *PENDING_SKILL.lock() = Some((name.clone(), desc.clone()));
                    let _ = EVENT_BUS.publish(Event {
                        id: 0, topic: String::from(cortex::TOPIC_LLM_REQUEST),
                        payload: prompt.into_bytes(), token: CapabilityToken::Legacy(1),
                    });
                    self.state = HermesState::AwaitingLLM;
                    if self.sil.needs_create() || !self.sil.is_active() {
                        // alimenta SIL se ativo em Create
                    }
                    String::from("...")
                }
                hermes::Command::Learn(ref name, ref desc) => {
                    let instructions = alloc::format!("Skill gerada via /learn: {}", desc);
                    let md = alloc::format!(
                        "---\nschema: 1\nkind: skill\nname: {}\ndescription: {}\n\
                         contexto: \"Skill criada via comando /learn\"\n\
                         acionaveis: [\"on_demand\"]\nrequired_tokens: [1]\n\
                         provenance: hermes_created\nsandbox_status: none\n---\n\n\
                         ## Contexto\n\nSkill criada sob demanda via /learn.\n\n\
                         ## Goal\n\n{}\n\n\
                         ## Acionaveis\n\n- on_demand\n\n\
                         ## Workflow\n1. Interpretar o pedido\n2. Executar conforme descricao\n3. Verificar resultado\n\n\
                         ## Pre-Flight\n- [ ] Descricao nao vazia\n\n\
                         ## Success Criteria\n- [ ] Resultado conforme descricao\n\n\
                         ## Failure Policy\n- Reportar falha e pedir esclarecimento\n",
                        name, desc, desc
                    );
                    let skill = skill_registry::DynamicSkill::new(name, desc, &instructions);
                    SKILL_REGISTRY.lock().register(alloc::boxed::Box::new(skill));
                    let mut storage = SKILL_STORAGE.lock();
                    match crate::self_evolve::verify_and_register(&mut storage, &md) {
                        Ok(n) => {
                            hermes_crate::self_evolve::publish_change("skill", name);
                            alloc::format!("Skill '{}' aprendida+verified! Descricao: {}", n, desc)
                        }
                        Err(e) => alloc::format!("Skill '{}' rejeitada: {}", name, e),
                    }
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
                    if path.trim().is_empty() {
                        msg.push_str("[MODEL] Usage:\n");
                        msg.push_str("  /model <FAT32-8.3>              — ATA AirLLM load + set_model\n");
                        msg.push_str("  /model http://ip:port/path [DEST.GGUF]\n");
                        msg.push_str("  /model-fetch http://ip:port/path [DEST.GGUF]\n");
                        msg.push_str("[MODEL] Net uses e1000/smoltcp http_get (not SLIP). RX=0 → L3.5/RX fail.\n");
                        msg
                    } else if crate::gguf_streaming::is_http_model_spec(path) {
                        match crate::gguf_streaming::hot_swap_from_net(path) {
                            Ok(dest) => {
                                msg.push_str(&alloc::format!(
                                    "[MODEL] Net→FAT→AirLLM OK dest={} (set_model).\n",
                                    dest
                                ));
                                msg.push_str("[MODEL] PrefetchEngine = soft double-buffer (NOT DMA).\n");
                                crate::kjson!("MODEL", "SWAP", "net_airllm", "dest", dest.as_str());
                            }
                            Err(e) => {
                                msg.push_str(&alloc::format!("[MODEL] Net hot-swap FAIL: {}\n", e));
                                msg.push_str("[MODEL] Honest: if L3.5/RX, download did NOT succeed.\n");
                                crate::kjson!("MODEL", "SWAP", "net_fail", "err", e);
                            }
                        }
                        msg
                    } else if let Ok(data) = crate::fs::read_vfs(path) {
                        if !data.is_empty() {
                            if let Some(model) = crate::cortex::load_model(&data) {
                                crate::cortex::set_model(alloc::boxed::Box::new(model));
                                msg.push_str("[MODEL] Model loaded and activated (VFS).\n");
                                crate::kjson!("MODEL", "SWAP", "ok", "path", path);
                            } else {
                                msg.push_str("[MODEL] Failed to parse model file.\n");
                            }
                        } else {
                            msg.push_str("[MODEL] Empty file.\n");
                        }
                        msg
                    } else {
                        match crate::gguf_streaming::hot_swap_from_ata(path) {
                            Ok(()) => {
                                msg.push_str("[MODEL] AirLLM GGUFStreamingModel loaded (ATA local, set_model).\n");
                                msg.push_str("[MODEL] PrefetchEngine = soft double-buffer (NOT DMA).\n");
                                crate::kjson!("MODEL", "SWAP", "airllm", "path", path);
                            }
                            Err(e_air) => {
                                if let Some(model) = crate::gguf::load_gguf_model_from_disk(path) {
                                    crate::cortex::set_model(alloc::boxed::Box::new(model));
                                    msg.push_str("[MODEL] GGUF full in-RAM model loaded from disk.\n");
                                    crate::kjson!("MODEL", "SWAP", "gguf_full", "path", path);
                                } else {
                                    msg.push_str(&alloc::format!(
                                        "[MODEL] AirLLM/GGUF load failed: {}\n",
                                        e_air
                                    ));
                                    msg.push_str(&crate::gguf::print_supported_formats());
                                }
                            }
                        }
                        msg
                    }
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
                    match hermes_crate::cognitive_bridge::budget_tick() {
                        hermes_crate::cognitive_bridge::BudgetVerdict::Exhausted => {
                            String::from("[BUDGET] exhausted — /budget N para resetar max")
                        }
                        verdict => {
                            if matches!(verdict, hermes_crate::cognitive_bridge::BudgetVerdict::Grace) {
                                k_nano::slog_bin!("BUDGET", "info", "grace cycle");
                            }
                            let token_val = event.token.as_legacy();
                            let tick_now = crate::interrupts::TIMER_TICKS
                                .load(core::sync::atomic::Ordering::Relaxed) as u64;
                            let intent = self.cortex.think(msg);
                            let structured_skill = match intent {
                                cortex::Intent::Greeting | cortex::Intent::Chat => None,
                                cortex::Intent::AudioVolume => Some("audio_set_volume"),
                                _ => Some(intent.skill_name()),
                            };

                            // Uma única classificação Trinity alimenta decisão e trace R3.
                            let classified = crate::global_arena::with_arena(|arena| {
                                let trinity = TRINITY.lock();
                                let (expert, trace) =
                                    trinity.classify_intent_with_trace(msg, arena);
                                (expert.name, trinity.moe_router_loaded(), trace)
                            });
                            let (expert, moe_loaded) = if let Some((expert, loaded, trace)) = classified {
                                crate::global_arena::set_pending_route(expert, Some(trace));
                                (expert, loaded)
                            } else {
                                let trinity = TRINITY.lock();
                                let expert = trinity.classify_intent(msg).name;
                                let loaded = trinity.moe_router_loaded();
                                drop(trinity);
                                crate::global_arena::set_pending_route(expert, None);
                                (expert, loaded)
                            };
                            let route =
                                hermes_crate::cognitive_bridge::route_classified_user_intent(
                                    msg,
                                    token_val,
                                    tick_now,
                                    expert,
                                    moe_loaded,
                                    structured_skill,
                                );
                            hermes_crate::cognitive_bridge::note_route(&route);
                            k_nano::slog_bin!("ROUTE", "info", "{} — {}", route.reason, route.emotion);

                            match route.kind {
                                hermes_crate::cognitive_bridge::RouteKind::Tts => {
                                    alloc::format!(
                                        "[TTS] Falando: \"{}\" (Pocket TTS pendente — Sprint Sound)",
                                        msg
                                    )
                                }
                                hermes_crate::cognitive_bridge::RouteKind::DenyTrust => {
                                    alloc::format!("[Hermes] {}", route.reason)
                                }
                                hermes_crate::cognitive_bridge::RouteKind::EscalateLlm => {
                                    alloc::format!(
                                        "[HITL] {}\n/approve {}   ou   /deny {}",
                                        route.reason,
                                        route.approval_id.unwrap_or(0),
                                        route.approval_id.unwrap_or(0)
                                    )
                                }
                                hermes_crate::cognitive_bridge::RouteKind::ExpertSkill
                                | hermes_crate::cognitive_bridge::RouteKind::Structured => {
                                    let sk = route.skill.unwrap_or("system_status");
                                    match self.execute_skill(sk, msg.as_bytes(), &event.token) {
                                        Ok(output) => {
                                            let text = core::str::from_utf8(&output).unwrap_or("(binary)");
                                            alloc::format!(
                                                "[Route:{:?}:{}→{}] {}",
                                                route.kind, route.expert, sk, text
                                            )
                                        }
                                        Err(e) => {
                                            if hermes_crate::cognitive_bridge::llm_allowed(token_val, tick_now).is_ok() {
                                                hermes_crate::cognitive_bridge::session_record("user", msg, tick_now);
                                                *PENDING_LEARNER_INPUT.lock() = Some(String::from(msg));
                                                self.workflow_engine.start();
                                                let _ = EVENT_BUS.publish(Event {
                                                    id: 0, topic: String::from(cortex::TOPIC_LLM_REQUEST),
                                                    payload: msg.as_bytes().to_vec(),
                                                    token: CapabilityToken::Legacy(1),
                                                });
                                                self.state = HermesState::AwaitingLLM;
                                                String::from("...")
                                            } else {
                                                alloc::format!("[Trinity:{}] skill '{}' erro: {}", route.expert, sk, e)
                                            }
                                        }
                                    }
                                }
                                hermes_crate::cognitive_bridge::RouteKind::Llm => {
                                    k_nano::slog_cortex!("LLM", "info", "Enviando: \"{}\" (trinity: {})", msg, route.expert);
                                    hermes_crate::cognitive_bridge::session_record(
                                        "user", msg, tick_now,
                                    );
                                    *PENDING_LEARNER_INPUT.lock() = Some(String::from(msg));
                                    self.workflow_engine.start();
                                    let _ = EVENT_BUS.publish(Event {
                                        id: 0,
                                        topic: String::from(cortex::TOPIC_LLM_REQUEST),
                                        payload: msg.as_bytes().to_vec(),
                                        token: CapabilityToken::Legacy(1),
                                    });
                                    self.state = HermesState::AwaitingLLM;
                                    String::from("...")
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
                    k_nano::slog_bin!("CRITIQUE", "info", "{}: {}", reason, &response[..core::cmp::min(60, response.len())]);
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
                    payload: response.into_bytes(), token: CapabilityToken::Legacy(1),
                });
            } else {
                EVENT_LOG.lock().push(conversation::EventKind::UserInput, event.payload.clone(), now);
            }
            } // else !power_phrase
        }

        // Power EventBus — executa orderly (nunca retorna se shutdown/reboot)
        crate::shutdown::drain_power_requests(
            &mut self.shutdown_receiver,
            &mut self.reboot_receiver,
        );

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

/// Plataforma (PCI+ACPI+APIC[+SMP]) ja inicializada — evita double-init APIC/PIC.
pub static PLATFORM_READY: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// Init sync de plataforma — chamar de kernel_main ANTES dos drivers.
/// Reutiliza pci/acpi/apic/smp. SMP: AP_COUNT==0 → no-op (seguro WHPX).
/// PIC→APIC: init_apic desabilita PIC (STI ja ligado no Pacote A).
pub unsafe fn init_platform_sync() {
    use core::sync::atomic::Ordering;
    if PLATFORM_READY.load(Ordering::Acquire) {
        k_nano::slog_bin!("PLATFORM", "info", "ja inicializada — skip");
        return;
    }
    crate::display::fb::boot_ckpt(18, "PLATFORM pci+acpi");
    crate::pci::init_pci();
    let phys_off = crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    let acpi_info = crate::acpi::init_acpi(phys_off);
    if let Some(ref info) = acpi_info {
        crate::display::fb::boot_ckpt(19, "PLATFORM apic");
        crate::apic::init_apic(info);
        let lapic_count = info.lapic_count;
        if lapic_count > 1 {
            crate::smp::AP_COUNT.store(lapic_count - 1, Ordering::Relaxed);
        }
    } else {
        // HW-3: Fallback PIT timer quando ACPI/APIC/LAPIC timer não disponível
        k_nano::slog_bin!("PLATFORM", "warn", "ACPI not found — falling back to PIT timer");
        crate::interrupts::remap_pic_pit_fallback();
    }
    crate::display::fb::boot_ckpt(20, "PLATFORM smp");
    // Nao forca SMP: trampoline stub é BSP only (sem hang SIPI)
    // SESSÃO_260 (AIOS auto-tudo): loga hv + LAPICs do MADT + APs esperados no
    // ramlog — o dump do BOOT.LOG mostra se o wake vai acontecer e quantos.
    let hv_name = k_nano::platform_probe::hypervisor().name();
    let madt_lapics = acpi_info.as_ref().map(|i| i.lapic_count).unwrap_or(0);
    let ap_expected = crate::smp::AP_COUNT.load(Ordering::Relaxed);
    k_nano::boot_logger::log(&alloc::format!(
        "SMP: hv={} madt_lapics={} ap_expected={} allow_smp={}",
        hv_name, madt_lapics, ap_expected,
        k_nano::platform_probe::allow_smp()
    ));
    crate::smp::init_smp();
    let cores = k_nano::smp::total_cores();
    k_nano::boot_logger::log(&alloc::format!("SMP: total_cores={} apos wake", cores));
    // ADR-0061: Initialize core pinning pools after SMP (k_nano owns core_pinning + smp)
    k_nano::core_pinning::init_pools(cores);
    // STI so depois de SMP — timer IRQ nao pode reentrar serial/FB spinlock
    x86_64::instructions::interrupts::enable();
    k_nano::interrupts::calibrate_timer_hz();
    PLATFORM_READY.store(true, Ordering::Release);
    crate::display::fb::boot_ckpt(21, "PLATFORM sync OK");
    k_nano::slog_bin!("PLATFORM", "info", "sync OK (PCI+ACPI+APIC+SMP) STI=1");
}

/// PlatformAgent — PCI + ACPI + APIC + SMP init (idempotente se sync ja rodou)
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
        use core::sync::atomic::Ordering;
        // Pacote B: boot sync ja fez o trabalho — no-op
        if PLATFORM_READY.load(Ordering::Acquire) {
            return AgentTickResult::Done;
        }
        match self.phase {
            0 => {
                unsafe { crate::pci::init_pci(); }
                self.phase = 1;
                AgentTickResult::Pending
            }
            1 => {
                let phys_off = crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
                let acpi_info = unsafe { crate::acpi::init_acpi(phys_off) };
                if let Some(ref info) = acpi_info {
                    unsafe { crate::apic::init_apic(info); }
                    let lapic_count = info.lapic_count;
                    if lapic_count > 1 {
                        crate::smp::AP_COUNT.store(lapic_count - 1, Ordering::Relaxed);
                    }
                }
                self.phase = 2;
                AgentTickResult::Pending
            }
            2 => {
                unsafe { crate::smp::init_smp(); }
                PLATFORM_READY.store(true, Ordering::Release);
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
                let arch = crate::inventory::SystemArchitecture {
                    ring0_mode: 0, ring1_mode: 0, heap_size_mb: 2048,
                    trust_level: 1, power_mode: 0, tensor_tier: 0,
                };
                k_nano::slog_bin!("ARCH", "info", "System architecture: ring0={} ring1={} heap={}MB trust={} power={} tensor={}",
                    arch.ring0_mode, arch.ring1_mode, arch.heap_size_mb,
                    arch.trust_level, arch.power_mode, arch.tensor_tier);
                *crate::SYSTEM_ARCH.lock() = Some(arch);
                self.phase = 1;
                AgentTickResult::Pending
            }
            1 => {
                let mhi = crate::mhi::MemoryHierarchy::new();
                k_nano::slog_bin!("MHI", "info", "{} tier(s). Best: {:?} ({} bytes avail)",
                    mhi.tiers.len(), mhi.best_tier(), mhi.tiers[0].capacity_bytes);
                // ponytail: skip heap-allocated mhi.clone() to avoid stack overflow
                *crate::MEMORY_HIERARCHY.lock() = Some(mhi);
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
        // Pacote B: NIC ja init no boot sync → no-op (evita re-init)
        let nic_ok = crate::net::E1000.lock().is_some()
            || crate::net::I225.lock().is_some()
            || crate::net::RTL8139.lock().is_some()
            || crate::net::VIRTIO_DEV.lock().is_some();
        if nic_ok {
            k_nano::slog_hermes!("Net", "info", "NIC ja inicializada — NetDriverAgent no-op");
            return AgentTickResult::Done;
        }
        unsafe {
            if crate::net::probe_nics_from_bind_plan() {
                k_nano::slog_hermes!("Net", "info", "NIC OK (plano DeviceTree).");
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
        k_nano::slog_bin!("USB", "info", "Inicializado via init_xhci().");
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

        // ── ADR-0042 N2: Trust (token, agent, skill) + inventário VID-gated ──
        {
            let trusted = crate::TRUST_CACHE.lock().check_or_cache_agent(
                1, "self_heal", "recover", _tick, u64::MAX,
            );
            if !trusted {
                k_nano::slog_kai!("Gate", "n2", "trust DENY (token,agent,skill)=(1,self_heal,recover) — skip scan");
            } else {
                // ADR-0088: inventário = DeviceTree (H1 já rodou). Sem re-scan PCI
                // (SESSION_262: scan_pci no metal USB travava). Sem ATA ≠ árvore vazia.
                let inv = crate::inventory::HardwareInventory::from_khal();
                let triples = k_ai::boot_observe::heal_triples_from_tree();
                let fw_n = inv.fw_gated_devices().len();
                k_nano::slog_kai!(
                    "Gate",
                    "n2",
                    "inventory tree={} fw_gated={} trust=OK (sem rescan PCI)",
                    triples.len(),
                    fw_n
                );
                if fw_n == 0 {
                    k_nano::slog_kai!("Gate", "n2", "HEALTH_ISSUE: honest noop (fw_gated=0 — no known VID needs FW)");
                }
                let mut heal = crate::SELF_HEAL.lock();
                let report = heal.run_vid_gated_scan(&triples);
                let _ = crate::SYSTEM_ARCH.lock().get_or_insert_with(|| {
                    crate::inventory::SystemArchitecture::infer(&inv)
                });
                k_nano::slog_kai!(
                    "Gate",
                    "n2",
                    "gate complete heal={} noop={} HEALTH_ISSUE={} (k_ai crate N2.5)",
                    report.heal_issues,
                    report.noop,
                    report.health_published
                );
            }
        }

        // Shutdown persistente via FAT — orçado (ver boot_log_agent). Sem budget,
        // walk root sem limite bloqueava init_phase e NetAgent nunca entrava no run().
        k_nano::slog_bin!("SELF", "HEAL", "lendo shutdown cause (FAT boot-log budgeted)...");
        let last_cause = if crate::env::is_sandbox() {
            k_nano::slog_bin!("SELF", "HEAL", "sandbox: skip FAT shutdown log (evita hang PIO)");
            None
        } else {
            crate::shutdown::read_last_shutdown_from_boot_log()
        };
        match last_cause {
            Some(crate::shutdown::ShutdownCause::Unexpected) => {
                k_nano::slog_bin!("SELF", "HEAL", "*** ULTIMO DESLIGAMENTO FOI INESPERADO! ***");
                k_nano::slog_bin!("SELF", "HEAL", "Analisando boot log para possiveis erros...");
                let _ = log_analyst_agent::write_log("self_heal",
                    "Ultimo desligamento foi INESPERADO. Iniciando analise de erros.");
                // Ja lemos o log em read_last_shutdown; reusa a mesma API budgeted
                if let Some(log) = crate::boot_log_agent::BootLogAgent::read_last_boot_log() {
                    k_nano::slog_bin!("SELF-HEAL", "info", "previous boot log available: FAT32 //logs/BOOT.LOG ({} chars)", log.len());
                    let diagnostics = crate::boot_log_agent::BootLogAgent::analyze_log(&log);
                    for (kind, msg) in &diagnostics {
                        k_nano::slog_bin!("SELF", "HEAL", "Diagnostico: {} — {}", kind, msg);
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
                    k_nano::slog_bin!("SELF", "HEAL", "Boot log nao disponivel para analise.");
                }
            }
            Some(cause) => {
                k_nano::slog_bin!("SELF", "HEAL", "Ultimo desligamento: {} (ok)", crate::shutdown::label(cause));
            }
            None => {
                k_nano::slog_bin!("SELF", "HEAL", "Primeiro boot ou sem registro de desligamento.");
            }
        }
        k_nano::slog_bin!("SELF", "HEAL", "oneshot Done — liberando init_phase p/ NetAgent");

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
        // Idempotente — session key pode já ter sido gerada pré-PackageHub
        k_nano::identity::init_session_identity();
        let mut tc = crate::TRUST_CACHE.lock();
        tc.add_exempt_token(1);
        tc.load_boot_policy(&["net_", "fs_write", "exec_"]);
        // ADR-0042 N2: Trust por (token, agent, skill) para SelfHeal / inventário
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
// Usado pelos agentes do The Agency (12 divisoes, ~147 especialistas)
// Pacote B: EventDriven (nao Continuous). Sem subscribe a topico proprio —
// grace ~20 ticks no scheduler, depois dorme ate evento/wake externo.
// ---------------------------------------------------------------------------

pub struct SpecialistAgent {
    manifest: AgentManifest,
    spec: crate::agency::AgentSpec,
    announced: bool,
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
            // Activation-on-Demand: Agency → EventDriven (nao Continuous)
            manifest: AgentManifest { name, kind, schedule: ScheduleKind::EventDriven, auto_start: true, persist: true },
            spec,
            announced: false,
        }
    }
}

impl Agent for SpecialistAgent {
    fn manifest(&self) -> &AgentManifest { &self.manifest }
    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        // Anuncia skills uma vez; Pending para grace do EventDriven, depois dorme
        if self.announced {
            return AgentTickResult::Pending;
        }
        let topic = alloc::format!("AGENCY_{}", self.spec.name.to_ascii_uppercase());
        let _ = EVENT_BUS.publish(Event {
            id: 0, topic, payload: self.spec.skills.join(",").into_bytes(),
            token: CapabilityToken::Legacy(1),
        });
        self.announced = true;
        AgentTickResult::Pending
    }
}

/// Registra SpecialistAgents Agency **somente** se PackageHub tiver AGENT.md
/// agency assinados (ADR-0052). Seed compilado vazio — stubs não entram no fleet.
/// Fallback: se specs vazio, registra 2 AgentSpecs mínimos (SystemDiagnostics, HwMonitor).
pub fn register_agency_agents(registry: &mut agent_core::AgentRegistry) {
    let specs = crate::package_hub::PACKAGE_HUB.lock().agency_specs();
    if specs.is_empty() {
        // ponytail: 2 specs fallback — boot log mostra >0 agentes sem AGENT.md assinado.
        let fallback = vec![
            crate::agency::AgentSpec {
                name: String::from("SystemDiagnostics"),
                division: String::from("qa"),
                mission: String::from("Diagnóstico de saúde do kernel e invariantes"),
                skills: vec![String::from("diagnostic"), String::from("health")],
                deliverable: String::from("auto"),
            },
            crate::agency::AgentSpec {
                name: String::from("HwMonitor"),
                division: String::from("infrastructure"),
                mission: String::from("Monitora hardware detectado e publica estado"),
                skills: vec![String::from("hw"), String::from("monitor")],
                deliverable: String::from("auto"),
            },
        ];
        for spec in &fallback {
            let agent = SpecialistAgent::new(spec.clone());
            registry.register(Box::new(agent));
        }
        k_nano::slog_bin!("AGENCY", "info", "2 agentes fallback registrados (ADR-0052: sem AGENT.md agency assinado)");
        return;
    }
    let agency = crate::agency::Agency::from_specs(specs);
    for div in &agency.divisions {
        for spec in &div.agents {
            let agent = SpecialistAgent::new(spec.clone());
            registry.register(Box::new(agent));
        }
    }
    let count: usize = agency.divisions.iter().map(|d| d.agents.len()).sum();
    k_nano::slog_bin!("AGENCY", "info", "{} agentes registrados via PackageHub AGENT.md assinado", count);
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
    k_nano::slog_bin!("HW", "AGENTS", "{} agentes de hardware registrados", hw.agents.len());
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
        // Plug-and-play (truth path do bin): PCI → HwCapabilityCard → EventBus.
        // Sem generate() free-text do HW Expert v3 (OA5US…).
        let mut hw = crate::hw_agents::HwRegistry::new();
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

            if (agent.class == 0x02 || agent.class == 0x0D) && card.ring_size == 0 {
                if let Some(map) = crate::cortex::generate_register_map(vid, did) {
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

            k_nano::slog_bin!("Log", "msg", "{}", card.log_line());

            let wire = card.to_wire();
            let _ = EVENT_BUS.publish(Event {
                id: 0,
                topic: String::from(k_ai::hw_capability::TOPIC_HW_CAPABILITY),
                payload: wire.as_bytes().to_vec(),
                token: CapabilityToken::Legacy(1),
            });
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

            dispatch_pnp_action_nk(&card);
            cards_n = cards_n.saturating_add(1);
        }

        k_nano::slog_bin!("HW", "PnP", "published {} capability cards", cards_n);
        k_nano::slog_bin!("HW", "AI", "Arvore PnP:\n{}", device_tree);

        // Hermes consome HW_CAPABILITY e decide (agentico). Não dump free-text → LLM aqui.
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

fn dispatch_pnp_action_nk(card: &k_ai::hw_capability::HwCapabilityCard) {
    use k_ai::hw_capability::HwNextAction;
    match card.next_action {
        HwNextAction::BindNetwork | HwNextAction::Ready => {
            k_nano::slog_bin!("HW", "PnP", "READY {:04X}:{:04X} → agent={} (já no boot path)", card.vid, card.did, card.agent);
        }
        HwNextAction::LoadFirmware => {
            k_nano::slog_bin!("HW", "PnP", "NEED_FW {:04X}:{:04X} fw={} → SelfHeal/HEALTH_ISSUE path", card.vid, card.did, card.firmware.unwrap_or("?"));
            let msg = alloc::format!(
                "HEALTH_ISSUE:I3:{:04X}:{:04X}:firmware_hint:{}",
                card.vid, card.did, card.firmware.unwrap_or("-")
            );
            let _ = EVENT_BUS.publish(Event {
                id: 0,
                topic: String::from("HEALTH_ISSUE"),
                payload: msg.into_bytes(),
                token: CapabilityToken::Legacy(1),
            });
        }
        HwNextAction::BindWifiScan => {
            k_nano::slog_bin!("HW", "PnP", "WIFI_SCAN {:04X}:{:04X} → WifiAgent", card.vid, card.did);
            let _ = EVENT_BUS.publish(Event {
                id: 0,
                topic: String::from("NET_IFACE_AVAILABLE"),
                payload: alloc::format!("wifi:{:04X}:{:04X}", card.vid, card.did).into_bytes(),
                token: CapabilityToken::Legacy(1),
            });
        }
        HwNextAction::BindUsbHost => {
            k_nano::slog_bin!("HW", "PnP", "USB_HOST {:04X}:{:04X} → xHCI/UVC/UAC hot-enum", card.vid, card.did);
        }
        HwNextAction::BindGpuCompute => {
            k_nano::slog_bin!("HW", "PnP", "GPU {:04X}:{:04X} fw={} → GpuBackend", card.vid, card.did, card.firmware.unwrap_or("-"));
        }
        HwNextAction::BindAudio => {
            k_nano::slog_bin!("HW", "PnP", "AUDIO {:04X}:{:04X} → HdaAudioAgent", card.vid, card.did);
        }
        HwNextAction::BindStorage => {
            k_nano::slog_bin!("HW", "PnP", "STORAGE {:04X}:{:04X} → DiskAgent", card.vid, card.did);
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
        k_nano::slog_bin!("TRINITY", "Learn", "Iniciando aprendizado: {}...", topic);
        for need in &mut self.needs {
            if need.topic == topic { need.triggered = true; }
        }

        // Carrega conhecimento da FAT32 e faz fine-tuning on-device via BitNetTrainer
        let knowledge = self.load_knowledge(topic);
        if knowledge.is_empty() {
            k_nano::slog_bin!("TRINITY", "Learn", "{}: conhecimento indisponivel em FAT32", topic);
            k_nano::slog_bin!("TRINITY", "Learn", "Coloque {}.BIN na FAT32 ou gere via SDIO pipeline", topic.to_uppercase());
            return;
        }

        k_nano::slog_bin!("TRINITY", "Learn", "{}: {} bytes carregados. R3 replay com rotas congeladas...", topic, knowledge.len());

        // R3: atualiza router com logits congelados (sem re-rotear / sem dummy train_step)
        let mut traces = [crate::r3::RouteTrace {
            embedding_addr: 0,
            logits_addr: 0,
            num_experts: 0,
            selected_expert: 0,
            old_log_prob: 0.0,
            token_ids_addr: 0,
            token_count: 0,
        }; 64];
        let n = crate::global_arena::snapshot_route_traces(&mut traces);
        let mut weights = alloc::vec![0i8; 64 * 6];
        let mut loss = 0.0f32;
        let mut steps = 0u32;
        if n > 0 {
            let trinity = TRINITY.lock();
            for t in traces.iter().take(n) {
                // reward heurístico: conhecimento carregado → reforço positivo
                loss += crate::r3::update_with_replay(&trinity, t, 1.0, &mut weights, 0.05, 0.0);
                steps += 1;
            }
            drop(trinity);
            let mut trainer = crate::BITNET_TRAINER.lock();
            trainer.trained += steps as u64;
            drop(trainer);
            k_nano::slog_bin!("TRINITY", "Learn", "{}: R3 replay {} traces loss={:.4} (arena tokens={})",
                topic,
                n,
                loss,
                crate::global_arena::token_steps());
        } else {
            // Sem traces ainda — bootstrap R3 (não re-calcula rotas via train_step dummy)
            let mut dummy_trace = crate::r3::RouteTrace {
                embedding_addr: 0,
                logits_addr: 0,
                num_experts: 6,
                selected_expert: 0,
                old_log_prob: libm::logf(0.2),
                token_ids_addr: 0,
                token_count: 0,
            };
            let trinity = TRINITY.lock();
            loss = crate::r3::update_with_replay(&trinity, &dummy_trace, 0.5, &mut weights, 0.01, 0.0);
            drop(trinity);
            let _ = &mut dummy_trace;
            let mut trainer = crate::BITNET_TRAINER.lock();
            trainer.trained += 1;
            k_nano::slog_bin!("TRINITY", "Learn", "{}: bootstrap R3 (sem cache) loss={:.4} steps={}",
                topic,
                loss,
                trainer.trained);
        }
        crate::global_arena::reset_moe_cache();
        k_nano::slog_bin!("TRINITY", "Learn", "{}: TRINITY APRENDEU! (R3 reset O(1))", topic);
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
                            k_nano::slog_bin!("TRINITY", "Learn", "{} carregado via FAT32: {} bytes", fname, data.len());
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
        // Prefer host HTTP via QEMU gateway (tools serve); fallback DNS hostname.
        let url_gw = alloc::format!("http://10.0.2.2:8080/{}.BIN", topic);
        let url_dns = alloc::format!("http://repository.neuralos.local/{}.BIN", topic);
        k_nano::slog_bin!("TRINITY", "Learn", "Tentando download: {}", url_gw);
        let _ = crate::EVENT_BUS.publish(Event {
            id: 0,
            topic: alloc::string::String::from(crate::browser_agent::TOPIC_FETCH_REQUEST),
            payload: url_gw.as_bytes().to_vec(),
            token: CapabilityToken::Legacy(1),
        });
        match crate::net::resolve_and_http_get_safe(&url_gw) {
            Ok(data) if !data.is_empty() => {
                k_nano::slog_bin!("TRINITY", "Learn", "download OK {} bytes via {}", data.len(), url_gw);
                return Some(data);
            }
            Ok(_) | Err(_) => {}
        }
        match crate::net::resolve_and_http_get_safe(&url_dns) {
            Ok(data) if !data.is_empty() => {
                k_nano::slog_bin!("TRINITY", "Learn", "download OK {} bytes via {}", data.len(), url_dns);
                Some(data)
            }
            Err(e) => {
                k_nano::slog_bin!(
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
// SleepCycleAgent — canônico em hermes (emagreçer: 1 impl + bin pub use)
// ---------------------------------------------------------------------------
pub use hermes_crate::agents::SleepCycleAgent;

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

pub struct FsBridgeAgent;

impl FsBridgeAgent {
    pub fn new() -> Self { FsBridgeAgent }

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
                        k_nano::slog_bin!("FS", "BRIDGE", "Migrado {:?} → DRAM ({} bytes, idle={})", phys, size, _tick.saturating_sub(*last_access));
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
        // ponytail: scheduler já garante PollEvery(500), sem rate-limiting interno
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
                k_nano::slog_jarbas!("VGPU", "info", "VirtIO-GPU OK.");
            }
        }
        // A-015 honesto (SESSION_274): detect/canário rodaram no DriverInit
        // (k_hal); o agente reporta a postura REAL do backend, não só VirtIO.
        k_nano::slog_jarbas!("GPU", "info", "backend: {} | {}",
            crate::gpu::backend::gpu_status(),
            crate::gpu::vram::vram_status());
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

        k_nano::slog_bin!("Log", "msg", "{}", report);
        crate::println!("{}", report);
        Ok(report.into_bytes())
    }
}

// ---------------------------------------------------------------------------
// SelfEvolveAgent — Sprint 108: observe→generate→verify→improve→reflect
// ---------------------------------------------------------------------------

const SELF_EVOLVE_MANIFEST: AgentManifest = AgentManifest {
    name: "self_evolve",
    kind: AgentKind::System,
    schedule: ScheduleKind::PollEvery(100),
    auto_start: true,
    persist: true,
};

pub struct SelfEvolveAgent {
    receiver: Receiver,
    change_receiver: Receiver,
    last_reflect: u64,
    cycles: u64,
}

impl SelfEvolveAgent {
    pub fn new() -> Self {
        SelfEvolveAgent {
            receiver: EVENT_BUS.subscribe(crate::self_evolve::TOPIC_SELF_EVOLVE),
            change_receiver: EVENT_BUS.subscribe(hermes_crate::self_evolve::TOPIC_CHANGE),
            last_reflect: 0,
            cycles: 0,
        }
    }
}

impl Agent for SelfEvolveAgent {
    fn manifest(&self) -> &AgentManifest { &SELF_EVOLVE_MANIFEST }

    fn tick(&mut self, tick: u64, _count: u64) -> AgentTickResult {
        // Consome sinais (cron review / sleep reflect)
        while let Some(ev) = self.receiver.try_receive() {
            let msg = core::str::from_utf8(&ev.payload).unwrap_or("");
            k_nano::slog_bin!("S108", "info", "event: {}", msg);
            if msg == "skill_review" || msg.starts_with("intents=") {
                let mut storage = SKILL_STORAGE.lock();
                let n = crate::self_evolve::tick_cycle(&mut storage, tick);
                drop(storage);
                if n > 0 {
                    k_nano::slog_bin!("S108", "info", "tick_cycle registered/improved={}", n);
                }
            }
        }

        // Consome CHANGE_NOTIFY (skills/evolve): reindexa p/ o próximo system prompt
        while let Some(ev) = self.change_receiver.try_receive() {
            k_nano::slog_bin!("CHANGE", "info", "{}", core::str::from_utf8(&ev.payload).unwrap_or("?"));
            hermes_crate::skill_loader::invalidate_skill_index();
        }

        // Ciclo periódico: generate + improve
        self.cycles = self.cycles.wrapping_add(1);
        if self.cycles % 5 == 0 {
            let mut storage = SKILL_STORAGE.lock();
            let n = crate::self_evolve::tick_cycle(&mut storage, tick);
            drop(storage);
            if n > 0 {
                k_nano::slog_bin!("S108", "info", "periodic work={}", n);
            }
        }

        // Reflect a cada ~2000 ticks
        if tick.saturating_sub(self.last_reflect) >= 2000 {
            self.last_reflect = tick;
            let detail = crate::self_evolve::reflect(tick);
            let _ = EVENT_BUS.publish(Event {
                id: 0,
                topic: String::from(hermes::TOPIC_HERMES_RESPONSE),
                payload: alloc::format!("[S108-REFLECT] {}", detail).into_bytes(),
                token: CapabilityToken::Legacy(1),
            });
        }

        if tick > 0 && tick % 5000 == 0 {
            k_nano::slog_bin!("Log", "msg", "{}", crate::self_evolve::status_line());
        }

        AgentTickResult::Pending
    }
}

