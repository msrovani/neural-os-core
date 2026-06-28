use alloc::string::String;
use alloc::vec::Vec;
use crate::kgraph::{Graph, NodeKind};
use crate::memory_tree::{MemoryTree, MemTier};

pub struct SuperContext {
    pub memory: MemoryTree,
    pub kg: Graph,
}

impl SuperContext {
    pub fn new(tick: u64) -> Self {
        SuperContext {
            memory: MemoryTree::new("SuperContext", tick),
            kg: Graph::new(),
        }
    }

    pub fn scout_context(&self, agent: &str, depth: usize) -> Vec<String> {
        let mut ctx = Vec::new();
        if let Some(node) = self.kg.get(agent) {
            ctx.push(alloc::format!("[Agent: {}]", node.label));
            for (_, label) in self.kg.neighbors(node.id) {
                ctx.push(alloc::format!("  relates: {}", label));
            }
        }
        for (_, summary, imp, tier) in self.memory.scout(self.memory.root, depth) {
            ctx.push(alloc::format!("  [{}][{}] {}", 
                match tier { MemTier::Working => "W", MemTier::Episodic => "E",
                             MemTier::Semantic => "S", MemTier::Procedural => "P" },
                imp, summary));
        }
        ctx
    }

    pub fn ingest(&mut self, agent: &str, skill: &str, data: &[u8], importance: u8, tick: u64) {
        self.memory.tick = tick;
        let idx = self.kg.add_node(NodeKind::Agent, agent);
        let skill_idx = self.kg.add_node(NodeKind::Skill, skill);
        self.kg.add_edge(idx, skill_idx, "executes");
        self.memory.add(self.memory.root, &alloc::format!("{}:{}", agent, skill),
                        data, importance, MemTier::Working, 1000);
    }
}
