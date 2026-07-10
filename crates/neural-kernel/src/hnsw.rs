//! HNSW (Hierarchical Navigable Small World) index para busca aproximada.
//! Multi-layer graph, O(log N) search. Baseado em Malkov & Yashunin (2016).

use alloc::vec::Vec;
use alloc::vec;
use alloc::string::String;
use core::cmp::Ordering;

const ML: f32 = 0.7;
const M: usize = 8;
const M0: usize = 16;
const EF_CONSTR: usize = 32;
const EF_SEARCH: usize = 16;

#[derive(Clone)]
pub struct HnswNode {
    pub id: u32,
    pub vector: Vec<f32>,
    pub level: usize,
    connections: Vec<Vec<u32>>,
}

#[derive(Clone, Copy, PartialEq)]
struct Candidate(f32, u32);
impl Eq for Candidate {}
impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.partial_cmp(&other.0).unwrap_or(Ordering::Equal)
    }
}
impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}

pub struct HnswIndex {
    pub nodes: Vec<HnswNode>,
    dim: usize,
    max_level: usize,
    enter_point: Option<u32>,
    pub dist_calls: u64,
}

impl HnswIndex {
    pub fn new(dim: usize) -> Self {
        HnswIndex { nodes: Vec::new(), dim, max_level: 0, enter_point: None, dist_calls: 0 }
    }

    pub fn len(&self) -> usize { self.nodes.len() }
    pub fn is_empty(&self) -> bool { self.nodes.is_empty() }

    fn random_level() -> usize {
        let r = crate::hw_rng::HardwareRandom::next_u64_retry(5).unwrap_or(42);
        let f = (r & 0xFFFF) as f32 / 65536.0;
        (-libm::logf(f.max(0.0001)) * ML) as usize
    }

    fn l2_dist(&self, a: &[f32], b: &[f32]) -> f32 {
        let mut d = 0.0f32;
        for i in 0..self.dim.min(a.len().min(b.len())) {
            let diff = a[i] - b[i];
            d += diff * diff;
        }
        d
    }

    fn search_layer(&mut self, query: &[f32], entry: u32, level: usize, ef: usize) -> Vec<(f32, u32)> {
        let n = self.nodes.len();
        let mut visited = vec![false; n];
        let mut candidates = Vec::new();
        let mut results = Vec::new();

        let d = self.l2_dist(query, &self.nodes[entry as usize].vector);
        self.dist_calls += 1;
        candidates.push(Candidate(d, entry));
        results.push(Candidate(d, entry));
        visited[entry as usize] = true;

        while let Some(Candidate(d, node_id)) = candidates.pop() {
            let farthest = results.last().map(|c| c.0).unwrap_or(f32::MAX);
            if d > farthest { break; }
            let conns: Vec<u32> = if level < self.nodes[node_id as usize].connections.len() {
                self.nodes[node_id as usize].connections[level].clone()
            } else { Vec::new() };
            for &neighbor in &conns {
                if visited[neighbor as usize] { continue; }
                visited[neighbor as usize] = true;
                let dist = self.l2_dist(query, &self.nodes[neighbor as usize].vector);
                self.dist_calls += 1;
                let farthest_r = results.last().map(|c| c.0).unwrap_or(f32::MAX);
                if dist < farthest_r || results.len() < ef {
                    candidates.push(Candidate(dist, neighbor));
                    results.push(Candidate(dist, neighbor));
                    results.sort();
                    while results.len() > ef { results.pop(); }
                }
            }
        }
        results.sort();
        results.into_iter().map(|c| (c.0, c.1)).collect()
    }

    pub fn insert(&mut self, id: u32, vector: Vec<f32>) {
        let level = Self::random_level();
        let node = HnswNode { id, vector, level, connections: Vec::new() };
        self.nodes.push(node);
        let node_idx = (self.nodes.len() - 1) as u32;

        if self.enter_point.is_none() {
            for l in 0..=level {
                while self.nodes[node_idx as usize].connections.len() <= l {
                    self.nodes[node_idx as usize].connections.push(Vec::new());
                }
            }
            self.enter_point = Some(node_idx);
            self.max_level = level;
            return;
        }

        let ep = self.enter_point.unwrap();
        let mut curr_entry = ep;
        let query_vec = self.nodes[node_idx as usize].vector.clone();

        for l in (level + 1..=self.max_level).rev() {
            let result = self.search_layer(&query_vec, curr_entry, l, 1);
            if let Some(&(_, next)) = result.first() { curr_entry = next; }
        }

        for l in (0..=level.min(self.max_level)).rev() {
            let candidates = self.search_layer(&query_vec, curr_entry, l, EF_CONSTR);
            let max_conn = if l == 0 { M0 } else { M };
            while self.nodes[node_idx as usize].connections.len() <= l {
                self.nodes[node_idx as usize].connections.push(Vec::new());
            }
            let n_conn = max_conn.min(candidates.len());
            for i in 0..n_conn {
                let neighbor = candidates[i].1;
                self.nodes[node_idx as usize].connections[l].push(neighbor);
                while self.nodes[neighbor as usize].connections.len() <= l {
                    self.nodes[neighbor as usize].connections.push(Vec::new());
                }
                self.nodes[neighbor as usize].connections[l].push(node_idx);
                if self.nodes[neighbor as usize].connections[l].len() > max_conn * 2 {
                    self.shrink_connections(neighbor, l);
                }
            }
            if !candidates.is_empty() { curr_entry = candidates[0].1; }
        }

        if level > self.max_level { self.max_level = level; self.enter_point = Some(node_idx); }
    }

    fn shrink_connections(&mut self, node_id: u32, level: usize) {
        let max_conn = if level == 0 { M0 } else { M };
        let neighbors = self.nodes[node_id as usize].connections[level].clone();
        let mut scored: Vec<(f32, u32)> = neighbors.iter()
            .map(|&n| {
                let d = self.l2_dist(&self.nodes[node_id as usize].vector, &self.nodes[n as usize].vector);
                (d, n)
            }).collect();
        self.dist_calls += neighbors.len() as u64;
        scored.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        self.nodes[node_id as usize].connections[level] = scored.iter().take(max_conn).map(|&(_, n)| n).collect();
    }

    pub fn search(&mut self, query: &[f32], k: usize) -> Vec<(f32, u32)> {
        if self.is_empty() { return Vec::new(); }
        let ep = self.enter_point.unwrap();
        let mut curr_entry = ep;
        for l in (1..=self.max_level).rev() {
            let result = self.search_layer(query, curr_entry, l, 1);
            if let Some(&(_, next)) = result.first() { curr_entry = next; }
        }
        let candidates = self.search_layer(query, curr_entry, 0, EF_SEARCH);
        let mut results: Vec<(f32, u32)> = candidates.into_iter().take(k).collect();
        results.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        results
    }

    pub fn stats(&self) -> String {
        let total: usize = self.nodes.iter().map(|n| n.connections.iter().map(|c| c.len()).sum::<usize>()).sum();
        alloc::format!("[HNSW] {} nodes, {} lvls, {} conns, {} dist",
            self.nodes.len(), self.max_level + 1, total, self.dist_calls)
    }
}
