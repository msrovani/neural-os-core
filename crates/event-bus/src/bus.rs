//! EventBus — topic pub/sub with bounded per-subscriber queues (AIOS honesty).
//!
//! Contracts (SESSION_375 / ADR-0088 / SESSION_352):
//! - Per-queue depth cap + drop_oldest (never unbounded VecDeque).
//! - Stream topics (AUDIO_*) use a tighter cap.
//! - Zombie subscribers (Receiver dropped) pruned via Arc::strong_count.
//! - `publish` returns Ok(delivered) — Ok(0) = nobody listening (not Err).

use alloc::collections::btree_map::Entry;
use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use ticket_lock::{TicketLock, TicketLockGuard};

use crate::event::Event;

/// s437: aquisição BOUNDED. Um `TicketLock` é não-reentrante: se um IRQ (ou o
/// BSP) chama `lock()` enquanto o MESMO core já segura o lock (ex.: `publish`
/// no thread interrompido por um IRQ que também publica/consome), `serving`
/// nunca avança e o core gira para SEMPRE. Confirmado por QEMU-monitor: BSP
/// preso em `TicketLock<VecDeque<Event>>::lock`. O bus é best-effort (filas
/// bounded + drop-oldest): dar spin-limit e DESISTIR mantém o OS vivo (AIOS
/// fail-closed mas vivo) em vez de congelar o scheduler.
const LOCK_SPIN_BUDGET: u32 = 256;

fn lock_bounded<T>(l: &TicketLock<T>) -> Option<TicketLockGuard<'_, T>> {
    for _ in 0..LOCK_SPIN_BUDGET {
        if let Some(g) = l.try_lock() {
            return Some(g);
        }
        core::hint::spin_loop();
    }
    None
}

/// Default depth for control-plane topics (BOOT_PHASE, HEALTH_ISSUE, …).
pub const DEFAULT_QUEUE_DEPTH: usize = 64;
/// High-rate PCM/control streams must not grow without bound (SESSION_352).
pub const STREAM_QUEUE_DEPTH: usize = 8;

fn queue_depth_for(topic: &str) -> usize {
    match topic {
        "AUDIO_FRAME" | "AUDIO_IN" | "AUDIO_OUT" => STREAM_QUEUE_DEPTH,
        _ => DEFAULT_QUEUE_DEPTH,
    }
}

fn push_bounded(q: &mut VecDeque<Event>, event: Event, depth: usize) {
    while q.len() >= depth {
        let _ = q.pop_front();
    }
    q.push_back(event);
}

pub struct Receiver {
    topic: String,
    queue: Arc<TicketLock<VecDeque<Event>>>,
}

impl Receiver {
    pub fn try_receive(&self) -> Option<Event> {
        lock_bounded(&self.queue).and_then(|mut g| g.pop_front())
    }

    pub fn has_pending(&self) -> bool {
        lock_bounded(&self.queue).map(|g| !g.is_empty()).unwrap_or(false)
    }

    pub fn topic(&self) -> &str {
        &self.topic
    }

    pub fn pending_len(&self) -> usize {
        lock_bounded(&self.queue).map(|g| g.len()).unwrap_or(0)
    }
}

pub struct EventBus {
    subscribers: TicketLock<BTreeMap<String, Vec<Arc<TicketLock<VecDeque<Event>>>>>>,
    next_event_id: AtomicU64,
}

impl EventBus {
    pub fn new() -> Self {
        EventBus {
            subscribers: TicketLock::new(BTreeMap::new()),
            next_event_id: AtomicU64::new(1),
        }
    }

    pub fn subscribe(&self, topic: &str) -> Receiver {
        let depth = queue_depth_for(topic);
        let queue = Arc::new(TicketLock::new(VecDeque::with_capacity(depth)));
        let mut subs = self.subscribers.lock();
        match subs.entry(String::from(topic)) {
            Entry::Occupied(mut o) => {
                o.get_mut().push(queue.clone());
            }
            Entry::Vacant(v) => {
                v.insert(vec![queue.clone()]);
            }
        }
        Receiver {
            topic: String::from(topic),
            queue,
        }
    }

    /// Remove this receiver's queue from the topic map (explicit lifecycle).
    pub fn unsubscribe(&self, rx: &Receiver) {
        let mut subs = self.subscribers.lock();
        if let Some(queues) = subs.get_mut(&rx.topic) {
            queues.retain(|q| !Arc::ptr_eq(q, &rx.queue));
            if queues.is_empty() {
                let topic = rx.topic.clone();
                subs.remove(&topic);
            }
        }
    }

