//! Bounded channel (SPSC-style mailbox) — ADR-0068 / Labor 9.
//! TicketLock + VecDeque; capacidade fixa; não substitui EventBus.
//!
//! Honesty: capacity 0 → send always Err("capacity_zero") (nunca coerção silenciosa a 1).
//! Clone shares the Arc — multi-producer possible; document as shared mailbox, not pure SPSC.

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use ticket_lock::TicketLock;

/// Canal bounded: API single-queue (Clone ⇒ shared Arc; not exclusive SPSC).
pub struct BoundedChannel<T> {
    inner: Arc<TicketLock<VecDeque<T>>>,
    capacity: usize,
}

impl<T> BoundedChannel<T> {
    /// `capacity == 0` is allowed but send will always fail with `capacity_zero`.
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(TicketLock::new(VecDeque::with_capacity(capacity))),
            capacity,
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn len(&self) -> usize {
        self.inner.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.lock().is_empty()
    }

    /// Enfileira; Err se cheio ou capacity==0.
    pub fn send(&self, item: T) -> Result<(), &'static str> {
        if self.capacity == 0 {
            return Err("capacity_zero");
        }
        let mut q = self.inner.lock();
        if q.len() >= self.capacity {
            return Err("channel_full");
        }
        q.push_back(item);
        Ok(())
    }

    pub fn try_recv(&self) -> Option<T> {
        self.inner.lock().pop_front()
    }
}

impl<T> Clone for BoundedChannel<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            capacity: self.capacity,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacity_zero_refuses_send() {
        let ch = BoundedChannel::<u8>::new(0);
        assert_eq!(ch.send(1), Err("capacity_zero"));
    }

    #[test]
    fn full_returns_err() {
        let ch = BoundedChannel::new(1);
        assert!(ch.send(1).is_ok());
        assert_eq!(ch.send(2), Err("channel_full"));
    }
}
