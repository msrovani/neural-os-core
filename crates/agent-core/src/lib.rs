#![cfg_attr(not(test), no_std)]
#![allow(dead_code)]
extern crate alloc;

pub mod budget;
pub mod crew;
pub mod hooks;
pub mod state_graph;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::collections::BTreeMap;
use budget::{AgentWatchdogState, BudgetManager};
use core::sync::atomic::AtomicPtr;
use core::ptr::null_mut;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AgentTier {
    Permanent,
    User,
}

impl AgentTier {
    pub fn priority(&self) -> u8 {
        match self {
            AgentTier::Permanent => 0,
            AgentTier::User => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AgentKind {
    System,
    Driver,
    Inference,
    Router,
    Console,
    Network,
    Skill,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScheduleKind {
    Oneshot,
    Continuous,
    PollEvery(u64),
    EventDriven,
}

/// FlowTrigger define quando um agente acorda.
/// Substitui ScheduleKind puro por eventos semanticos.
#[derive(Clone, Debug, PartialEq)]
pub enum FlowTrigger {
    /// Agenda tradicional (compatibilidade)
    Schedule(ScheduleKind),
    /// Acorda no boot (equivalente a Schedule(Oneshot) + auto_start)
    Start,
    /// Acorda quando um topico do EventBus tem mensagem
    Listen(&'static str),
    /// Acorda, le o payload do topico, roteia para handler baseado no conteudo
    Router(&'static str),
}

/// Determina se um agente com FlowTrigger deve ser pollado neste tick
fn should_poll_flow(flow: &FlowTrigger, tick: u64, last_poll: u64, has_event: bool) -> bool {
    match flow {
        FlowTrigger::Schedule(sched) => match sched {
            ScheduleKind::Continuous => true,
            ScheduleKind::PollEvery(n) => last_poll == 0 || tick - last_poll >= *n,
            ScheduleKind::Oneshot => true,
            ScheduleKind::EventDriven => has_event,
        },
        FlowTrigger::Start => last_poll == 0,
        FlowTrigger::Listen(_) => has_event,
        FlowTrigger::Router(_) => has_event,
    }
}

/// Watchdog: só considera "runaway" agente sem urgency (não-interativo).
/// Interativos (urgency>0) retornam Pending por design e pollam todo tick —
/// crashear por "nunca Done" os mataria sem recuperação. Espelha a isenção do
/// rate-limit (run(): urgency == 0 && consecutive > 50).
fn watchdog_should_crash(urgency: u8, consecutive_pending: u64) -> bool {
    urgency == 0 && consecutive_pending > 10000
}

/// EventDriven: decide se o agente deve ser pollado neste tick.
/// Polla no 1º tick (last_poll == 0 — cobre agents que anunciam uma vez,
/// ex.: SpecialistAgent) ou quando has_pending() sinaliza trabalho.
/// NUNCA deriva de consecutive_pending (self-referential): isso fazia agents
/// dormirem para sempre após 20 Pending, sem caminho de wake.
fn event_driven_has_event(last_poll: u64, has_pending: bool) -> bool {
    last_poll == 0 || has_pending
}

#[derive(Clone, Debug)]
pub struct AgentManifest {
    pub name: &'static str,
    pub kind: AgentKind,
    pub schedule: ScheduleKind,
    pub auto_start: bool,
    pub persist: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AgentTickResult {
    Pending,
    Done,
    Crashed,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AgentState {
    Inactive,
    Active,
    Done,
    Crashed,
}

pub trait Agent: Send {
    fn manifest(&self) -> &AgentManifest;
    fn tick(&mut self, tick: u64, tick_count: u64) -> AgentTickResult;
    fn on_activate(&mut self) {}
    fn on_deactivate(&mut self) {}
    /// EventDriven: há trabalho pendente? (ex.: receiver do EventBus com
    /// mensagem). Default false — agents sem trabalho assíncrono ficam em
    /// dormência após o 1º tick (last_poll == 0).
    fn has_pending(&self) -> bool { false }
}

/// Extensao opcional do AgentManifest para suporte a Crew/Flow.
/// Nao modifica AgentManifest original (evita quebrar 24+ const definitions).
#[derive(Clone, Debug)]
pub struct CrewManifest {
    pub role: &'static str,
    pub goal: &'static str,
    pub backstory: &'static str,
    pub flow: FlowTrigger,
    pub crew_id: Option<u16>,
}

impl CrewManifest {
    pub const fn empty() -> Self {
        CrewManifest {
            role: "", goal: "", backstory: "",
            flow: FlowTrigger::Schedule(ScheduleKind::Continuous),
            crew_id: None,
        }
    }
}

pub struct AgentInstance {
    pub agent: Box<dyn Agent>,
    pub state: AgentState,
    pub last_poll: u64,
    pub tick_counter: u64,
    /// Cached at register - avoid dyn manifest() in scheduler hot path.
    pub name: &'static str,
    pub auto_start: bool,
    pub schedule: ScheduleKind,
    pub consecutive_pending: u64,  // watchdog: ticks consecutivos sem Done
    pub crew: CrewManifest,
    pub tier: AgentTier,
    /// ADR-0055: pool ring 0=BSP/critical, 1=compute, 2=event/WASM
    pub affinity_ring: u8,
    // ─── Goal-aware scheduling (ADR-0076 Onda 4.3) ───
    /// VayuOS: goal urgency 0-255 (255=critical, must-run-now).
    pub goal_urgency: u8,
    /// RuVix: novelty score 0-255 (decays 1/tick, boosted on new events).
    pub novelty_score: u8,
    /// RuVix: coherence pressure — agents that comm together stay together.
    pub coherence_partner: Option<usize>,  // agent index to schedule near
    // ─── Budget watchdog (ADR-0078) ───
    /// Ticks spent in Paused state; auto-recover at 1000, crash at 10000.
    pub paused_ticks: u64,
}

impl AgentInstance {
    pub fn new(agent: Box<dyn Agent>) -> Self {
        // Copia campos Copy/&'static ANTES de mover o Box — evita borrow
        // atravessando o move e garante cache estável p/ o hot path.
        let name = agent.manifest().name;
        let auto_start = agent.manifest().auto_start;
        let schedule = agent.manifest().schedule;
        let affinity_ring = match schedule {
            ScheduleKind::Continuous => 0,
            ScheduleKind::Oneshot => 0,
            ScheduleKind::PollEvery(_) => 1,
            ScheduleKind::EventDriven => 2,
        };
        AgentInstance {
            agent,
            state: AgentState::Inactive,
            last_poll: 0,
            tick_counter: 0,
            name,
            auto_start,
            schedule,
            consecutive_pending: 0,
            // Espelha schedule no flow — PollEvery/EventDriven funcionam de fato.
            crew: CrewManifest {
                role: "",
                goal: "",
                backstory: "",
                flow: FlowTrigger::Schedule(schedule),
                crew_id: None,
            },
            tier: AgentTier::Permanent,
            affinity_ring,
            goal_urgency: 0,
            novelty_score: 0,
            coherence_partner: None,
            paused_ticks: 0,
        }
    }
}

pub struct AgentRegistry {
    pub agents: Vec<AgentInstance>,
    pub skill_map: BTreeMap<String, usize>,
    pub budget_manager: BudgetManager,
    /// HookRegistry — PreTick/PostTick/OnCrash/OnSpawn hooks.
    pub hooks: hooks::HookRegistry,
    /// SESSÃO_260: trace opcional do init_phase (fn pointer, zero-dep) —
    /// chamado ANTES de cada tick de um Oneshot no init_phase. O HW real
    /// travou no K51 (init_phase start) sem K52; o bin seta isso para logar
    /// no ramlog e revelar o agente do freeze.
    pub init_trace: Option<fn(&str, u64)>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        AgentRegistry {
            agents: Vec::new(),
            skill_map: BTreeMap::new(),
            budget_manager: BudgetManager::new(),
            hooks: hooks::HookRegistry::new(),
            init_trace: None,
        }
    }

    pub fn register(&mut self, agent: Box<dyn Agent>) -> usize {
        let name = agent.manifest().name;
        let auto_start = agent.manifest().auto_start;
        let idx = self.agents.len();
        let mut instance = AgentInstance::new(agent);
        // Re-stamp after move into instance (matrix QA: cached name was empty in run()).
        instance.name = name;
        instance.auto_start = auto_start;
        self.agents.push(instance);
        self.budget_manager.register(name, None);
        idx
    }

    pub fn activate(&mut self, idx: usize) {
        if idx < self.agents.len() {
            self.agents[idx].state = AgentState::Active;
            self.agents[idx].agent.on_activate();
        }
    }

    /// Override tick budget for an agent (default: 100).
    /// Register a hook callback.
    pub fn register_hook(&mut self, hook: hooks::Hook) {
        self.hooks.register(hook);
    }

    /// Access the hook registry (for direct manipulation).
    pub fn hook_registry(&mut self) -> &mut hooks::HookRegistry {
        &mut self.hooks
    }

    pub fn set_budget(&mut self, name: &str, max_ticks_per_call: u64) {
        self.budget_manager.register(name, Some(max_ticks_per_call));
    }

    pub fn get(&self, name: &str) -> Option<&AgentInstance> {
        self.agents.iter().find(|a| a.name == name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut AgentInstance> {
        self.agents.iter_mut().find(|a| a.name == name)
    }

    /// ADR-0055/0089: override affinity ring (0=BSP-only, 1=Compute, 2=Worker/event).
    pub fn set_affinity_ring(&mut self, name: &str, ring: u8) -> bool {
        match self.get_mut(name) {
            Some(a) => {
                a.affinity_ring = ring;
                true
            }
            None => false,
        }
    }

    /// Goal-aware: agents com urgency > 0 NÃO são rate-limited pelo scheduler
    /// (rate-limit só atinge `urgency == 0 && consecutive_pending > 50`).
    /// Agentes interativos (input, hw_bridge, net, mouse) devem marcar urgency
    /// alto — senão o rate-limit os mata de fome e input/rede morrem (~50 ticks).
    pub fn set_urgency(&mut self, name: &str, urgency: u8) -> bool {
        match self.get_mut(name) {
            Some(a) => {
                a.goal_urgency = urgency;
                true
            }
            None => false,
        }
    }

    pub fn active_count(&self) -> usize {
        self.agents.iter().filter(|a| a.state == AgentState::Active).count()
    }

    /// ADR-0055: índices por affinity_ring (0=BSP/critical, 1=compute, 2=event).
    pub fn agents_by_affinity_ring(&self, ring: u8) -> Vec<usize> {
        self.agents
            .iter()
            .enumerate()
            .filter(|(_, a)| a.affinity_ring == ring)
            .map(|(i, _)| i)
            .collect()
    }

    /// Ordem de poll: ring0 → ring1 → ring2 (CorePools R0/R1/R2).
    /// Dentro de cada ring, ordena por goal_urgency + novelty_score (goal-aware).
    /// VayuOS + RuVix pattern: agentes com objetivo urgente ou novidade alta pollam primeiro.
    pub fn poll_order_by_affinity(&self) -> Vec<usize> {
        let mut order = Vec::with_capacity(self.agents.len());
        for ring in 0u8..3 {
            let mut ring_agents: Vec<usize> = self.agents_by_affinity_ring(ring);
            // Goal-aware sort: urgency (weight 2) + novelty (weight 1)
            ring_agents.sort_by(|&a, &b| {
                let score_a = (self.agents[a].goal_urgency as u16) * 2
                    + self.agents[a].novelty_score as u16;
                let score_b = (self.agents[b].goal_urgency as u16) * 2
                    + self.agents[b].novelty_score as u16;
                score_b.cmp(&score_a) // descending: higher score = earlier poll
            });
            order.extend(ring_agents);
        }
        // Qualquer ring fora 0..2
        for (i, a) in self.agents.iter().enumerate() {
            if a.affinity_ring > 2 {
                order.push(i);
            }
        }
        order
    }
}

impl AgentRegistry {
    /// Budget watchdog check before calling agent.tick().
    /// Returns `true` if tick should proceed, `false` if agent is paused/crashed.
    fn check_budget(&mut self, idx: usize) -> bool {
        let name = self.agents[idx].name;

        // If already Paused: track pause time, maybe auto-recover, skip tick
        if self.agents[idx].paused_ticks > 0 {
            self.agents[idx].paused_ticks += 1;
            if self.agents[idx].paused_ticks >= 1000 {
                // Auto-recover after 1000 ticks in Paused
                self.budget_manager.recover(name);
                self.agents[idx].paused_ticks = 0;
                maybe_log_budget(name, "recovered");
            } else if self.agents[idx].paused_ticks >= 10000 {
                // Safety net: too long paused → crash
                self.agents[idx].state = AgentState::Crashed;
                maybe_log_budget(name, "crashed_timeout");
                return false;
            }
            return false; // skip tick while paused
        }

        // Consume 1 tick from budget
        if !self.budget_manager.consume(name, 1) {
            let wd = self.budget_manager
                .get_state(name)
                .unwrap_or(AgentWatchdogState::Normal);
            match wd {
                AgentWatchdogState::Paused | AgentWatchdogState::Crashed => {
                    // First tick entering Paused — mark it and skip
                    self.agents[idx].paused_ticks = 1;
                    maybe_log_budget(name, "paused");
                    return false;
                }
                AgentWatchdogState::Warning => {
                    // Over budget but still Warning — log it but allow tick
                    maybe_log_budget(name, "budget_warning");
                }
                _ => {}
            }
        }
        true
    }

    /// Boot Oneshot round-robin até todos Done **ou** timeout.
    /// NÃO processa agentes um-a-um até Done (hang se A espera evento de B).
    /// Pendentes após timeout ficam Active para o `run()` — impossível hangar o boot.
    pub fn init_phase(&mut self) {
        const MAX_ROUNDS: u64 = 10_000;
        for i in 0..self.agents.len() {
            if self.agents[i].schedule != ScheduleKind::Oneshot { continue; }
            if !self.agents[i].auto_start { continue; }
            if self.agents[i].state == AgentState::Done || self.agents[i].state == AgentState::Crashed {
                continue;
            }
            self.agents[i].state = AgentState::Active;
            self.agents[i].agent.on_activate();
        }
        let mut round = 0u64;
        loop {
            round += 1;
            let mut any_active = false;
            let n = self.agents.len();
            for i in 0..n {
                // Defesa: tick profundo (self_heal) já corrompeu Vec via stack smash
                // quando registry ficava abaixo do sched stack no bump — aborta honesto.
                if i >= self.agents.len() {
                    break;
                }
                if self.agents[i].schedule != ScheduleKind::Oneshot { continue; }
                if self.agents[i].state != AgentState::Active { continue; }
                any_active = true;
                self.agents[i].tick_counter += 1;
                let tc = self.agents[i].tick_counter;
                if let Some(trace) = self.init_trace {
                    trace(self.agents[i].name, round);
                }
                let result = self.agents[i].agent.tick(round, tc);
                if i >= self.agents.len() {
                    // Registry corrompido sob o tick — não indexar.
                    break;
                }
                match result {
                    AgentTickResult::Done => self.agents[i].state = AgentState::Done,
                    AgentTickResult::Crashed => self.agents[i].state = AgentState::Crashed,
                    AgentTickResult::Pending => {}
                }
            }
            if !any_active { break; }
            if round >= MAX_ROUNDS { break; }
        }
    }

    /// Scheduler loop — called from kernel
    /// `halt()`: called when no agent needs CPU (platform-specific hlt)
    /// `check_respawns(): returns names of agents to re-create (e.g., from RESPAWN_QUEUE)`
    /// `spawn_agent(name): creates a new Agent by name`
    #[allow(unreachable_code, unreachable_patterns)]
    pub fn run<H: Fn(), C: FnMut() -> Vec<String>, S: Fn(&str) -> Option<Box<dyn Agent>>>(
        &mut self, halt: H, mut check_respawns: C, spawn_agent: S,
    ) -> ! {
        if let Some(trace) = self.init_trace { trace(">> ENTER run", 0); }
        for i in 0..self.agents.len() {
            if self.agents[i].state != AgentState::Inactive {
                continue;
            }
            let a_name = self.agents[i].name;
            if let Some(trace) = self.init_trace { trace(a_name, i as u64); }
            if self.agents[i].auto_start {
                self.agents[i].state = AgentState::Active;
                self.agents[i].agent.on_activate();
            }
        }
        if let Some(trace) = self.init_trace { trace(">> ACT_DONE", 0); }
        // BudgetManager vive dentro do AgentRegistry heap-pinned (bin: Box::leak
        // antes do switch de RSP). Ponteiro global válido pelo noreturn do run().
        set_budget_stats_ref(&mut self.budget_manager);

        let mut tick_id: u64 = 0;
        loop {
            tick_id += 1;
            if let Some(trace) = self.init_trace { trace(">> TICK", tick_id); }
            // Budget por ciclo: reset a cada tick do scheduler. ANTES nunca era
            // chamado (reset_all sem callers) — ticks_used acumulava para sempre
            // e apos ~103 polls todos os agentes Continuous viravam Paused →
            // polled=0 → input/rede/OTA paravam (bug real de HW + QEMU).
            self.budget_manager.reset_all();
            // Check for respawn requests before polling agents
            let respawns = check_respawns();
            for name in &respawns {
                if let Some(agent) = spawn_agent(name) {
                    let idx = self.agents.len();
                    self.agents.push(AgentInstance::new(agent));
                    self.budget_manager.register(name, None);
                    self.agents[idx].state = AgentState::Active;
                    self.agents[idx].agent.on_activate();
                    // OnSpawn hook: notify registrados
                    self.hooks.run(hooks::HookType::OnSpawn, name, tick_id);
                }
            }

            // ADR-0089: offload ring≥1 → per-CPU run-queues quando APs vivos.
            let smp_offload = unsafe { SMP_OFFLOAD_PREDICATE.map(|p| p()).unwrap_or(false) };
            if smp_offload {
                if let Some(dist) = unsafe { SMP_DISTRIBUTE } {
                    let mut batch: Vec<(u32, u8, u8, Option<usize>, u8)> = Vec::new();
                    for (i, a) in self.agents.iter().enumerate() {
                        if a.state != AgentState::Active || a.affinity_ring == 0 {
                            continue;
                        }
                        batch.push((
                            i as u32,
                            a.affinity_ring,
                            a.tier.priority(),
                            a.coherence_partner,
                            a.goal_urgency,
                        ));
                    }
                    if !batch.is_empty() {
                        let _ = dist(&batch, tick_id as u32);
                    }
                }
            }

            let mut polled: u32 = 0;
            // Scheduler por affinity ring R0→R1→R2 (ADR-0055) + FlowTrigger
            let order = self.poll_order_by_affinity();
            for &i in &order {
                let state = self.agents[i].state;
                if state != AgentState::Active {
                    continue;
                }
                // Offload: BSP não ticka ring≥1 — APs consomem a RQ.
                if smp_offload && self.agents[i].affinity_ring >= 1 {
                    continue;
                }
                let flow = &self.agents[i].crew.flow;
                // Rate-limiting: passive agents (Pending >50x consec) skipped 80% of ticks
                // Goal-aware: agents with urgency > 0 are NOT rate-limited
                let consecutive = self.agents[i].consecutive_pending;
                let urgency = self.agents[i].goal_urgency;
                if urgency == 0 && consecutive > 50 && tick_id % 5 != 0 {
                    continue;
                }
                let schedule = self.agents[i].schedule;
                // EventDriven: polla no 1º tick ou quando o agente sinaliza
                // trabalho pendente (has_pending) — nunca por consecutive_pending
                // (self-referential: dormência eterna após 20 Pending, sem wake).
                let has_event = schedule != ScheduleKind::EventDriven
                    || event_driven_has_event(
                        self.agents[i].last_poll,
                        self.agents[i].agent.has_pending(),
                    );
                let should_poll = should_poll_flow(flow, tick_id, self.agents[i].last_poll, has_event);
                if !should_poll {
                    continue;
                }
                self.agents[i].last_poll = tick_id;

                // Budget watchdog guard — skip tick if budget exhausted
                if !self.check_budget(i) {
                    continue;
                }

                // PreTick hook: block agent execution if any hook returns Block
                let agent_name = self.agents[i].name;
                if !self.hooks.check(hooks::HookType::PreTick, agent_name, tick_id) {
                    continue;
                }

                // Watchdog de tick lento (freeze metal): mede ms em torno do
                // tick. Hooks não registrados = custo zero (branch previsível).
                let wdt_clock = unsafe { TICK_CLOCK_HOOK };
                let wdt_slow = unsafe { SLOW_TICK_HOOK };
                let wdt_t0 = wdt_clock.map(|c| c()).unwrap_or(0);
                // Stamp p/ HUD ao vivo: tick em curso + agente (freeze s317).
                if wdt_clock.is_some() {
                    TICK_ENTERED_MS.store(wdt_t0, core::sync::atomic::Ordering::Relaxed);
                    CUR_AGENT_PTR.store(
                        agent_name.as_ptr() as u64,
                        core::sync::atomic::Ordering::Relaxed,
                    );
                    CUR_AGENT_LEN.store(agent_name.len(), core::sync::atomic::Ordering::Relaxed);
                }
                let result = with_agent_tick_lock(|| {
                    self.agents[i].tick_counter += 1;
                    let tc = self.agents[i].tick_counter;
                    self.agents[i].agent.tick(tick_id, tc)
                });
                TICK_ENTERED_MS.store(0, core::sync::atomic::Ordering::Relaxed);
                if let (Some(c), Some(report)) = (wdt_clock, wdt_slow) {
                    let dt = c().wrapping_sub(wdt_t0);
                    if dt > TICK_WATCHDOG_MS {
                        report(agent_name, dt);
                    }
                }
                polled = polled.saturating_add(1);

                // PostTick hook: always run, even after Pending
                self.hooks.run(hooks::HookType::PostTick, agent_name, tick_id);

                // Watchdog: detecta loops infinitos (10000+ ticks sem Done).
                // Só crashea agentes SEM urgency (espelha a isenção do rate-limit
                // acima): interativos (urgency>0) retornam Pending por design e
                // pollam todo tick — crashear por "nunca Done" os mataria em ~9 min
                // sem recuperação (RESPAWN_QUEUE sem writers, hooks não wireados).
                match result {
                    AgentTickResult::Pending => {
                        self.agents[i].consecutive_pending += 1;
                        if watchdog_should_crash(
                            self.agents[i].goal_urgency,
                            self.agents[i].consecutive_pending,
                        ) {
                            self.agents[i].state = AgentState::Crashed;
                        }
                    }
                    AgentTickResult::Done => {
                        self.agents[i].consecutive_pending = 0;
                        if self.agents[i].schedule == ScheduleKind::Oneshot {
                            self.agents[i].state = AgentState::Done;
                        }
                    }
                    AgentTickResult::Crashed => {
                        self.agents[i].state = AgentState::Crashed;
                        // OnCrash hook: notify registered crash handlers
                        self.hooks.run(hooks::HookType::OnCrash, agent_name, tick_id);
                    }
                    _ => {}
                }
            }
            // Novelty decay (RuVix): decrease all novelty scores by 1 each tick
            for agent in &mut self.agents {
                if agent.novelty_score > 0 {
                    agent.novelty_score -= 1;
                }
            }
            maybe_log_sched_metrics(tick_id, self.agents.len(), polled);
            halt();
        }
    }
}

/// Hook opcional de métricas (N1.3). Kernel registra `serial_println` via `set_sched_metrics_hook`.
static mut SCHED_METRICS_HOOK: Option<fn(u64, usize, u32)> = None;

/// Hook opcional para BEI tick (ADR-0060). Kernel registra via `set_bei_tick_hook`.
static mut BEI_TICK_HOOK: Option<fn(u64)> = None;

/// Watchdog de tick lento (diagnóstico freeze metal). `clock` devolve ms
/// monotônico (kernel registra `k_nano::tsc::now_ms`); `slow` reporta
/// (agent, ms) quando um tick ultrapassa `TICK_WATCHDOG_MS`. Sem hooks = no-op.
static mut TICK_CLOCK_HOOK: Option<fn() -> u64> = None;
static mut SLOW_TICK_HOOK: Option<fn(&str, u64)> = None;

/// Limiar do watchdog de tick (ms). Acima disso o agente bloqueou o scheduler
/// cooperativo — é exatamente o sintoma do freeze pós-1º-frame no metal.
pub const TICK_WATCHDOG_MS: u64 = 500;

/// Stamp de entrada no tick (ms via clock hook) + agente corrente.
/// 0 = fora de tick. HUD do jarbas lê ao vivo p/ diagnóstico de freeze (s317).
pub static TICK_ENTERED_MS: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);
static CUR_AGENT_PTR: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);
static CUR_AGENT_LEN: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);

/// (agente corrente, ms de entrada) se um tick está em curso; None fora.
///
/// SAFETY: ptr/len são gravados no `run()` a partir de `&'static str`
/// (manifestos de agente — vivem no .rodata do binário); a leitura reconstrói
/// o &str sem cópia. Fora de tick (stamp 0) retorna None.
pub fn tick_in_progress() -> Option<(&'static str, u64)> {
    let entered = TICK_ENTERED_MS.load(core::sync::atomic::Ordering::Relaxed);
    if entered == 0 {
        return None;
    }
    let ptr = CUR_AGENT_PTR.load(core::sync::atomic::Ordering::Relaxed) as *const u8;
    let len = CUR_AGENT_LEN.load(core::sync::atomic::Ordering::Relaxed);
    if ptr.is_null() || len == 0 || len > 64 {
        return None;
    }
    Some(unsafe {
        (
            core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len)),
            entered,
        )
    })
}

/// ADR-0089: se true, ring≥1 vão para run-queue AP (BSP só tick ring0).
static mut SMP_OFFLOAD_PREDICATE: Option<fn() -> bool> = None;
/// Batch: (idx, affinity_ring, priority, coherence_partner, urgency) → distributed count.
static mut SMP_DISTRIBUTE: Option<fn(&[(u32, u8, u8, Option<usize>, u8)], u32) -> usize> = None;

/// Spinlock cooperativa BSP↔AP sobre ticks de agent (zero-dep).
/// Invariante AIOS/ArceOS wake_handoff: hold curto apenas em torno do `tick()` —
/// sem spin infinito esperando remote `on_cpu` / handoff cross-core.
static AGENT_TICK_BUSY: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

fn with_agent_tick_lock<R>(f: impl FnOnce() -> R) -> R {
    while AGENT_TICK_BUSY.swap(true, core::sync::atomic::Ordering::AcqRel) {
        core::hint::spin_loop();
    }
    let r = f();
    AGENT_TICK_BUSY.store(false, core::sync::atomic::Ordering::Release);
    r
}

/// Ponteiro do registry heap-pinned — AP tick via índice.
static REGISTRY_PTR: AtomicPtr<AgentRegistry> = AtomicPtr::new(null_mut());

pub fn set_registry_ptr(reg: &mut AgentRegistry) {
    REGISTRY_PTR.store(reg as *mut AgentRegistry, core::sync::atomic::Ordering::Release);
}

pub fn set_smp_offload_hooks(
    predicate: Option<fn() -> bool>,
    distribute: Option<fn(&[(u32, u8, u8, Option<usize>, u8)], u32) -> usize>,
) {
    unsafe {
        SMP_OFFLOAD_PREDICATE = predicate;
        SMP_DISTRIBUTE = distribute;
    }
}

/// Tick de um agent por índice (AP run-queue). Retorna true se tickou.
pub fn tick_agent_by_index(idx: u32, tick_id: u32) -> bool {
    let ptr = REGISTRY_PTR.load(core::sync::atomic::Ordering::Acquire);
    if ptr.is_null() {
        return false;
    }
    with_agent_tick_lock(|| {
        // SAFETY: registry vive no leak do boot; exclusive via AGENT_TICK_BUSY.
        let reg = unsafe { &mut *ptr };
        let i = idx as usize;
        if i >= reg.agents.len() {
            return false;
        }
        if reg.agents[i].state != AgentState::Active {
            return false;
        }
        reg.agents[i].tick_counter += 1;
        let tc = reg.agents[i].tick_counter;
        let _ = reg.agents[i].agent.tick(tick_id as u64, tc);
        true
    })
}

/// Snapshot para HUD Jarbas (atualizado a cada tick do scheduler).
pub static LAST_SCHED_AGENTS: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);
pub static LAST_SCHED_POLLED: core::sync::atomic::AtomicU32 =
    core::sync::atomic::AtomicU32::new(0);

/// Período em ticks do scheduler entre logs `[SCHED]`.
pub const SCHED_METRICS_PERIOD: u64 = 32;

pub fn set_sched_metrics_hook(hook: Option<fn(u64, usize, u32)>) {
    unsafe { SCHED_METRICS_HOOK = hook; }
}

/// Registra clock (ms monotônico) + reporter de tick lento (watchdog).
/// Sem registro = medição desligada (custo zero no loop).
pub fn set_tick_watchdog_hooks(clock: Option<fn() -> u64>, slow: Option<fn(&str, u64)>) {
    unsafe {
        TICK_CLOCK_HOOK = clock;
        SLOW_TICK_HOOK = slow;
    }
}

/// Registra hook para BEI tick (ADR-0060). Chamado a cada tick do scheduler.
pub fn set_bei_tick_hook(hook: Option<fn(u64)>) {
    unsafe { BEI_TICK_HOOK = hook; }
}

// ─── Budget watchdog global reference ───
/// Raw pointer to the AgentRegistry's BudgetManager, set during kernel init.
/// Used by `agent_budget_stats()` for Hermes monitoring without passing the registry around.
static BUDGET_MGR_PTR: AtomicPtr<BudgetManager> = AtomicPtr::new(null_mut());

/// Set the global BudgetManager reference for the stats free function.
/// Called once during kernel agent registry initialization.
pub fn set_budget_stats_ref(bm: &mut BudgetManager) {
    BUDGET_MGR_PTR.store(bm, core::sync::atomic::Ordering::Release);
}

/// Public API for Hermes monitoring: returns snapshot of all agent budgets.
/// Each entry: `(agent_name, ticks_used, watchdog_state)`.
pub fn agent_budget_stats() -> Vec<(String, u64, AgentWatchdogState)> {
    let ptr = BUDGET_MGR_PTR.load(core::sync::atomic::Ordering::Acquire);
    if ptr.is_null() {
        return Vec::new();
    }
    // SAFETY: BudgetManager lives as long as AgentRegistry, which is a kernel static.
    // `set_budget_stats_ref` is called once at init; the reference stays valid.
    unsafe { (*ptr).stats() }
}

// ─── Budget event logging hook ───
static mut BUDGET_EVENT_HOOK: Option<fn(&str, &str)> = None;

/// Register a hook for budget watchdog events (paused, recovered, crashed, warning).
/// Kernel provides `serial_println`-based logging via this hook.
pub fn set_budget_event_hook(hook: Option<fn(&str, &str)>) {
    unsafe { BUDGET_EVENT_HOOK = hook; }
}

fn maybe_log_budget(name: &str, event: &str) {
    if let Some(hook) = unsafe { BUDGET_EVENT_HOOK } {
        hook(name, event);
    }
}

fn maybe_log_sched_metrics(tick_id: u64, n_agents: usize, polled: u32) {
    LAST_SCHED_AGENTS.store(n_agents, core::sync::atomic::Ordering::Relaxed);
    LAST_SCHED_POLLED.store(polled, core::sync::atomic::Ordering::Relaxed);
    if tick_id == 1 || tick_id % SCHED_METRICS_PERIOD == 0 {
        if let Some(hook) = unsafe { SCHED_METRICS_HOOK } {
            hook(tick_id, n_agents, polled);
        }
    }
    // BEI tick hook (ADR-0060)
    if let Some(hook) = unsafe { BEI_TICK_HOOK } {
        hook(tick_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sched_semantics_event_driven_and_watchdog() {
        // EventDriven: polla no 1º tick (announce) e quando há trabalho pendente;
        // dorme sem contador self-referential (nunca fica órfão por consecutive).
        assert!(event_driven_has_event(0, false));      // 1º tick
        assert!(event_driven_has_event(500, true));     // trabalho pendente
        assert!(!event_driven_has_event(500, false));   // dorme
        // Watchdog: só crashea sem urgency (interativos com urgency>0 imunes).
        assert!(!watchdog_should_crash(200, 1_000_000));
        assert!(!watchdog_should_crash(0, 10000));
        assert!(watchdog_should_crash(0, 10001));
    }
}
