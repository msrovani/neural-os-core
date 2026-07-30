//! Crew — grupo de agentes com objetivo comum (CrewAI-inspired).
//! Um Crew é um time orquestrado: agentes colaboram, tasks sao delegadas,
//! resultados sao consolidados. HermesAgent atua como ManagerAgent.
//!
//! Integracao: AgentRegistry::create_crew() monta um Crew a partir dos agentes
//! registrados com mesmo objetivo. CrewPool::resolve() ordena execucao por
//! dependencia. Scheduler usa order se crew_mode=true.

use alloc::string::String;
use alloc::vec::Vec;

pub type CrewId = u16;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProcessType {
    /// Agentes executam tasks em sequencia
    Sequential,
    /// ManagerAgent (Hermes) delega e coordena
    Hierarchical,
}

#[derive(Debug, Clone)]
pub struct ScheduledTask {
    pub id: u16,
    pub description: String,
    pub agent_name: String,
    pub skill: String,
    pub depends_on: Vec<u16>,
    pub expected_output: OutputSchema,
    pub priority: u8,
    pub done: bool,
    pub result: Vec<u8>,
}

#[derive(Debug, Clone)]
pub enum OutputSchema {
    Any,
    String,
    Json(Vec<String>),
}

#[derive(Debug, Clone)]
pub struct Crew {
    pub id: CrewId,
    pub name: String,
    pub goal: String,
    pub process: ProcessType,
    pub agent_ids: Vec<usize>,     // indices no AgentRegistry
    pub tasks: Vec<ScheduledTask>,
    pub active: bool,
}

impl Crew {
    pub fn new(id: CrewId, name: &str, goal: &str, process: ProcessType) -> Self {
        Crew {
            id, name: String::from(name), goal: String::from(goal),
            process, agent_ids: Vec::new(), tasks: Vec::new(), active: false,
        }
    }

    pub fn add_task(&mut self, desc: &str, agent: &str, skill: &str) -> u16 {
        let id = self.tasks.len() as u16 + 1;
        self.tasks.push(ScheduledTask {
            id, description: String::from(desc),
            agent_name: String::from(agent), skill: String::from(skill),
            depends_on: Vec::new(), expected_output: OutputSchema::Any,
            priority: 5, done: false, result: Vec::new(),
        });
        id
    }
}

/// Pool de crews gerenciado pelo AgentRegistry
pub struct CrewPool {
    crews: Vec<Crew>,
    next_id: CrewId,
}

impl CrewPool {
    pub fn new() -> Self {
        CrewPool { crews: Vec::new(), next_id: 1 }
    }

    pub fn create(&mut self, name: &str, goal: &str, process: ProcessType) -> CrewId {
        let id = self.next_id;
        self.next_id += 1;
        self.crews.push(Crew::new(id, name, goal, process));
        id
    }

    pub fn get(&self, id: CrewId) -> Option<&Crew> {
        self.crews.iter().find(|c| c.id == id)
    }

    pub fn get_mut(&mut self, id: CrewId) -> Option<&mut Crew> {
        self.crews.iter_mut().find(|c| c.id == id)
    }

    pub fn assign_to_crew(&mut self, crew_id: CrewId, agent_idx: usize) {
        if let Some(crew) = self.crews.iter_mut().find(|c| c.id == crew_id) {
            if !crew.agent_ids.contains(&agent_idx) {
                crew.agent_ids.push(agent_idx);
            }
        }
    }

    /// Kickoff: marca crew como ativo, tasks comecam a ser processadas
    pub fn kickoff(&mut self, crew_id: CrewId) -> bool {
        if let Some(crew) = self.crews.iter_mut().find(|c| c.id == crew_id) {
            crew.active = true;
            true
        } else { false }
    }

    /// Retorna proxima task pronta (dependencias resolvidas, nao done)
    pub fn next_ready_task(&self, crew_id: CrewId) -> Option<&ScheduledTask> {
        let crew = self.crews.iter().find(|c| c.id == crew_id)?;
        if !crew.active { return None; }
        crew.tasks.iter().find(|t| {
            if t.done { return false; }
            t.depends_on.iter().all(|dep_id| {
                crew.tasks.iter().find(|dt| dt.id == *dep_id).map_or(true, |dt| dt.done)
            })
        })
    }

    pub fn complete_task(&mut self, crew_id: CrewId, task_id: u16, result: Vec<u8>) {
        if let Some(crew) = self.crews.iter_mut().find(|c| c.id == crew_id) {
            if let Some(task) = crew.tasks.iter_mut().find(|t| t.id == task_id) {
                task.done = true;
                task.result = result;
            }
            // Se todas as tasks estao done, crew terminou
            if crew.tasks.iter().all(|t| t.done) {
                crew.active = false;
            }
        }
    }

    pub fn list(&self) -> Vec<&Crew> {
        self.crews.iter().filter(|c| c.active).collect()
    }
}
