use alloc::collections::btree_map::Entry;
use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use ticket_lock::TicketLock;

use crate::event::Event;

pub struct Receiver {
    queue: Arc<TicketLock<VecDeque<Event>>>,
}

impl Receiver {
    pub fn try_receive(&self) -> Option<Event> {
        self.queue.lock().pop_front()
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
        Receiver { queue }
    }

    pub fn publish(&self, mut event: Event) -> Result<(), &'static str> {
        if !event.token.is_valid() {
            return Err("token de capacidade invalido");
        }
        event.id = self.next_event_id.fetch_add(1, Ordering::Relaxed);
        let subs = self.subscribers.lock();
        if let Some(queues) = subs.get(&event.topic) {
            for q in queues {
                q.lock().push_back(event.clone());
            }
        }
        Ok(())
    }
}
