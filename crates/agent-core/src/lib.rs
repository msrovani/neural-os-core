#![no_std]
extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::collections::BTreeMap;

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

pub struct AgentInstance {
    pub agent: Box<dyn Agent>,
    pub state: AgentState,
    pub last_poll: u64,
    pub tick_counter: u64,
    pub schedule: ScheduleKind,
    pub consecutive_pending: u64,  // watchdog: ticks consecutivos sem Done
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
        }
    }
}

pub struct AgentRegistry {
    pub agents: Vec<AgentInstance>,
    pub skill_map: BTreeMap<String, usize>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        AgentRegistry { agents: Vec::new(), skill_map: BTreeMap::new() }
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
}

impl AgentRegistry {
    /// Run all Oneshot agents synchronously (boot phase).
    pub fn init_phase(&mut self) {
        let mut i = 0;
        while i < self.agents.len() {
            let sched = self.agents[i].schedule;
            if sched != ScheduleKind::Oneshot { i += 1; continue; }
            if !self.agents[i].agent.manifest().auto_start { i += 1; continue; }
            // Use raw pointer to bypass borrow checker for synchronous boot init
            let ptr: *mut AgentInstance = &mut self.agents[i];
            unsafe {
                (*ptr).state = AgentState::Active;
                (*ptr).agent.on_activate();
                loop {
                    let result = (*ptr).agent.tick(0, (*ptr).tick_counter + 1);
                    (*ptr).tick_counter += 1;
                    match result {
                        AgentTickResult::Done => { (*ptr).state = AgentState::Done; break; }
                        AgentTickResult::Crashed => { (*ptr).state = AgentState::Crashed; break; }
                        AgentTickResult::Pending => {}
                    }
                }
            }
            i += 1;
        }
    }

    /// Scheduler loop — called from kernel
    /// `halt()`: called when no agent needs CPU (platform-specific hlt)
    /// `check_respawns(): returns names of agents to re-create (e.g., from RESPAWN_QUEUE)`
    /// `spawn_agent(name): creates a new Agent by name`
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
            for i in 0..self.agents.len() {
                let state = self.agents[i].state;
                if state != AgentState::Active {
                    continue;
                }
                let schedule = self.agents[i].schedule;
                let last = self.agents[i].last_poll;
                let should_poll = match schedule {
                    ScheduleKind::Continuous => true,
                    ScheduleKind::PollEvery(n) => last == 0 || tick_id - last >= n,
                    ScheduleKind::Oneshot => true,  // poll every tick until Done/Crashed
                    ScheduleKind::EventDriven => false,
                };
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
                        if schedule == ScheduleKind::Oneshot {
                            self.agents[i].state = AgentState::Done;
                        }
                    }
                    AgentTickResult::Crashed => {
                        self.agents[i].state = AgentState::Crashed;
                    }
                    AgentTickResult::Pending => {}
                }
            }
            halt();
        }
    }
}
