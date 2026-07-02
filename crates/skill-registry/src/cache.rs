//! OutputCache — cacheia outputs de skills idempotentes.
//! Skills marcadas com idempotent: true tem seus outputs cacheados
//! por um TTL configurado. Evita re-execucao de skills frequentes.
//! Uso: CortexAgent e HermesAgent consultam o cache antes de executar.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

struct CacheEntry {
    output: Vec<u8>,
    expires_at_tick: u64,
}

pub struct OutputCache {
    cache: BTreeMap<u64, CacheEntry>, // hash(nome+payload) -> entry
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
        for b in name.bytes() { h = h.wrapping_mul(33).wrapping_add(b as u64); }
        for b in payload.iter() { h = h.wrapping_mul(33).wrapping_add(*b as u64); }
        h
    }

    /// Tenta ler do cache. Retorna `Some(output)` se hit e nao expirado.
    pub fn get(&self, name: &str, payload: &[u8], now: u64) -> Option<&[u8]> {
        let key = Self::hash(name, payload);
        if let Some(entry) = self.cache.get(&key) {
            if now < entry.expires_at_tick {
                return Some(&entry.output);
            }
        }
        None
    }

    /// Armazena output no cache
    pub fn set(&mut self, name: &str, payload: &[u8], output: Vec<u8>, now: u64, ttl: Option<u64>) {
        let key = Self::hash(name, payload);
        let ttl = ttl.unwrap_or(self.default_ttl);
        self.cache.insert(key, CacheEntry {
            output,
            expires_at_tick: now + ttl,
        });
    }

    /// Limpa entradas expiradas
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
