#![no_std]
#![allow(dead_code)]
extern crate alloc;

pub mod pipeline;
pub mod dagsched;
pub mod dashboard;
pub mod state;
pub mod timer_wheel;
pub mod crew;
pub mod flow;
pub mod state_graph;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::collections::BTreeMap;
use crate::crew::CrewPool;
use crate::state_graph::StateGraph;
use crate::flow::should_poll_flow;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AgentTier {
    /// Agentes do sistema (nunca suspensos)
    Permanent,
    /// Agentes ativados sob demanda do sistema (PCI, ACPI)
    System,
    /// Agentes ativados por demanda do usuario
    User,
    /// Agentes com schedule periodico (baixa prioridade)
    Periodic,
    /// Agentes de aprendizado (mínima prioridade, executa quando idle)
    Learning,
}

impl AgentTier {
    pub fn priority(&self) -> u8 {
        match self {
            AgentTier::Permanent => 0,
            AgentTier::System => 1,
            AgentTier::User => 2,
            AgentTier::Periodic => 3,
            AgentTier::Learning => 4,
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

/// Agente que tambem implementa CrewManifest (role, goal, flow)
pub trait CrewAgent: Agent {
    fn crew_manifest(&self) -> &CrewManifest;
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
}

impl AgentInstance {
    pub fn new(agent: Box<dyn Agent>) -> Self {
        let schedule = agent.manifest().schedule;
        AgentInstance {
            agent,
            state: AgentState::Inactive,
            last_poll: 0,
            tick_counter: 0,
            schedule,
            consecutive_pending: 0,
            crew: CrewManifest::empty(),
            tier: AgentTier::Permanent,
        }
    }
}

pub struct AgentRegistry {
    pub agents: Vec<AgentInstance>,
    pub skill_map: BTreeMap<String, usize>,
    pub crews: CrewPool,
    pub graph: Option<StateGraph>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        AgentRegistry {
            agents: Vec::new(), skill_map: BTreeMap::new(),
            crews: CrewPool::new(), graph: None,
        }
    }

    /// Cria um crew e retorna seu ID
    pub fn create_crew(&mut self, name: &str, goal: &str, process: crew::ProcessType) -> crew::CrewId {
        self.crews.create(name, goal, process)
    }

    /// Associa um agente a um crew
    pub fn assign_to_crew(&mut self, crew_id: crew::CrewId, agent_idx: usize) {
        self.crews.assign_to_crew(crew_id, agent_idx);
        if let Some(_crew) = self.crews.get_mut(crew_id) {
            for agent in self.agents.iter_mut() {
                if agent.crew.crew_id.is_none() {
                    agent.crew.crew_id = Some(crew_id);
                }
            }
        }
    }

    /// Inicializa StateGraph (substitui scheduler round-robin)
    pub fn init_graph(&mut self, graph: StateGraph) {
        self.graph = Some(graph);
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

    /// Migra um agente para um novo tier. Learning/Periodic ganham menos ticks.
    pub fn migrate_to_tier(&mut self, idx: usize, new_tier: AgentTier) -> Result<(), &'static str> {
        if idx >= self.agents.len() {
            return Err("migrate_to_tier: indice invalido");
        }
        self.agents[idx].tier = new_tier;
        Ok(())
    }

    /// Migra um agente por nome
    pub fn migrate_to_tier_by_name(&mut self, name: &str, new_tier: AgentTier) -> Result<(), &'static str> {
        if let Some(inst) = self.agents.iter_mut().find(|a| a.agent.manifest().name == name) {
            inst.tier = new_tier;
            Ok(())
        } else {
            Err("migrate_to_tier_by_name: agente nao encontrado")
        }
    }

    /// Filtra agentes por tier
    pub fn agents_by_tier(&self, tier: AgentTier) -> Vec<usize> {
        self.agents.iter().enumerate()
            .filter(|(_, a)| a.tier == tier)
            .map(|(i, _)| i)
            .collect()
    }
}

impl AgentRegistry {
    /// Run all Oneshot agents synchronously (boot phase).
    pub fn init_phase(&mut self) {
        let mut i = 0;
        while i < self.agents.len() {
            let sched = self.agents[i].schedule;
            if sched != ScheduleKind::Oneshot { i += 1; continue; }
            if !self.agents[i].agent.manifest().auto_start { i += 1; continue; }
            // Extrai o agente temporariamente para evitar raw pointer aliasing
            let mut agent = self.agents.remove(i);
            agent.state = AgentState::Active;
            agent.agent.on_activate();
            loop {
                let result = agent.agent.tick(0, agent.tick_counter + 1);
                agent.tick_counter += 1;
                match result {
                    AgentTickResult::Done => { agent.state = AgentState::Done; break; }
                    AgentTickResult::Crashed => { agent.state = AgentState::Crashed; break; }
                    AgentTickResult::Pending => {}
                }
            }
            self.agents.insert(i, agent);
            i += 1;
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
            // Se StateGraph ativo, usa ele para decidir qual agente pollar
            if let Some(ref mut graph) = self.graph {
                let node_idx = graph.advance();
                if node_idx < self.agents.len() {
                    let i = node_idx;
                    self.agents[i].last_poll = tick_id;
                    self.agents[i].tick_counter += 1;
                    let tc = self.agents[i].tick_counter;
                    let result = self.agents[i].agent.tick(tick_id, tc);
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
                halt();
                continue;
            }
            // Scheduler round-robin padrao (com FlowTrigger support)
            for i in 0..self.agents.len() {
                let state = self.agents[i].state;
                if state != AgentState::Active {
                    continue;
                }
                let flow = &self.agents[i].crew.flow;
                // Rate-limiting: passive agents (Pending >50x consec) skipped 80% of ticks
                let consecutive = self.agents[i].consecutive_pending;
                if consecutive > 50 && tick_id % 5 != 0 {
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
            halt();
        }
    }
}
