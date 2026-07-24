use core::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionTier {
    Binary1bit,
    Ternary2bit,
    Int4,
    Int5,
    Int8,
    Bf16,
    F32,
}

impl CompressionTier {
    pub fn bits_per_weight(&self) -> usize {
        match self {
            CompressionTier::Binary1bit => 1,
            CompressionTier::Ternary2bit => 2,
            CompressionTier::Int4 => 4,
            CompressionTier::Int5 => 5,
            CompressionTier::Int8 => 8,
            CompressionTier::Bf16 => 16,
            CompressionTier::F32 => 32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TierPolicy {
    Conservative,
    Balanced,
    Performance,
}

impl TierPolicy {
    pub fn suggest_tier(&self, importance: f32, age: u64, pressure: f32) -> CompressionTier {
        match self {
            TierPolicy::Conservative => {
                if pressure > 0.8 { CompressionTier::Ternary2bit }
                else if age > 1000 || importance < 0.3 { CompressionTier::Ternary2bit }
                else { CompressionTier::Bf16 }
            }
            TierPolicy::Balanced => {
                if pressure > 0.8 { CompressionTier::Binary1bit }
                else if importance > 0.8 { CompressionTier::Bf16 }
                else if pressure > 0.5 { CompressionTier::Ternary2bit }
                else { CompressionTier::Int8 }
            }
            TierPolicy::Performance => {
                if importance > 0.5 { CompressionTier::F32 }
                else { CompressionTier::Bf16 }
            }
        }
    }
}

pub struct BudgetManager {
    max_memory_bytes: usize,
    used_memory_bytes: AtomicUsize,
    tier_policy: TierPolicy,
    temperature: f32,
}

impl BudgetManager {
    pub fn new(max_memory_bytes: usize, tier_policy: TierPolicy) -> Self {
        BudgetManager {
            max_memory_bytes,
            used_memory_bytes: AtomicUsize::new(0),
            tier_policy,
            temperature: 0.3,
        }
    }

    pub fn can_promote(&self, current: CompressionTier, target: CompressionTier, size: usize) -> bool {
        if target.bits_per_weight() <= current.bits_per_weight() {
            return true;
        }
        let delta = (target.bits_per_weight() - current.bits_per_weight()) * size / 8;
        let used = self.used_memory_bytes.load(Ordering::Relaxed);
        used + delta <= self.max_memory_bytes
    }

    pub fn suggest_tier(&self, importance: f32, age: u64) -> CompressionTier {
        let pressure = self.pressure();
        self.tier_policy.suggest_tier(importance, age, pressure)
    }

    pub fn pressure(&self) -> f32 {
        let used = self.used_memory_bytes.load(Ordering::Relaxed) as f32;
        let max = self.max_memory_bytes as f32;
        if max == 0.0 { 1.0 } else { (used / max).min(1.0) }
    }

    pub fn allocate(&self, bytes: usize) -> bool {
        let used = self.used_memory_bytes.load(Ordering::Relaxed);
        if used + bytes > self.max_memory_bytes {
            return false;
        }
        self.used_memory_bytes.store(used + bytes, Ordering::Relaxed);
        true
    }

    pub fn deallocate(&self, bytes: usize) {
        let used = self.used_memory_bytes.load(Ordering::Relaxed);
        self.used_memory_bytes.store(used.saturating_sub(bytes), Ordering::Relaxed);
    }

    pub fn used_bytes(&self) -> usize {
        self.used_memory_bytes.load(Ordering::Relaxed)
    }

    pub fn max_bytes(&self) -> usize {
        self.max_memory_bytes
    }

    pub fn set_temperature(&mut self, t: f32) {
        self.temperature = t.clamp(0.0, 1.0);
    }

    pub fn temperature(&self) -> f32 {
        self.temperature
    }
}
