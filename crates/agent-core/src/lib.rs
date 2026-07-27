#![no_std]
#![allow(dead_code)]
extern crate alloc;

pub mod budget;
pub mod hooks;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::collections::BTreeMap;

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
}

impl AgentInstance {
    pub fn new(agent: Box<dyn Agent>) -> Self {
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
        }
    }
}

pub struct AgentRegistry {
    pub agents: Vec<AgentInstance>,
    pub skill_map: BTreeMap<String, usize>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        AgentRegistry {
            agents: Vec::new(), skill_map: BTreeMap::new(),
        }
    }

    pub fn register(&mut self, agent: Box<dyn Agent>) -> usize {
        let idx = self.agents.len();
        let instance = AgentInstance::new(agent);
        self.agents.push(instance);
        idx
    }

    pub fn activate(&mut self, idx: usize) {
        if idx < self.agents.len() {
            self.agents[idx].state = AgentState::Active;
            self.agents[idx].agent.on_activate();
        }
    }

    pub fn get(&self, name: &str) -> Option<&AgentInstance> {
        self.agents.iter().find(|a| a.agent.manifest().name == name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut AgentInstance> {
        self.agents.iter_mut().find(|a| a.agent.manifest().name == name)
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
    /// Boot Oneshot round-robin até todos Done **ou** timeout.
    /// NÃO processa agentes um-a-um até Done (hang se A espera evento de B).
    /// Pendentes após timeout ficam Active para o `run()` — impossível hangar o boot.
    pub fn init_phase(&mut self) {
        const MAX_ROUNDS: u64 = 10_000;
        for i in 0..self.agents.len() {
            if self.agents[i].schedule != ScheduleKind::Oneshot { continue; }
            if !self.agents[i].agent.manifest().auto_start { continue; }
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
            for i in 0..self.agents.len() {
                if self.agents[i].schedule != ScheduleKind::Oneshot { continue; }
                if self.agents[i].state != AgentState::Active { continue; }
                any_active = true;
                self.agents[i].tick_counter += 1;
                let tc = self.agents[i].tick_counter;
                match self.agents[i].agent.tick(round, tc) {
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
        for i in 0..self.agents.len() {
            if self.agents[i].state == AgentState::Done || self.agents[i].state == AgentState::Crashed {
                continue;
            }
            if self.agents[i].agent.manifest().auto_start {
                self.agents[i].state = AgentState::Active;
                self.agents[i].agent.on_activate();
            }
        }
        let mut tick_id: u64 = 0;
        loop {
            tick_id += 1;
            // Check for respawn requests before polling agents
            let respawns = check_respawns();
            for name in &respawns {
                if let Some(agent) = spawn_agent(name) {
                    let idx = self.agents.len();
                    self.agents.push(AgentInstance::new(agent));
                    self.agents[idx].state = AgentState::Active;
                    self.agents[idx].agent.on_activate();
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
                let flow = &self.agents[i].crew.flow;
                // Rate-limiting: passive agents (Pending >50x consec) skipped 80% of ticks
                // Goal-aware: agents with urgency > 0 are NOT rate-limited
                let consecutive = self.agents[i].consecutive_pending;
                let urgency = self.agents[i].goal_urgency;
                if urgency == 0 && consecutive > 50 && tick_id % 5 != 0 {
                    continue;
                }
                let schedule = self.agents[i].schedule;
                let has_event = schedule != ScheduleKind::EventDriven || consecutive < 20;
                let should_poll = should_poll_flow(flow, tick_id, self.agents[i].last_poll, has_event);
                if !should_poll {
                    continue;
                }
                self.agents[i].last_poll = tick_id;
                self.agents[i].tick_counter += 1;
                let tc = self.agents[i].tick_counter;
                let result = self.agents[i].agent.tick(tick_id, tc);
                polled = polled.saturating_add(1);
                // Watchdog: detecta loops infinitos (10000+ ticks sem Done)
                match result {
                    AgentTickResult::Pending => {
                        self.agents[i].consecutive_pending += 1;
                        if self.agents[i].consecutive_pending > 10000 {
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

/// Registra hook para BEI tick (ADR-0060). Chamado a cada tick do scheduler.
pub fn set_bei_tick_hook(hook: Option<fn(u64)>) {
    unsafe { BEI_TICK_HOOK = hook; }
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
