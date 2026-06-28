use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

const MAX_CHUNK: usize = 512;
const MAX_CHILDREN: usize = 16;

#[derive(Debug)]
pub struct MemNode {
    pub summary: String,
    pub data: Vec<u8>,
    pub children: Vec<usize>,
    pub importance: u8,
}

#[derive(Debug)]
pub struct MemoryTree {
    pub nodes: Vec<MemNode>,
    pub root: usize,
}

impl MemoryTree {
    pub fn new(root_summary: &str) -> Self {
        MemoryTree {
            nodes: vec![MemNode {
                summary: String::from(root_summary),
                data: Vec::new(),
                children: Vec::new(),
                importance: 0,
            }],
            root: 0,
        }
    }

    pub fn add(&mut self, parent: usize, summary: &str, data: &[u8], importance: u8) -> Option<usize> {
        if parent >= self.nodes.len() { return None; }
        if self.nodes[parent].children.len() >= MAX_CHILDREN { return None; }
        if data.len() > MAX_CHUNK { return None; }
        let idx = self.nodes.len();
        self.nodes.push(MemNode {
            summary: String::from(summary),
            data: data.to_vec(),
            children: Vec::new(),
            importance,
        });
        self.nodes[parent].children.push(idx);
        Some(idx)
    }

    pub fn get(&self, idx: usize) -> Option<&MemNode> {
        self.nodes.get(idx)
    }

    pub fn get_mut(&mut self, idx: usize) -> Option<&mut MemNode> {
        self.nodes.get_mut(idx)
    }

    pub fn scout(&self, idx: usize, depth: usize) -> Vec<(usize, &str, u8)> {
        let mut result = Vec::new();
        if let Some(n) = self.nodes.get(idx) {
            result.push((idx, n.summary.as_str(), n.importance));
            if depth > 0 {
                for &c in &n.children {
                    result.extend(self.scout(c, depth - 1));
                }
            }
        }
        result
    }

    pub fn prune(&mut self, min_importance: u8) {
        self.prune_inner(0, min_importance);
    }

    fn prune_inner(&mut self, idx: usize, min: u8) -> bool {
        if idx >= self.nodes.len() { return false; }
        let mut i = 0;
        while i < self.nodes[idx].children.len() {
            let c = self.nodes[idx].children[i];
            if self.prune_inner(c, min) { self.nodes[idx].children.remove(i); }
            else { i += 1; }
        }
        self.nodes[idx].importance < min && self.nodes[idx].children.is_empty()
    }
}
