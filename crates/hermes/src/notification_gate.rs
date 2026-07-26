//! Notification Gate (IDEA #315.4).
//! Gerencia notificações com 4 níveis de urgência, rate limiting e dedup.

use alloc::vec::Vec;
use alloc::string::String;
use alloc::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Urgency {
    Debug,
    Info,
    High,
    Critical,
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub id: u64,
    pub urgency: Urgency,
    pub title: String,
    pub message: String,
    pub source: String,
    pub expires_at: u64,
}

pub struct NotificationGate {
    queue: VecDeque<Notification>,
    max_visible: usize,
    ttl_ticks: u64,
    next_id: u64,
    /// Dedup cache: hash of (title, message) → tick
    dedup: Vec<(u64, u64)>,
}

impl NotificationGate {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            max_visible: 4,
            ttl_ticks: 300,
            next_id: 1,
            dedup: Vec::new(),
        }
    }

    pub fn push(
        &mut self,
        urgency: Urgency,
        title: &str,
        message: &str,
        source: &str,
        now: u64,
    ) -> bool {
        // Dedup check
        let hash = Self::hash_notif(title, message);
        if self
            .dedup
            .iter()
            .any(|&(h, t)| h == hash && now.wrapping_sub(t) < 30)
        {
            return false; // dedup'd
        }
        // Rate limit
        let recent = self
            .queue
            .iter()
            .filter(|n| now.wrapping_sub(n.expires_at) < self.ttl_ticks)
            .count();
        if recent > 5 {
            return false; // rate limited
        }

        self.dedup.push((hash, now));
        if self.dedup.len() > 64 {
            self.dedup.remove(0);
        }

        let expires = match urgency {
            Urgency::Critical => u64::MAX,
            _ => now.saturating_add(
                self.ttl_ticks
                    * match urgency {
                        Urgency::Debug => 2,
                        Urgency::Info => 1,
                        Urgency::High => 3,
                        _ => 1,
                    },
            ),
        };
        self.queue.push_back(Notification {
            id: self.next_id,
            urgency,
            title: String::from(title),
            message: String::from(message),
            source: String::from(source),
            expires_at: expires,
        });
        self.next_id += 1;
        true
    }

    pub fn prune(&mut self, now: u64) {
        self.queue.retain(|n| n.expires_at > now);
    }

    pub fn visible(&self) -> &VecDeque<Notification> {
        &self.queue
    }

    fn hash_notif(title: &str, msg: &str) -> u64 {
        let mut h: u64 = 0;
        for b in title.bytes().chain(msg.bytes()) {
            h = h.wrapping_mul(31).wrapping_add(b as u64);
        }
        h
    }
}
