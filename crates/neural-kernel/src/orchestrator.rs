//! OrchestratorAgent — Tree-of-Thought task decomposition.
//! Swarms-inspired: quebra tarefas em sub-tarefas, coleta resultados.
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

#[derive(Debug)]
pub struct SubTask {
    pub id: u16,
    pub description: String,
    pub required_skills: Vec<String>,
    pub depends_on: Vec<u16>,
}

#[derive(Debug)]
pub struct SubTaskResult {
    pub id: u16,
    pub output: String,
    pub success: bool,
}

pub struct Orchestrator {
    pub agents: Vec<(&'static str, Vec<&'static str>)>, // (agent, skills)
}

impl Orchestrator {
    pub fn new() -> Self {
        Orchestrator { agents: Vec::new() }
    }

    pub fn register_agent(&mut self, name: &'static str, skills: &[&'static str]) {
        self.agents.push((name, skills.to_vec()));
    }

    pub fn decompose(&self, task: &str) -> Vec<SubTask> {
        let lower = task.to_ascii_lowercase();
        let mut tasks = Vec::new();
        // Heurística simples: divide por palavras-chave
        if lower.contains("hardware") || lower.contains("pci") {
            tasks.push(SubTask { id: 1, description: String::from("Scan PCI devices"), required_skills: vec![String::from("pci")], depends_on: vec![] });
            tasks.push(SubTask { id: 2, description: String::from("Identify devices via LLM"), required_skills: vec![String::from("llm")], depends_on: vec![1] });
        }
        if lower.contains("rede") || lower.contains("network") {
            tasks.push(SubTask { id: 3, description: String::from("Check network interfaces"), required_skills: vec![String::from("net")], depends_on: vec![] });
        }
        if lower.contains("memoria") || lower.contains("memory") {
            tasks.push(SubTask { id: 4, description: String::from("Check memory usage"), required_skills: vec![String::from("memory")], depends_on: vec![] });
        }
        tasks
    }

    pub fn assign(&self, task: &SubTask) -> Option<&'static str> {
        for (agent, skills) in &self.agents {
            for req in &task.required_skills {
                if skills.iter().any(|s| s == req) {
                    return Some(agent);
                }
            }
        }
        None
    }
}
