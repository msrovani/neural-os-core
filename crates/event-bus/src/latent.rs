//! LatentBus — canal paralelo ao EventBus para hidden states [f16; 256] (ADR-0047 Pilar 1).
//! Coexiste com EventBus textual; zero deps novas.
//!
//! Honesty (SESSION_375): bounded queues + drop_oldest; `recv_count` só em try_receive
//! (nunca no enqueue); zombies podados via Arc::strong_count.

use alloc::collections::btree_map::Entry;
use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use ticket_lock::TicketLock;

use crate::capability::CapabilityToken;

pub const LATENT_DIM: usize = 256;
pub const TOPIC_THOUGHT_LLM: &str = "THOUGHT_LLM";
/// Latent vectors are control-plane sized; keep shallow to avoid heap growth.
pub const LATENT_QUEUE_DEPTH: usize = 16;

/// Packed IEEE-754 binary16 (u16 bits).
pub type F16Bits = u16;

#[derive(Clone)]
pub struct LatentPacket {
    pub id: u64,
    pub topic: String,
    pub vec: [F16Bits; LATENT_DIM],
    pub token: CapabilityToken,
    /// L2-ish norm proxy (f32 bits as u32 for serial/debug).
    pub norm_bits: u32,
}

fn push_bounded(q: &mut VecDeque<LatentPacket>, packet: LatentPacket, depth: usize) {
    while q.len() >= depth {
        let _ = q.pop_front();
    }
    q.push_back(packet);
}

pub struct LatentReceiver {
    topic: String,
    queue: Arc<TicketLock<VecDeque<LatentPacket>>>,
    /// Shared counter — incremented only on successful try_receive.
    recv_count: Arc<AtomicU64>,
}

impl LatentReceiver {
    pub fn try_receive(&self) -> Option<LatentPacket> {
        let pkt = self.queue.lock().pop_front()?;
        self.recv_count.fetch_add(1, Ordering::Relaxed);
        Some(pkt)
    }

    pub fn has_pending(&self) -> bool {
        !self.queue.lock().is_empty()
    }
}

pub struct LatentBus {
    subscribers: TicketLock<BTreeMap<String, Vec<Arc<TicketLock<VecDeque<LatentPacket>>>>>>,
    next_id: AtomicU64,
    pub publish_count: AtomicU64,
    /// Actual receives (try_receive), not enqueue fan-out.
    pub recv_count: Arc<AtomicU64>,
}

impl LatentBus {
    pub fn new() -> Self {
        LatentBus {
            subscribers: TicketLock::new(BTreeMap::new()),
            next_id: AtomicU64::new(1),
            publish_count: AtomicU64::new(0),
            recv_count: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn subscribe(&self, topic: &str) -> LatentReceiver {
        let queue = Arc::new(TicketLock::new(VecDeque::with_capacity(LATENT_QUEUE_DEPTH)));
        let mut subs = self.subscribers.lock();
        match subs.entry(String::from(topic)) {
            Entry::Occupied(mut o) => {
                o.get_mut().push(queue.clone());
            }
            Entry::Vacant(v) => {
                v.insert(vec![queue.clone()]);
            }
        }
        LatentReceiver {
            topic: String::from(topic),
            queue,
            recv_count: Arc::clone(&self.recv_count),
        }
    }

    pub fn unsubscribe(&self, rx: &LatentReceiver) {
        let mut subs = self.subscribers.lock();
        if let Some(queues) = subs.get_mut(&rx.topic) {
            queues.retain(|q| !Arc::ptr_eq(q, &rx.queue));
            if queues.is_empty() {
                let topic = rx.topic.clone();
                subs.remove(&topic);
            }
        }
    }

    /// Ok(n) = delivered to n live subscribers; Ok(0) = nobody listening.
    pub fn publish(&self, mut packet: LatentPacket) -> Result<usize, &'static str> {
        if !packet.token.is_valid() {
            return Err("token de capacidade invalido");
        }
        packet.id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.publish_count.fetch_add(1, Ordering::Relaxed);
        let topic_key = packet.topic.clone();
        let mut subs = self.subscribers.lock();
        let mut delivered = 0usize;
        let mut empty_after = false;
        if let Some(queues) = subs.get_mut(&topic_key) {
            queues.retain(|q| Arc::strong_count(q) > 1);
            for q in queues.iter() {
                push_bounded(&mut q.lock(), packet.clone(), LATENT_QUEUE_DEPTH);
                delivered = delivered.saturating_add(1);
            }
            empty_after = queues.is_empty();
        }
        if empty_after {
            subs.remove(&topic_key);
        }
        Ok(delivered)
    }

    pub fn stats(&self) -> (u64, u64) {
        (
            self.publish_count.load(Ordering::Relaxed),
            self.recv_count.load(Ordering::Relaxed),
        )
    }
}

impl Default for LatentBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkt(topic: &str) -> LatentPacket {
        LatentPacket {
            id: 0,
            topic: String::from(topic),
            vec: [0; LATENT_DIM],
            token: CapabilityToken::Legacy(1),
            norm_bits: 0,
        }
    }

    #[test]
    fn recv_count_only_on_try_receive() {
        let bus = LatentBus::new();
        let rx = bus.subscribe(TOPIC_THOUGHT_LLM);
        assert_eq!(bus.publish(pkt(TOPIC_THOUGHT_LLM)).unwrap(), 1);
        let (p, r) = bus.stats();
        assert_eq!(p, 1);
        assert_eq!(r, 0); // not yet received
        assert!(rx.try_receive().is_some());
        let (_, r2) = bus.stats();
        assert_eq!(r2, 1);
    }

    #[test]
    fn zombie_pruned() {
        let bus = LatentBus::new();
        let rx = bus.subscribe(TOPIC_THOUGHT_LLM);
        assert_eq!(bus.publish(pkt(TOPIC_THOUGHT_LLM)).unwrap(), 1);
        drop(rx);
        assert_eq!(bus.publish(pkt(TOPIC_THOUGHT_LLM)).unwrap(), 0);
    }
}
