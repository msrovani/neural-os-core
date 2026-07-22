use alloc::vec::Vec;
use alloc::string::String;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MemoryLevel {
    L0 = 0, L1 = 1, L2 = 2, L3 = 3, L4 = 4, L5 = 5, L6 = 6, L7 = 7,
}

impl MemoryLevel {
    pub fn capacity(&self) -> usize {
        match self {
            MemoryLevel::L0 => 1024,
            MemoryLevel::L1 => 64 * 1024,
            MemoryLevel::L2 => 1024 * 1024,
            MemoryLevel::L3 => 16 * 1024 * 1024,
            MemoryLevel::L4 => 64 * 1024 * 1024,
            MemoryLevel::L5 => 128 * 1024 * 1024,
            MemoryLevel::L6 => 256 * 1024 * 1024,
            MemoryLevel::L7 => usize::MAX,
        }
    }

    pub fn latency_ns(&self) -> u64 {
        match self {
            MemoryLevel::L0 => 1,
            MemoryLevel::L1 => 10,
            MemoryLevel::L2 => 100,
            MemoryLevel::L3 => 1_000_000,
            MemoryLevel::L4 => 10_000_000,
            MemoryLevel::L5 => 100_000_000,
            MemoryLevel::L6 => 1_000_000_000,
            MemoryLevel::L7 => 10_000_000_000,
        }
    }

    pub fn ttl_ticks(&self) -> u64 {
        match self {
            MemoryLevel::L0 => 1,
            MemoryLevel::L1 => 10,
            MemoryLevel::L2 => 600,
            MemoryLevel::L3 => 6000,
            MemoryLevel::L4 => 60000,
            MemoryLevel::L5 => u64::MAX,
            MemoryLevel::L6 => u64::MAX,
            MemoryLevel::L7 => u64::MAX,
        }
    }
}

struct MemoryEntry {
    data: Vec<u8>,
    access_count: u64,
    last_access_tick: u64,
    birth_tick: u64,
    level: MemoryLevel,
}

impl MemoryEntry {
    fn new(data: Vec<u8>, level: MemoryLevel, tick: u64) -> Self {
        MemoryEntry { data, access_count: 1, last_access_tick: tick, birth_tick: tick, level }
    }
}

pub trait MemoryTier {
    fn read(&mut self, key: &str, tick: u64) -> Option<Vec<u8>>;
    fn write(&mut self, key: &str, value: &[u8], tick: u64);
    fn level(&self) -> MemoryLevel;
    fn used_bytes(&self) -> usize;
    fn entry_count(&self) -> usize;
    fn evict_one(&mut self) -> Option<String>;
    fn contains(&self, key: &str) -> bool;
}

pub struct InMemoryTier {
    level: MemoryLevel,
    map: BTreeMap<String, MemoryEntry>,
    max_bytes: usize,
}

impl InMemoryTier {
    pub fn new(level: MemoryLevel) -> Self {
        InMemoryTier { level, map: BTreeMap::new(), max_bytes: level.capacity() }
    }
}

impl MemoryTier for InMemoryTier {
    fn read(&mut self, key: &str, tick: u64) -> Option<Vec<u8>> {
        if let Some(entry) = self.map.get_mut(key) {
            entry.access_count += 1;
            entry.last_access_tick = tick;
            Some(entry.data.clone())
        } else {
            None
        }
    }

    fn write(&mut self, key: &str, value: &[u8], tick: u64) {
        let bytes = value.len();
        if let Some(entry) = self.map.get_mut(key) {
            entry.data = value.to_vec();
            entry.access_count += 1;
            entry.last_access_tick = tick;
        } else {
            while self.used_bytes() + bytes > self.max_bytes && self.max_bytes < usize::MAX {
                if self.evict_one().is_none() { break; }
            }
            self.map.insert(key.into(), MemoryEntry::new(value.to_vec(), self.level, tick));
        }
    }

    fn level(&self) -> MemoryLevel { self.level }

    fn used_bytes(&self) -> usize {
        self.map.values().map(|e| e.data.len()).sum()
    }

