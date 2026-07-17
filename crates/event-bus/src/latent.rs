//! LatentBus — canal paralelo ao EventBus para hidden states [f16; 256] (ADR-0047 Pilar 1).
//! Coexiste com EventBus textual; zero deps novas.

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

pub struct LatentReceiver {
    queue: Arc<TicketLock<VecDeque<LatentPacket>>>,
}

impl LatentReceiver {
    pub fn try_receive(&self) -> Option<LatentPacket> {
        self.queue.lock().pop_front()
    }

    pub fn has_pending(&self) -> bool {
        !self.queue.lock().is_empty()
    }
}

pub struct LatentBus {
    subscribers: TicketLock<BTreeMap<String, Vec<Arc<TicketLock<VecDeque<LatentPacket>>>>>>,
    next_id: AtomicU64,
    pub publish_count: AtomicU64,
    pub recv_count: AtomicU64,
}

impl LatentBus {
    pub fn new() -> Self {
        LatentBus {
            subscribers: TicketLock::new(BTreeMap::new()),
            next_id: AtomicU64::new(1),
            publish_count: AtomicU64::new(0),
            recv_count: AtomicU64::new(0),
        }
    }

    pub fn subscribe(&self, topic: &str) -> LatentReceiver {
        let queue = Arc::new(TicketLock::new(VecDeque::new()));
        let mut subs = self.subscribers.lock();
        match subs.entry(String::from(topic)) {
            Entry::Occupied(mut o) => {
                o.get_mut().push(queue.clone());
            }
            Entry::Vacant(v) => {
                v.insert(vec![queue.clone()]);
            }
        }
        LatentReceiver { queue }
    }

    pub fn publish(&self, mut packet: LatentPacket) -> Result<(), &'static str> {
        if !packet.token.is_valid() {
            return Err("token de capacidade invalido");
        }
        packet.id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.publish_count.fetch_add(1, Ordering::Relaxed);
        let subs = self.subscribers.lock();
        if let Some(queues) = subs.get(&packet.topic) {
            for q in queues {
                q.lock().push_back(packet.clone());
                self.recv_count.fetch_add(1, Ordering::Relaxed);
            }
        }
        Ok(())
    }

    pub fn stats(&self) -> (u64, u64) {
        (
            self.publish_count.load(Ordering::Relaxed),
            self.recv_count.load(Ordering::Relaxed),
        )
    }
}
