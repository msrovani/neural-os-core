use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use crate::memory_tree::{MemoryTree, MemTier, MemNode};

pub struct AtkinsonShiffrin {
    pub sensory: Vec<SensoryItem>,
    pub stm: MemoryTree,
    pub ltm: MemoryTree,
    pub tick: u64,
}

pub struct SensoryItem {
    pub data: Vec<u8>,
    pub tag: String,
    pub decay_ticks: u64,
    pub birth: u64,
}

const SENSORY_TTL: u64 = 2880;
const STM_HALF_LIFE: u64 = 10080;
const LTM_HALF_LIFE: u64 = 86400;

impl AtkinsonShiffrin {
    pub fn new(tick: u64) -> Self {
        AtkinsonShiffrin {
            sensory: Vec::new(),
            stm: MemoryTree::new("STM", tick),
            ltm: MemoryTree::new("LTM", tick),
            tick,
        }
    }

    pub fn sense(&mut self, data: &[u8], tag: &str) {
        self.sensory.push(SensoryItem {
            data: data.to_vec(), tag: String::from(tag),
            decay_ticks: SENSORY_TTL, birth: self.tick,
        });
        if self.sensory.len() > 64 { self.sensory.remove(0); }
    }

    pub fn attend(&mut self, tag: &str, summary: &str, importance: u8) -> Option<usize> {
        let pos = self.sensory.iter().position(|s| s.tag == tag)?;
        let item = self.sensory.remove(pos);
        self.stm.add(self.stm.root, summary, &item.data, importance, MemTier::Working, STM_HALF_LIFE)
    }

    pub fn tick_cycle(&mut self) {
        self.stm.tick = self.tick; self.ltm.tick = self.tick;
        self.sensory.retain(|s| self.tick < s.birth + s.decay_ticks);
        self.stm.consolidate(STM_HALF_LIFE);
        self.ltm.consolidate(LTM_HALF_LIFE);
    }

    pub fn promote_to_ltm(&mut self, stm_idx: usize, summary: &str) -> Option<usize> {
        let data = self.stm.get(stm_idx).map(|n| n.data.clone())?;
        let imp = self.stm.get(stm_idx).map(|n| n.importance).unwrap_or(0);
        self.ltm.add(self.ltm.root, summary, &data, imp, MemTier::Semantic, LTM_HALF_LIFE)
    }

    pub fn recall(&self, query: &str) -> Vec<&str> {
        let mut results = Vec::new();
        for (_, s, _, _) in self.ltm.scout(self.ltm.root, 3) { results.push(s); }
        for (_, s, _, _) in self.stm.scout(self.stm.root, 2) { results.push(s); }
        results
    }
}
