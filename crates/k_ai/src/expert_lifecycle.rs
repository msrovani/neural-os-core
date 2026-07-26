use alloc::vec::Vec;
use alloc::string::String;

#[derive(Debug, Clone)]
pub struct ExpertMetadata {
    pub id: u64,
    pub name: String,
    pub birth_tick: u64,
    pub hits: u64,
    pub avg_confidence: f32,
    pub entropy: f32,
    /// Bits per weight (quantização): 1, 2, 4, 8, 16, 32
    pub bits_per_weight: u8,
    pub last_active: u64,
}

pub struct ExpertLifecycleManager {
    experts: Vec<ExpertMetadata>,
    next_id: u64,
    prune_threshold_ticks: u64,
    merge_entropy_threshold: f32,
    split_entropy_threshold: f32,
}

impl ExpertLifecycleManager {
    pub fn new() -> Self {
        ExpertLifecycleManager {
            experts: Vec::new(),
            next_id: 1,
            prune_threshold_ticks: 10_000,
            merge_entropy_threshold: 0.3,
            split_entropy_threshold: 0.8,
        }
    }

    pub fn register(&mut self, name: String, bits_per_weight: u8, current_tick: u64) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.experts.push(ExpertMetadata {
            id,
            name,
            birth_tick: current_tick,
            hits: 0,
            avg_confidence: 0.0,
            entropy: 0.0,
            bits_per_weight,
            last_active: current_tick,
        });
        id
    }

    pub fn record_hit(&mut self, id: u64, confidence: f32, current_tick: u64) {
        if let Some(e) = self.experts.iter_mut().find(|e| e.id == id) {
            e.hits += 1;
            e.avg_confidence = e.avg_confidence + (confidence - e.avg_confidence) / (e.hits as f32);
            e.last_active = current_tick;
        }
    }

    pub fn update_entropy(&mut self, id: u64, entropy: f32) {
        if let Some(e) = self.experts.iter_mut().find(|e| e.id == id) {
            e.entropy = entropy;
        }
    }

    pub fn candidates_for_merge(&self, current_tick: u64) -> Vec<(usize, usize)> {
        let mut candidates = Vec::new();
        let active: Vec<usize> = self.experts.iter().enumerate()
            .filter(|(_, e)| current_tick - e.last_active < self.prune_threshold_ticks)
            .map(|(i, _)| i)
            .collect();

        for i in 0..active.len() {
            for j in i + 1..active.len() {
                let ei = &self.experts[active[i]];
                let ej = &self.experts[active[j]];
                if ei.entropy < self.merge_entropy_threshold
                    && ej.entropy < self.merge_entropy_threshold
                    && ei.bits_per_weight == ej.bits_per_weight
                {
                    candidates.push((active[i], active[j]));
                }
            }
        }
        candidates
    }

    pub fn candidates_for_split(&self, _current_tick: u64) -> Vec<usize> {
        self.experts.iter().enumerate()
            .filter(|(_, e)| e.entropy > self.split_entropy_threshold)
            .map(|(i, _)| i)
            .collect()
    }

    pub fn stale_experts(&self, current_tick: u64) -> Vec<usize> {
        self.experts.iter().enumerate()
            .filter(|(_, e)| current_tick - e.last_active >= self.prune_threshold_ticks && e.hits < 5)
            .map(|(i, _)| i)
            .collect()
    }

    pub fn remove_expert(&mut self, idx: usize) {
        if idx < self.experts.len() {
            self.experts.remove(idx);
        }
    }

    pub fn get(&self, id: u64) -> Option<&ExpertMetadata> {
        self.experts.iter().find(|e| e.id == id)
    }

    pub fn count(&self) -> usize {
        self.experts.len()
    }

    pub fn all(&self) -> &[ExpertMetadata] {
        &self.experts
    }
}
