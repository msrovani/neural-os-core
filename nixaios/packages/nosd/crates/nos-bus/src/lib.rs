//! EventBus std — filas bounded (ADAPT event-bus depth 64).

#![forbid(unsafe_code)]

use parking_lot::Mutex;
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

pub const DEFAULT_QUEUE_DEPTH: usize = 64;

#[derive(Clone, Debug)]
pub struct Event {
    pub topic: String,
    pub payload: Value,
}

#[derive(Clone, Default)]
pub struct EventBus {
    inner: Arc<Mutex<HashMap<String, VecDeque<Event>>>>,
    depth: usize,
}

impl EventBus {
    pub fn new(depth: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            depth: depth.max(1),
        }
    }

    pub fn publish(&self, topic: impl Into<String>, payload: Value) {
        let topic = topic.into();
        let mut g = self.inner.lock();
        let q = g.entry(topic.clone()).or_default();
        if q.len() >= self.depth {
            q.pop_front();
        }
        q.push_back(Event { topic, payload });
    }

    pub fn try_recv(&self, topic: &str) -> Option<Event> {
        let mut g = self.inner.lock();
        g.get_mut(topic)?.pop_front()
    }

    pub fn pending(&self, topic: &str) -> usize {
        self.inner.lock().get(topic).map(|q| q.len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_drop_oldest() {
        let bus = EventBus::new(2);
        bus.publish("t", serde_json::json!(1));
        bus.publish("t", serde_json::json!(2));
        bus.publish("t", serde_json::json!(3));
        assert_eq!(bus.pending("t"), 2);
        assert_eq!(bus.try_recv("t").unwrap().payload, 2);
    }
}
