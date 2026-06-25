use alloc::collections::BTreeMap;
use core::sync::atomic::{AtomicU64, Ordering};

pub const DEFAULT_TTL_TICKS: u64 = 1800;

pub struct TrustEntry {
    pub token: u64,
    pub granted_at: u64,
    pub ttl_ticks: u64,
}

impl TrustEntry {
    pub fn is_expired(&self, current_ticks: u64) -> bool {
        current_ticks.saturating_sub(self.granted_at) > self.ttl_ticks
    }
}

pub struct TrustCache {
    entries: BTreeMap<u64, TrustEntry>,
    next_id: AtomicU64,
}

impl TrustCache {
    pub fn new() -> Self {
        TrustCache {
            entries: BTreeMap::new(),
            next_id: AtomicU64::new(1),
        }
    }

    pub fn grant(&mut self, token: u64, current_ticks: u64, ttl_override: Option<u64>) {
        let ttl = ttl_override.unwrap_or(DEFAULT_TTL_TICKS);
        self.entries.insert(token, TrustEntry {
            token,
            granted_at: current_ticks,
            ttl_ticks: ttl,
        });
    }

    pub fn revoke(&mut self, token: u64) {
        self.entries.remove(&token);
    }

    pub fn is_trusted(&self, token: u64, current_ticks: u64) -> bool {
        self.entries.get(&token).map_or(false, |e| !e.is_expired(current_ticks))
    }

    pub fn next_token(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }
}
