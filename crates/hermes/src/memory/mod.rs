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
            MemoryLevel::L7 => 1024 * 1024 * 1024, // H7: finito — evict em vez de órfão
        }
    }

    pub fn promote_threshold(&self) -> u64 {
        match self {
            MemoryLevel::L0 => 3,   // hit 3x per tick → L1
            MemoryLevel::L1 => 5,   // hit 5x → L2
            MemoryLevel::L2 => 8,   // hit 8x → L3
            MemoryLevel::L3 => 12,
            MemoryLevel::L4 => 20,
            MemoryLevel::L5 => 30,
            MemoryLevel::L6 => 50,
            MemoryLevel::L7 => u64::MAX,
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

struct MemoryEntry {
    data: Vec<u8>,
    access_count: u64,
    last_access_tick: u64,
    birth_tick: u64,
}

impl MemoryEntry {
    fn new(data: Vec<u8>, tick: u64) -> Self {
        MemoryEntry { data, access_count: 1, last_access_tick: tick, birth_tick: tick }
    }
}

pub trait MemoryTier {
    fn read(&mut self, key: &str, tick: u64) -> Option<(Vec<u8>, u64)>;
    fn write(&mut self, key: &str, value: &[u8], tick: u64);
    fn remove(&mut self, key: &str);
    fn level(&self) -> MemoryLevel;
    fn used_bytes(&self) -> usize;
    fn entry_count(&self) -> usize;
    fn evict_one(&mut self) -> Option<String>;
    fn evict_expired(&mut self, tick: u64) -> usize;
    fn clear(&mut self);
    fn contains(&self, key: &str) -> bool;
    /// Entry mais "quente" (maior access_count) para batch promotion.
    /// Default: None — tiers sem índice de calor não promovem em batch.
    fn promotion_candidate(&self) -> Option<(String, u64)> { None }
}

pub struct InMemoryTier {
    level: MemoryLevel,
    map: BTreeMap<String, MemoryEntry>,
    max_bytes: usize,
    used: usize,
}

impl InMemoryTier {
    pub fn new(level: MemoryLevel) -> Self {
        InMemoryTier { level, map: BTreeMap::new(), max_bytes: level.capacity(), used: 0 }
    }
}

impl MemoryTier for InMemoryTier {
    fn read(&mut self, key: &str, tick: u64) -> Option<(Vec<u8>, u64)> {
        self.map.get_mut(key).map(|entry| {
            entry.access_count += 1;
            entry.last_access_tick = tick;
            (entry.data.clone(), entry.access_count)
        })
    }

    fn write(&mut self, key: &str, value: &[u8], tick: u64) {
        if let Some(entry) = self.map.get_mut(key) {
            let old = entry.data.len();
            entry.data = value.to_vec();
            self.used = self.used - old + entry.data.len();
            entry.access_count += 1;
            entry.last_access_tick = tick;
            return;
        }
        let bytes = value.len();
        if self.max_bytes < usize::MAX {
            // M19: contador incremental — sem sum() O(n) por iteração.
            while self.used + bytes > self.max_bytes {
                if self.evict_one().is_none() { break; }
            }
        }
        self.used += bytes;
        self.map.insert(key.into(), MemoryEntry::new(value.to_vec(), tick));
    }

    fn remove(&mut self, key: &str) {
        if let Some(e) = self.map.remove(key) { self.used -= e.data.len(); }
    }

    fn level(&self) -> MemoryLevel { self.level }

    fn used_bytes(&self) -> usize { self.used }

    fn entry_count(&self) -> usize { self.map.len() }

    fn evict_one(&mut self) -> Option<String> {
        let target = self.map.iter()
            .min_by_key(|(_, e)| (e.last_access_tick, core::cmp::Reverse(e.access_count)))
            .map(|(k, _)| k.clone());
        if let Some(ref k) = target {
            if let Some(e) = self.map.remove(k) { self.used -= e.data.len(); }
        }
        target
    }

    fn evict_expired(&mut self, tick: u64) -> usize {
        let ttl = self.level.ttl_ticks();
        if ttl == u64::MAX { return 0; }
        let keys: Vec<String> = self.map.iter()
            .filter(|(_, e)| tick - e.last_access_tick > ttl)
            .map(|(k, _)| k.clone())
            .collect();
        let n = keys.len();
        for k in keys {
            if let Some(e) = self.map.remove(&k) { self.used -= e.data.len(); }
        }
        n
    }

    fn clear(&mut self) { self.map.clear(); self.used = 0; }

    fn contains(&self, key: &str) -> bool { self.map.contains_key(key) }

    fn promotion_candidate(&self) -> Option<(String, u64)> {
        self.map.iter()
            .max_by_key(|(_, e)| e.access_count)
            .map(|(k, e)| (k.clone(), e.access_count))
    }
}

pub struct MemoryStore {
    tiers: [Option<Box<dyn MemoryTier>>; 8],
    promote_on_read: bool,
    tick: u64,
}

impl MemoryStore {
    pub fn new() -> Self {
        MemoryStore { tiers: Default::default(), promote_on_read: true, tick: 0 }
    }

    pub fn init_default_tiers(&mut self) {
        for lv in 0..8 {
            if let Ok(level) = MemoryLevel::try_from(lv) {
                self.tiers[lv] = Some(Box::new(InMemoryTier::new(level)));
            }
        }
    }

    pub fn read(&mut self, key: &str) -> Option<Vec<u8>> {
        let found = self.find_level(key);
        let (lv, data, count) = found?;
        if self.promote_on_read && lv > 0
            && count >= MemoryLevel::try_from(lv).ok().map_or(u64::MAX, |l| l.promote_threshold())
        {
            let higher = lv - 1;
            if let Some(ref mut dest) = self.tiers[higher] {
                dest.write(key, &data, self.tick);
            }
            if let Some(ref mut src) = self.tiers[lv] {
                src.remove(key);
            }
        }
        Some(data)
    }

    fn find_level(&mut self, key: &str) -> Option<(usize, Vec<u8>, u64)> {
        for lv in 0..8 {
            if let Some(ref mut tier) = self.tiers[lv] {
                if let Some((data, count)) = tier.read(key, self.tick) {
                    return Some((lv, data, count));
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

    pub fn tick_advance(&mut self) {
        self.tick += 1;
        // H7: L0 não é clear() cego — eviction por TTL (ttl_ticks=1) como os demais.
        if self.tick % 10 == 0 {
            for lv in 0..8 {
                if let Some(ref mut tier) = self.tiers[lv] {
                    tier.evict_expired(self.tick);
                }
            }
        }
        // auto-promote every 50 ticks
        if self.tick % 50 == 0 {
            self.auto_promote();
        }
    }

    fn auto_promote(&mut self) {
        // H7: batch promotion — entry quente sobe um tier (lv → lv-1, L0 é o mais
        // quente; espelha a promoção read-time). Sem dado inventado: threshold real.
        for lv in 1..8 {
            let candidate = self.tiers[lv].as_ref().and_then(|t| t.promotion_candidate());
            let (key, count) = match candidate { Some(c) => c, None => continue };
            let threshold = MemoryLevel::try_from(lv).ok().map_or(u64::MAX, |l| l.promote_threshold());
            if count < threshold { continue; }
            if let Some((data, _)) = self.tiers[lv].as_mut().and_then(|t| t.read(&key, self.tick)) {
                if let Some(dest) = self.tiers[lv - 1].as_mut() {
                    dest.write(&key, &data, self.tick);
                }
                if let Some(src) = self.tiers[lv].as_mut() {
                    src.remove(&key);
                }
            }
        }
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






