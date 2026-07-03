use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;

fn now() -> u64 { crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64 }

pub struct CacheEntry {
    pub data: Vec<u8>,
    pub freq: u16,
    pub last_access: u64,
    pub dirty: bool,
}

pub struct ArcCache {
    entries: BTreeMap<u64, CacheEntry>,
    capacity: usize,
}

impl ArcCache {
    pub fn new(capacity_kb: usize) -> Self {
        ArcCache { entries: BTreeMap::new(), capacity: capacity_kb / 4 }
    }

    pub fn get(&mut self, lba: u64) -> Option<&[u8]> {
        let tick = now();
        if let Some(entry) = self.entries.get_mut(&lba) {
            entry.freq = entry.freq.saturating_add(1);
            entry.last_access = tick;
            Some(&entry.data)
        } else {
            None
        }
    }

    pub fn insert(&mut self, lba: u64, data: &[u8]) {
        let tick = now();
        if self.entries.len() >= self.capacity { self.evict_one(); }
        self.entries.insert(lba, CacheEntry {
            data: data.to_vec(), freq: 1, last_access: tick, dirty: false,
        });
    }

    pub fn mark_dirty(&mut self, lba: u64) {
        if let Some(entry) = self.entries.get_mut(&lba) { entry.dirty = true; }
    }

    pub fn tick(&mut self, flush_fn: &mut dyn FnMut(u64, &[u8])) {
        let tick = now();
        let to_flush: Vec<u64> = self.entries.iter()
            .filter(|(_, e)| e.dirty && tick - e.last_access > 500)
            .map(|(k, _)| *k).collect();
        for lba in &to_flush {
            if let Some(entry) = self.entries.get(lba) {
                flush_fn(*lba, &entry.data);
                if let Some(e) = self.entries.get_mut(lba) { e.dirty = false; }
            }
        }
    }

    fn evict_one(&mut self) {
        let tick = now();
        let victim = self.entries.iter()
            .min_by_key(|(_, e)| (e.freq as i32 * -1, ((tick - e.last_access) as i32) * -1))
            .map(|(k, _)| *k);
        if let Some(lba) = victim {
            if let Some(entry) = self.entries.remove(&lba) {
                if entry.dirty {
                    crate::serial_println!("[CACHE] Evict dirty LBA {}", lba);
                }
            }
        }
    }

    pub fn stats(&self) -> (usize, usize) { (self.entries.len(), self.capacity) }
}
