//! Bounded channel (SPSC-style mailbox) — ADR-0068 / Labor 9.
//! TicketLock + VecDeque; capacidade fixa; não substitui EventBus.

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use ticket_lock::TicketLock;

/// Canal bounded: um produtor / um consumidor lógicos (API single-queue).
pub struct BoundedChannel<T> {
    inner: Arc<TicketLock<VecDeque<T>>>,
    capacity: usize,
}

impl<T> BoundedChannel<T> {
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.max(1);
        Self {
            inner: Arc::new(TicketLock::new(VecDeque::with_capacity(cap))),
            capacity: cap,
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

    /// Enfileira; Err se cheio.
    pub fn send(&self, item: T) -> Result<(), &'static str> {
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
