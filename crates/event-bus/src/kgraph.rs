use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use alloc::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NodeKind {
    Agent, Skill, Hardware, Event, Unknown,
}

#[derive(Debug)]
pub struct KNode {
    pub id: usize,
    pub kind: NodeKind,
    pub label: String,
}

#[derive(Debug)]
pub struct KEdge {
    pub source: usize,
    pub target: usize,
    pub relation: String,
}

#[derive(Debug)]
pub struct Graph {
    pub nodes: Vec<KNode>,
    pub edges: Vec<KEdge>,
    pub label_map: BTreeMap<String, usize>,
}

impl Graph {
    pub fn new() -> Self {
        Graph { nodes: Vec::new(), edges: Vec::new(), label_map: BTreeMap::new() }
    }

    pub fn add_node(&mut self, kind: NodeKind, label: &str) -> usize {
        if let Some(&id) = self.label_map.get(label) { return id; }
        let id = self.nodes.len();
        self.nodes.push(KNode { id, kind, label: String::from(label) });
        self.label_map.insert(String::from(label), id);
        id
    }

    pub fn add_edge(&mut self, source: usize, target: usize, relation: &str) {
        self.edges.push(KEdge { source, target, relation: String::from(relation) });
    }

    pub fn get(&self, label: &str) -> Option<&KNode> {
        self.label_map.get(label).and_then(|&id| self.nodes.get(id))
    }

    pub fn neighbors(&self, id: usize) -> Vec<(usize, &str)> {
        let mut result = Vec::new();
        for e in &self.edges {
            if e.source == id {
                if let Some(n) = self.nodes.get(e.target) {
                    result.push((e.target, n.label.as_str()));
                }
            }
            if e.target == id {
                if let Some(n) = self.nodes.get(e.source) {
                    result.push((e.source, n.label.as_str()));
                }
            }
        }
        result
    }

    pub fn query(&self, relation: &str) -> Vec<(usize, usize)> {
        self.edges.iter()
            .filter(|e| e.relation == relation)
            .map(|e| (e.source, e.target))
            .collect()
    }
}
