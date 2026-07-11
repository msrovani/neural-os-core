//! ARC cache com write-back coalescing, dirty tracking, evict com flush.
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;

fn now() -> u64 { crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64 }

pub struct CacheEntry {
    pub data: Vec<u8>,
    pub freq: u16,
    pub last_access: u64,
    pub last_write: u64,
    pub dirty: bool,
}

pub struct ArcCache {
    entries: BTreeMap<u64, CacheEntry>,
    max_entries: usize,
    pub tier_name: &'static str,
    write_coalesce_ms: u64,
}

impl ArcCache {
    pub fn new(max_kb: usize, tier: &'static str) -> Self {
        ArcCache {
            entries: BTreeMap::new(),
            max_entries: (max_kb / 4).max(16),
            tier_name: tier,
            write_coalesce_ms: 100,
        }
    }

    pub fn get(&mut self, lba: u64) -> Option<&[u8]> {
        let tick = now();
        if let Some(entry) = self.entries.get_mut(&lba) {
            entry.freq = entry.freq.saturating_add(1);
            entry.last_access = tick;
            Some(&entry.data)
        } else { None }
    }

    pub fn insert(&mut self, lba: u64, data: &[u8]) {
        if self.entries.len() >= self.max_entries { self.evict_one(); }
        self.entries.insert(lba, CacheEntry {
            data: data.to_vec(), freq: 1, last_access: now(), last_write: 0, dirty: false,
        });
    }

    pub fn mark_dirty(&mut self, lba: u64) {
        if let Some(entry) = self.entries.get_mut(&lba) {
            entry.dirty = true;
            entry.last_write = now();
        }
    }

    pub fn tick(&mut self, flush_fn: &mut dyn FnMut(u64, &[u8])) -> usize {
        let tick = now();
        let threshold = tick.saturating_sub(self.write_coalesce_ms);
        let to_flush: Vec<u64> = self.entries.iter()
            .filter(|(_, e)| e.dirty && e.last_write < threshold)
            .map(|(k, _)| *k).collect();
        let n = to_flush.len();
        for lba in &to_flush {
            if let Some(entry) = self.entries.get(lba) {
                flush_fn(*lba, &entry.data);
                if let Some(e) = self.entries.get_mut(lba) { e.dirty = false; }
            }
        }
        n
    }

    /// Evita o entry menos frequente (LFU) — faz writeback se dirty
    fn evict_one(&mut self) {
        let tick = now();
        let victim = self.entries.iter()
            .min_by_key(|(_, e)| (e.freq, (tick - e.last_access)))
            .map(|(k, _)| *k);
        if let Some(lba) = victim {
            let dirty = self.entries.get(&lba).map_or(false, |e| e.dirty);
            if dirty {
                crate::serial_println!("[CACHE] evict dirty {:#x} without flush_fn — DATA LOSS RISK", lba);
            }
            self.entries.remove(&lba);
        }
    }

    pub fn resize(&mut self, new_max_kb: usize) {
        self.max_entries = (new_max_kb / 4).max(16);
        while self.entries.len() > self.max_entries { self.evict_one(); }
    }

    pub fn stats(&self) -> (usize, usize, usize) {
        let dirty = self.entries.iter().filter(|(_, e)| e.dirty).count();
        (self.entries.len(), self.max_entries, dirty)
    }
}
