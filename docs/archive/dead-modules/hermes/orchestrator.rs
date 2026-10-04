//! Multi-Agent Orchestration (IDEA A-016).
//! Graph-based execution: sequential, concurrent, handoff entre agents.
//!
//! AIOS na veia: workflow de agents coordenados, não tasks avulsas.
//! Substitui o dead ToT decomposition anterior.

#![allow(dead_code)]

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use k_nano::EVENT_BUS;

/// Nó no grafo de orquestração.
#[derive(Debug, Clone)]
pub struct WorkflowNode {
    pub id: String,
    pub agent_name: String,
    pub skill: String,
    pub depends_on: Vec<String>, // IDs dos nós que devem executar antes
}

/// Modo de execução de um workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    Sequential, // Um após o outro
    Concurrent, // Em paralelo (quando sem dependências)
    Handoff,    // Handoff: saída de A vira entrada de B
}

/// Resultado da execução de um nó.
#[derive(Debug, Clone)]
pub struct NodeResult {
    pub node_id: String,
    pub success: bool,
    pub output: String,
    pub ticks_taken: u64,
}

/// Workflow em execução.
#[derive(Debug, Clone)]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub nodes: Vec<WorkflowNode>,
    pub mode: ExecutionMode,
    pub results: BTreeMap<String, NodeResult>,
    pub current_node: usize,
    pub status: WorkflowStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowStatus {
    Pending,
    Running,
    Completed,
    Failed(String),
}

impl Workflow {
    pub fn new(id: &str, name: &str, nodes: Vec<WorkflowNode>, mode: ExecutionMode) -> Self {
        Self {
            id: String::from(id),
            name: String::from(name),
            nodes,
            mode,
            results: BTreeMap::new(),
            current_node: 0,
            status: WorkflowStatus::Pending,
        }
    }

    /// Verifica se o nó pode executar (dependências resolvidas).
    pub fn can_execute(&self, node: &WorkflowNode) -> bool {
        node.depends_on
            .iter()
            .all(|dep| self.results.get(dep).map(|r| r.success).unwrap_or(false))
    }

    /// Avança o workflow (deve ser chamado a cada tick do orchestrator agent).
    pub fn tick(&mut self, _tick: u64) {
        match self.status {
            WorkflowStatus::Pending => {
                self.status = WorkflowStatus::Running;
            }
            WorkflowStatus::Running => {
                if self.current_node >= self.nodes.len() {
                    self.status = WorkflowStatus::Completed;
                    return;
                }
                let node = &self.nodes[self.current_node];
                if self.can_execute(node) {
                    // Publica evento para o agent executar a skill
                    let _ = EVENT_BUS.publish(event_bus::Event {
                        id: 0,
                        topic: alloc::format!("AGENT_EXECUTE_{}", node.agent_name),
                        payload: node.skill.as_bytes().to_vec(),
                        token: event_bus::CapabilityToken::Legacy(1),
                    });
                    self.results.insert(
                        node.id.clone(),
                        NodeResult {
                            node_id: node.id.clone(),
                            success: true,
                            output: String::new(),
                            ticks_taken: 0,
                        },
                    );
                    self.current_node += 1;
                }
            }
            _ => {}
        }
    }
}

/// Orquestrador de workflows multi-agente.
pub struct MultiAgentOrchestrator {
    pub workflows: Vec<Workflow>,
}

impl MultiAgentOrchestrator {
    pub fn new() -> Self {
        Self {
            workflows: Vec::new(),
        }
    }

    /// Registra um novo workflow.
    pub fn register(&mut self, workflow: Workflow) {
        self.workflows.push(workflow);
    }

    /// Tick de todos os workflows ativos.
    pub fn tick_all(&mut self, tick: u64) {
        for wf in &mut self.workflows {
            if matches!(wf.status, WorkflowStatus::Running | WorkflowStatus::Pending) {
                wf.tick(tick);
            }
        }
        // Remove completed workflows
        self.workflows
            .retain(|w| !matches!(w.status, WorkflowStatus::Completed));
    }

    pub fn workflow_count(&self) -> usize {
        self.workflows.len()
    }
}