    fn entry_count(&self) -> usize { self.map.len() }

    fn evict_one(&mut self) -> Option<String> {
        let oldest = self.map.iter()
            .min_by_key(|(_, e)| e.last_access_tick)
            .map(|(k, _)| k.clone());
        if let Some(ref key) = oldest {
            self.map.remove(key);
        }
        oldest
    }

    fn contains(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }
}

pub struct MemoryStore {
    tiers: [Option<Box<dyn MemoryTier>>; 8],
    promote_on_read: bool,
    tick: u64,
    promote_threshold: u64,
}

impl MemoryStore {
    pub fn new() -> Self {
        MemoryStore {
            tiers: Default::default(),
            promote_on_read: true,
            tick: 0,
            promote_threshold: 3,
        }
    }

    pub fn init_default_tiers(&mut self) {
        let levels = [
            MemoryLevel::L0, MemoryLevel::L1, MemoryLevel::L2,
            MemoryLevel::L3, MemoryLevel::L4, MemoryLevel::L5,
            MemoryLevel::L6, MemoryLevel::L7,
        ];
        for &lv in &levels {
            let idx = lv as usize;
            self.tiers[idx] = Some(Box::new(InMemoryTier::new(lv)));
        }
    }

    pub fn read(&mut self, key: &str) -> Option<Vec<u8>> {
        for lv in 0..8 {
            if let Some(ref mut tier) = self.tiers[lv] {
                if let Some(data) = tier.read(key, self.tick) {
                    if self.promote_on_read && lv > 0 {
                            let higher = lv - 1;
                            if let Some(ref mut dest) = self.tiers[higher] {
                            dest.write(key, &data, self.tick);
                        }
                    }
                    return Some(data);
                }
            }
        }
        None
    }

    pub fn write(&mut self, key: &str, value: &[u8], target: MemoryLevel) {
        let idx = target as usize;
        if let Some(ref mut tier) = self.tiers[idx] {
            tier.write(key, value, self.tick);
        }
    }

    pub fn promote(&mut self, key: &str, from: MemoryLevel, to: MemoryLevel) {
        if from == to || from as u8 >= to as u8 { return; }
        let data = self.read(key);
        if let Some(ref d) = data {
            let fi = from as usize;
            let ti = to as usize;
            if let Some(ref mut src) = self.tiers[fi] {
                src.write(key, &[], self.tick); // touch to avoid double-promote
            }
            if let Some(ref mut dst) = self.tiers[ti] {
                dst.write(key, d, self.tick);
            }
        }
    }

    pub fn tick_advance(&mut self) {
        self.tick += 1;
        if self.tick % 100 == 0 {
            self.auto_promote();
        }
    }

    fn auto_promote(&mut self) {
        // ponytail: auto-promotion stub — per-level frequency scanning added when profiling shows need
    }

    pub fn evict_from(&mut self, level: MemoryLevel) -> Option<String> {
        let idx = level as usize;
        self.tiers[idx].as_mut().and_then(|t| t.evict_one())
    }

    pub fn contains(&self, key: &str) -> bool {
        self.tiers.iter().flatten().any(|t| t.contains(key))
    }

    pub fn stats(&self) -> alloc::vec::Vec<(MemoryLevel, usize, usize)> {
        let mut stats = alloc::vec::Vec::new();
        for t in self.tiers.iter().flatten() {
            stats.push((t.level(), t.entry_count(), t.used_bytes()));
        }
        stats
    }
}

impl MemoryLevel {
    pub fn try_from(value: usize) -> Result<MemoryLevel, ()> {
        match value {
            0 => Ok(MemoryLevel::L0),
            1 => Ok(MemoryLevel::L1),
            2 => Ok(MemoryLevel::L2),
            3 => Ok(MemoryLevel::L3),
            4 => Ok(MemoryLevel::L4),
            5 => Ok(MemoryLevel::L5),
            6 => Ok(MemoryLevel::L6),
            7 => Ok(MemoryLevel::L7),
            _ => Err(()),
        }
    }
}
