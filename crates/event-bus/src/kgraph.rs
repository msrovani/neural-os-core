use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use alloc::collections::BTreeMap;

pub const EMBED_DIM: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NodeKind { Agent, Skill, Hardware, Event, Unknown }

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
    /// Bi-temporal: validade do fato (aplicacao)
    pub valid_from: u64,  // tick quando o fato comecou a ser verdadeiro
    pub valid_to: u64,    // tick quando o fato deixou de ser verdadeiro (0 = infinito)
    /// Bi-temporal: quando o fato foi registrado (sistema)
    pub tx_from: u64,     // tick em que esta versao foi inserida
    pub tx_to: u64,       // tick em que esta versao foi substituida (0 = atual)
}

#[derive(Debug)]
pub struct Graph {
    pub nodes: Vec<KNode>,
    pub edges: Vec<KEdge>,
    pub label_map: BTreeMap<String, usize>,
    pub tick: u64,
}

impl Graph {
    pub fn new() -> Self {
        Graph { nodes: Vec::new(), edges: Vec::new(), label_map: BTreeMap::new(), tick: 0 }
    }

    pub fn set_tick(&mut self, tick: u64) { self.tick = tick; }

    pub fn add_node(&mut self, kind: NodeKind, label: &str) -> usize {
        if let Some(&id) = self.label_map.get(label) { return id; }
        let id = self.nodes.len();
        self.nodes.push(KNode { id, kind, label: String::from(label) });
        self.label_map.insert(String::from(label), id);
        id
    }

    /// Adiciona aresta com validade temporal (aplicacao e sistema)
    pub fn add_edge(&mut self, source: usize, target: usize, relation: &str) {
        self.add_edge_with_time(source, target, relation, self.tick, 0);
    }

    pub fn add_edge_with_time(&mut self, source: usize, target: usize, relation: &str,
                               valid_from: u64, valid_to: u64) {
        // Fecha versao anterior do sistema (tx_to = now)
        for e in self.edges.iter_mut().rev() {
            if e.source == source && e.target == target && e.relation == relation && e.tx_to == 0 {
                e.tx_to = self.tick;
                break;
            }
        }
        self.edges.push(KEdge {
            source, target, relation: String::from(relation),
            valid_from, valid_to, tx_from: self.tick, tx_to: 0,
        });
    }

    /// Invalida um fato (aplica valid_to)
    pub fn invalidate_edge(&mut self, source: usize, target: usize, relation: &str, at_tick: u64) {
        for e in self.edges.iter_mut().rev() {
            if e.source == source && e.target == target && e.relation == relation && e.valid_to == 0 {
                e.valid_to = at_tick;
                break;
            }
        }
    }

    /// Query: fatos validos em um dado tick (as-of query)
    pub fn as_of(&self, tick: u64) -> Vec<(usize, usize, &str)> {
        self.edges.iter()
            .filter(|e| e.valid_from <= tick && (e.valid_to == 0 || e.valid_to > tick)
                    && e.tx_from <= tick && (e.tx_to == 0 || e.tx_to > tick))
            .map(|e| (e.source, e.target, e.relation.as_str()))
            .collect()
    }

    pub fn query(&self, relation: &str) -> Vec<(usize, usize)> {
        self.edges.iter()
            .filter(|e| e.relation == relation && e.valid_to == 0 && e.tx_to == 0)
            .map(|e| (e.source, e.target))
            .collect()
    }

    pub fn neighbors(&self, id: usize) -> Vec<(usize, &str)> {
        let mut result = Vec::new();
        for e in &self.edges {
            if e.valid_to != 0 || e.tx_to != 0 { continue; }
            if e.source == id {
                if let Some(n) = self.nodes.get(e.target) { result.push((e.target, n.label.as_str())); }
            }
            if e.target == id {
                if let Some(n) = self.nodes.get(e.source) { result.push((e.source, n.label.as_str())); }
            }
        }
        result
    }

    pub fn ranked_query(&self, query: &str) -> Vec<(usize, String, f32)> {
        let mut scores: BTreeMap<usize, f32> = BTreeMap::new();
        let query_lower = query.to_ascii_lowercase();
        for node in &self.nodes {
            let label_lower = node.label.to_ascii_lowercase();
            if label_lower.contains(&query_lower) {
                let score = 1.0 + (label_lower.len() as f32).recip();
                *scores.entry(node.id).or_insert(0.0) += score;
            }
        }
        let matched: Vec<usize> = scores.keys().cloned().collect();
        for &id in &matched {
            for edge in &self.edges {
                if edge.valid_to != 0 || edge.tx_to != 0 { continue; }
                if edge.source == id || edge.target == id {
                    let other = if edge.source == id { edge.target } else { edge.source };
                    *scores.entry(other).or_insert(0.0) += 0.5;
                }
            }
        }
        let mut result: Vec<(usize, String, f32)> = scores.into_iter()
            .filter_map(|(id, score)| self.nodes.get(id).map(|n| (id, n.label.clone(), score)))
            .collect();
        result.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(core::cmp::Ordering::Equal));
        result.truncate(10);
        result
    }

    /// Historico completo de uma relacao (todas as versoes)
    pub fn history(&self, source: usize, target: usize, relation: &str) -> Vec<(u64, u64, u64, u64)> {
        self.edges.iter()
            .filter(|e| e.source == source && e.target == target && e.relation == relation)
            .map(|e| (e.valid_from, e.valid_to, e.tx_from, e.tx_to))
            .collect()
    }
}
