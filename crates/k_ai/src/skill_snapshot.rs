//! Tool-State Save Game (IDEA #315.15).
//! Snapshot + rollback de estado de skills.
//! Permite salvar o estado de uma skill antes de executar uma operação
//! arriscada e restaurar se algo der errado.

use alloc::vec::Vec;
use alloc::string::String;
use alloc::collections::BTreeMap;

/// Snapshot do estado de uma skill.
#[derive(Debug, Clone)]
pub struct SkillSnapshot {
    pub skill_name: String,
    pub timestamp: u64,
    pub state_data: Vec<u8>,       // Serialized state
    pub metadata: BTreeMap<String, String>,
}

/// Gerenciador de snapshots de skills.
pub struct SkillSnapshotManager {
    snapshots: BTreeMap<String, Vec<SkillSnapshot>>,
    max_per_skill: usize,
}

impl SkillSnapshotManager {
    pub fn new() -> Self {
        Self {
            snapshots: BTreeMap::new(),
            max_per_skill: 5, // Keep last 5 snapshots per skill
        }
    }

    /// Cria um snapshot do estado atual de uma skill.
    pub fn snapshot(&mut self, skill_name: &str, state: &[u8], tick: u64) {
        let entry = self.snapshots.entry(String::from(skill_name)).or_insert_with(Vec::new);

        if entry.len() >= self.max_per_skill {
            entry.remove(0);
        }

        entry.push(SkillSnapshot {
            skill_name: String::from(skill_name),
            timestamp: tick,
            state_data: state.to_vec(),
            metadata: {
                let mut m = BTreeMap::new();
                m.insert(String::from("created_at"), alloc::format!("{}", tick));
                m
            },
        });
    }

    /// Restaura o snapshot mais recente de uma skill.
    pub fn restore(&mut self, skill_name: &str) -> Option<SkillSnapshot> {
        self.snapshots.get_mut(skill_name).and_then(|entries| entries.pop())
    }

    /// Lista snapshots disponíveis para uma skill.
    pub fn list_snapshots(&self, skill_name: &str) -> Vec<&SkillSnapshot> {
        self.snapshots.get(skill_name).map(|entries| entries.iter().collect()).unwrap_or_default()
    }

    /// Número total de snapshots gerenciados.
    pub fn total_snapshots(&self) -> usize {
        self.snapshots.values().map(|v| v.len()).sum()
    }
}
