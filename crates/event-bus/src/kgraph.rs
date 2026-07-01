use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use alloc::collections::BTreeMap;

pub const EMBED_DIM: usize = 16; // CodebookVQ embedding size (leve)

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

    /// Gbrain-style reranker: combina score de relacionamento + frequencia
    pub fn ranked_query(&self, query: &str) -> Vec<(usize, String, f32)> {
        let mut scores: BTreeMap<usize, f32> = BTreeMap::new();

        // Score por match de label
        for node in &self.nodes {
            let label_lower = node.label.to_ascii_lowercase();
            let query_lower = query.to_ascii_lowercase();
            if label_lower.contains(&query_lower) {
                let score = 1.0 + (label_lower.len() as f32).recip();
                *scores.entry(node.id).or_insert(0.0) += score;
            }
        }

        // Score por relacao com nodes matchados
        let matched: Vec<usize> = scores.keys().cloned().collect();
        for &id in &matched {
            for edge in &self.edges {
                if edge.source == id || edge.target == id {
                    let other = if edge.source == id { edge.target } else { edge.source };
                    *scores.entry(other).or_insert(0.0) += 0.5;
                }
            }
        }

        // Ordena por score
        let mut result: Vec<(usize, String, f32)> = scores.into_iter()
            .filter_map(|(id, score)| {
                self.nodes.get(id).map(|n| (id, n.label.clone(), score))
            })
            .collect();
        result.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(core::cmp::Ordering::Equal));
        result.truncate(10);
        result
    }
}
