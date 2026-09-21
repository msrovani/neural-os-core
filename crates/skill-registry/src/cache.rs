//! OutputCache — só para skills com `idempotent: true` (caller deve checar).
//! TTL em ticks; cap de entradas; hits/misses reais; evict_expired.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

const MAX_ENTRIES: usize = 64;

struct CacheEntry {
    output: Vec<u8>,
    expires_at_tick: u64,
}

pub struct OutputCache {
    cache: BTreeMap<u64, CacheEntry>,
    default_ttl: u64,
    hits: u64,
    misses: u64,
}

impl OutputCache {
    pub fn new(default_ttl: u64) -> Self {
        OutputCache {
            cache: BTreeMap::new(),
            default_ttl,
            hits: 0,
            misses: 0,
        }
    }

    fn hash(name: &str, payload: &[u8]) -> u64 {
        let mut h: u64 = 5381;
        for b in name.bytes() {
            h = h.wrapping_mul(33).wrapping_add(b as u64);
        }
        for b in payload.iter() {
            h = h.wrapping_mul(33).wrapping_add(*b as u64);
        }
        h
    }

    /// Hit se presente e não expirado. Atualiza hits/misses.
    pub fn get(&mut self, name: &str, payload: &[u8], now: u64) -> Option<Vec<u8>> {
        let key = Self::hash(name, payload);
        if let Some(entry) = self.cache.get(&key) {
            if now < entry.expires_at_tick {
                self.hits = self.hits.saturating_add(1);
                return Some(entry.output.clone());
            }
        }
        self.misses = self.misses.saturating_add(1);
        None
    }

    /// Armazena output. Caller deve garantir idempotent. Cap MAX_ENTRIES (drop oldest key).
    pub fn set(&mut self, name: &str, payload: &[u8], output: Vec<u8>, now: u64, ttl: Option<u64>) {
        if self.cache.len() >= MAX_ENTRIES {
            self.evict_expired(now);
        }
        while self.cache.len() >= MAX_ENTRIES {
            if let Some(&k) = self.cache.keys().next() {
                self.cache.remove(&k);
            } else {
                break;
            }
        }
        let key = Self::hash(name, payload);
        let ttl = ttl.unwrap_or(self.default_ttl);
        self.cache.insert(
            key,
            CacheEntry {
                output,
                expires_at_tick: now.saturating_add(ttl),
            },
        );
    }

    pub fn evict_expired(&mut self, now: u64) {
        self.cache.retain(|_, entry| now < entry.expires_at_tick);
    }

    pub fn stats(&self) -> (u64, u64) {
        (self.hits, self.misses)
    }

    pub fn len(&self) -> usize {
        self.cache.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn miss_then_hit() {
        let mut c = OutputCache::new(100);
        assert!(c.get("a", b"1", 0).is_none());
        c.set("a", b"1", b"out".to_vec(), 0, None);
        assert_eq!(c.get("a", b"1", 50).as_deref(), Some(&b"out"[..]));
        let (h, m) = c.stats();
        assert_eq!(m, 1);
        assert_eq!(h, 1);
    }

    #[test]
    fn expired_is_miss() {
        let mut c = OutputCache::new(10);
        c.set("a", b"1", b"x".to_vec(), 0, Some(5));
        assert!(c.get("a", b"1", 10).is_none());
    }
}