    /// Publish. Ok(n) = delivered to n live subscribers; Ok(0) = no listeners.
    /// Err only on invalid capability token.
    pub fn publish(&self, mut event: Event) -> Result<usize, &'static str> {
        if !event.token.is_valid() {
            return Err("token de capacidade invalido");
        }
        event.id = self.next_event_id.fetch_add(1, Ordering::Relaxed);
        let depth = queue_depth_for(&event.topic);
        let topic_key = event.topic.clone();
        // s437: bounded — publish pode ser chamado de IRQ; bloquear no map lock
        // (enquanto um thread o segura) gira para sempre no mesmo core.
        let Some(mut subs) = lock_bounded(&self.subscribers) else {
            return Ok(0); // bus ocupado (IRQ vs thread) — dropar é seguro
        };
        let mut delivered = 0usize;
        let mut empty_after = false;
        if let Some(queues) = subs.get_mut(&topic_key) {
            // Zombie prune: Receiver dropped ⇒ strong_count == 1 (map only).
            queues.retain(|q| Arc::strong_count(q) > 1);
            for q in queues.iter() {
                if let Some(mut g) = lock_bounded(q) {
                    push_bounded(&mut g, event.clone(), depth);
                    delivered = delivered.saturating_add(1);
                }
            }
            empty_after = queues.is_empty();
        }
        if empty_after {
            subs.remove(&topic_key);
        }
        Ok(delivered)
    }

    /// Live subscriber count for a topic (after zombie prune).
    pub fn subscriber_count(&self, topic: &str) -> usize {
        let Some(mut subs) = lock_bounded(&self.subscribers) else {
            return 0; // ocupado — n/a ≠ 0, mas evita spin (HUD tolera)
        };
        let Some(queues) = subs.get_mut(topic) else {
            return 0;
        };
        queues.retain(|q| Arc::strong_count(q) > 1);
        let n = queues.len();
        if n == 0 {
            let key = String::from(topic);
            subs.remove(&key);
            return 0;
        }
        n
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::CapabilityToken;

    fn ev(topic: &str, payload: &[u8]) -> Event {
        Event {
            id: 0,
            topic: String::from(topic),
            payload: payload.to_vec(),
            token: CapabilityToken::Legacy(1),
        }
    }

    #[test]
    fn publish_no_subscribers_returns_zero() {
        let bus = EventBus::new();
        assert_eq!(bus.publish(ev("T", b"x")).unwrap(), 0);
    }

    #[test]
    fn invalid_token_denied() {
        let bus = EventBus::new();
        let e = Event {
            id: 0,
            topic: String::from("T"),
            payload: b"x".to_vec(),
            token: CapabilityToken::Legacy(0),
        };
        assert!(bus.publish(e).is_err());
    }

    #[test]
    fn drop_oldest_when_full() {
        let bus = EventBus::new();
        let rx = bus.subscribe("BOOT_PHASE");
        for i in 0..(DEFAULT_QUEUE_DEPTH + 3) {
            let _ = bus.publish(ev("BOOT_PHASE", &[i as u8]));
        }
        assert_eq!(rx.pending_len(), DEFAULT_QUEUE_DEPTH);
        let first = rx.try_receive().unwrap();
        assert_eq!(first.payload[0], 3); // 0,1,2 dropped
    }

    #[test]
    fn stream_topic_uses_tight_cap() {
        let bus = EventBus::new();
        let rx = bus.subscribe("AUDIO_FRAME");
        for i in 0..(STREAM_QUEUE_DEPTH + 5) {
            let _ = bus.publish(ev("AUDIO_FRAME", &[i as u8]));
        }
        assert_eq!(rx.pending_len(), STREAM_QUEUE_DEPTH);
    }

    #[test]
    fn zombie_receiver_pruned() {
        let bus = EventBus::new();
        let rx = bus.subscribe("HEALTH_ISSUE");
        assert_eq!(bus.publish(ev("HEALTH_ISSUE", b"a")).unwrap(), 1);
        drop(rx);
        assert_eq!(bus.publish(ev("HEALTH_ISSUE", b"b")).unwrap(), 0);
        assert_eq!(bus.subscriber_count("HEALTH_ISSUE"), 0);
    }

    #[test]
    fn unsubscribe_removes_queue() {
        let bus = EventBus::new();
        let rx = bus.subscribe("CARD_ACTION");
        bus.unsubscribe(&rx);
        assert_eq!(bus.publish(ev("CARD_ACTION", b"c")).unwrap(), 0);
    }
}
