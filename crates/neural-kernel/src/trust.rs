use alloc::collections::BTreeMap;
use alloc::string::String;

#[derive(Debug, Clone)]
pub struct TrustEntry {
    pub granted_at_ticks: u64,
    pub ttl_ticks: u64,
}

pub struct TrustCache {
    entries: BTreeMap<(u64, String), TrustEntry>,
    denylist: BTreeMap<(u64, String), ()>,
}

impl TrustCache {
    pub fn new() -> Self {
        TrustCache {
            entries: BTreeMap::new(),
            denylist: BTreeMap::new(),
        }
    }

    pub fn trust_allow(&mut self, token: u64, skill_name: &str, now_ticks: u64) {
        let key = (token, String::from(skill_name));
        self.denylist.remove(&key);
        self.entries.insert(key, TrustEntry {
            granted_at_ticks: now_ticks,
            ttl_ticks: u64::MAX,
        });
    }

    pub fn trust_deny(&mut self, token: u64, skill_name: &str) {
        let key = (token, String::from(skill_name));
        self.entries.remove(&key);
        self.denylist.insert(key, ());
    }

    pub fn is_trusted(&self, token: u64, skill_name: &str, now_ticks: u64) -> bool {
        let key = &(token, String::from(skill_name));
        if self.denylist.contains_key(key) {
            return false;
        }
        if let Some(entry) = self.entries.get(key) {
            if now_ticks.saturating_sub(entry.granted_at_ticks) <= entry.ttl_ticks {
                return true;
            }
        }
        false
    }

    pub fn check_or_cache(&mut self, token: u64, skill_name: &str, now_ticks: u64, ttl_ticks: u64) -> bool {
        if self.is_trusted(token, skill_name, now_ticks) {
            return true;
        }
        let key = (token, String::from(skill_name));
        if self.denylist.contains_key(&key) {
            return false;
        }
        self.entries.insert(key, TrustEntry {
            granted_at_ticks: now_ticks,
            ttl_ticks,
        });
        true
    }
}
