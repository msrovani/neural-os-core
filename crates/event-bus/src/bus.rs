use alloc::collections::btree_map::Entry;
use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use crate::event::Event;

pub struct Receiver {
    queue: Arc<Mutex<VecDeque<Event>>>,
}

impl Receiver {
    pub fn try_receive(&self) -> Option<Event> {
        self.queue.lock().pop_front()
    }
}

pub struct EventBus {
    subscribers: Mutex<BTreeMap<String, Vec<Arc<Mutex<VecDeque<Event>>>>>>,
    #[allow(dead_code)]
    next_event_id: Mutex<u64>,
}

impl EventBus {
    pub fn new() -> Self {
        EventBus {
            subscribers: Mutex::new(BTreeMap::new()),
            next_event_id: Mutex::new(1),
        }
    }

    pub fn subscribe(&self, topic: &str) -> Receiver {
        let queue = Arc::new(Mutex::new(VecDeque::new()));
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
        let mut id_lock = self.next_event_id.lock();
        event.id = *id_lock;
        *id_lock += 1;
        drop(id_lock);
        let subs = self.subscribers.lock();
        if let Some(queues) = subs.get(&event.topic) {
            for q in queues {
                q.lock().push_back(event.clone());
            }
        }
        Ok(())
    }
}
