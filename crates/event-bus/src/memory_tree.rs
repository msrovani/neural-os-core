use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

const MAX_CHUNK: usize = 1024;
const MAX_CHILDREN: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MemTier { Working, Episodic, Semantic, Procedural }

#[derive(Debug)]
pub struct MemNode {
    pub summary: String,
    pub data: Vec<u8>,
    pub children: Vec<usize>,
    pub importance: u8,
    pub tier: MemTier,
    pub ttl_ticks: u64,
    pub birth_tick: u64,
    pub access_count: u32,
    pub last_access_tick: u64,
}

impl MemNode {
    pub fn ebbinghaus_strength(&self, now: u64, half_life: u64) -> f32 {
        let age = if now > self.birth_tick { now - self.birth_tick } else { 0 };
        let decay = libm::expf(-(age as f32) / (half_life as f32));
        (self.importance as f32) * decay * (1.0 + (self.access_count as f32) * 0.2)
    }

    pub fn is_expired(&self, now: u64) -> bool {
        self.ttl_ticks > 0 && now > self.birth_tick + self.ttl_ticks
    }

    pub fn should_promote(&self, now: u64, half_life: u64) -> Option<MemTier> {
        let s = self.ebbinghaus_strength(now, half_life);
        match self.tier {
            MemTier::Working if s > 2.0 => Some(MemTier::Episodic),
            MemTier::Episodic if self.access_count > 5 => Some(MemTier::Semantic),
            MemTier::Semantic if self.access_count > 15 => Some(MemTier::Procedural),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct MemoryTree {
    pub nodes: Vec<MemNode>,
    pub root: usize,
    pub tick: u64,
}

impl MemoryTree {
    pub fn new(root_summary: &str, tick: u64) -> Self {
        MemoryTree {
            nodes: vec![MemNode {
                summary: String::from(root_summary), data: Vec::new(),
                children: Vec::new(), importance: 0, tier: MemTier::Working,
                ttl_ticks: 0, birth_tick: tick, access_count: 0, last_access_tick: tick,
            }],
            root: 0, tick,
        }
    }

    pub fn add(&mut self, parent: usize, summary: &str, data: &[u8],
               importance: u8, tier: MemTier, ttl: u64) -> Option<usize> {
        if parent >= self.nodes.len() || self.nodes[parent].children.len() >= MAX_CHILDREN { return None; }
        if data.len() > MAX_CHUNK { return None; }
        let idx = self.nodes.len();
        let now = self.tick;
        self.nodes.push(MemNode {
            summary: String::from(summary), data: data.to_vec(),
            children: Vec::new(), importance, tier,
            ttl_ticks: ttl, birth_tick: now, access_count: 0, last_access_tick: now,
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

    pub fn access(&mut self, idx: usize) {
        if let Some(n) = self.nodes.get_mut(idx) {
            n.access_count += 1; n.last_access_tick = self.tick;
        }
    }

    pub fn scout(&self, idx: usize, depth: usize) -> Vec<(usize, &str, u8, MemTier)> {
        let mut result = Vec::new();
        if let Some(n) = self.nodes.get(idx) {
            result.push((idx, n.summary.as_str(), n.importance, n.tier));
            if depth > 0 {
                for &c in &n.children { result.extend(self.scout(c, depth - 1)); }
            }
        }
        result
    }

    pub fn consolidate(&mut self, half_life: u64) {
        self.consolidate_inner(self.root, half_life);
    }

    fn consolidate_inner(&mut self, idx: usize, half_life: u64) -> bool {
        if idx >= self.nodes.len() { return false; }
        let (is_expired, should_promote, importance, tier) = {
            let node = &self.nodes[idx];
            let now = self.tick;
            let expired = node.is_expired(now);
            let promote = node.should_promote(now, half_life);
            (expired, promote, node.importance, node.tier)
        };
        if is_expired { return true; }
        if let Some(new_tier) = should_promote { self.nodes[idx].tier = new_tier; }
        let mut i = 0;
        while i < self.nodes[idx].children.len() {
            let c = self.nodes[idx].children[i];
            if self.consolidate_inner(c, half_life) { self.nodes[idx].children.remove(i); }
            else { i += 1; }
        }
        importance < 3 && tier == MemTier::Working && self.nodes[idx].children.is_empty()
    }

    pub fn prune(&mut self, min_importance: u8) {
        self.prune_inner(self.root, min_importance);
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
