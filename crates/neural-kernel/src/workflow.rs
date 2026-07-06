//! Visual Workflow Builder + Federated Cluster — #188, #189.

use alloc::vec::Vec;
use alloc::string::String;

pub enum WfNode { Trigger(String), Tool(String), Agent(String), Condition(String), Loop(u32), Gate(String) }
pub struct Workflow { pub name: String, pub nodes: Vec<WfNode>, pub edges: Vec<(usize, usize)> }

pub struct WorkflowBuilder { workflows: Vec<Workflow> }

impl WorkflowBuilder {
    pub fn new() -> Self { WorkflowBuilder { workflows: Vec::new() } }
    pub fn create(&mut self, name: &str) { self.workflows.push(Workflow { name: String::from(name), nodes: Vec::new(), edges: Vec::new() }); }
    pub fn add_node(&mut self, idx: usize, node: WfNode) { if idx < self.workflows.len() { self.workflows[idx].nodes.push(node); } }
    pub fn connect(&mut self, idx: usize, from: usize, to: usize) { if idx < self.workflows.len() { self.workflows[idx].edges.push((from, to)); } }
    pub fn status(&self) -> String { alloc::format!("[WORKFLOW] {} workflows", self.workflows.len()) }
}

pub struct FederatedNode { pub id: String, pub url: String, pub last_seen: u64 }
pub struct FederatedCluster { nodes: Vec<FederatedNode> }
impl FederatedCluster {
    pub fn new() -> Self { FederatedCluster { nodes: Vec::new() } }
    pub fn register(&mut self, id: &str, url: &str) { self.nodes.push(FederatedNode { id: String::from(id), url: String::from(url), last_seen: 0 }); }
    pub fn status(&self) -> String { alloc::format!("[FEDERATED] {} nodes", self.nodes.len()) }
}
