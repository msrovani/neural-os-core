//! Lock-free bounded MPMC queue (Vyukov CAS algorithm).
//! Substitui TicketLock<VecDeque<T>> no caminho quente do EventBus.
//! Zero dependencies — implementação standalone (não depende de k_nano).

use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};

/// Capacidade padrão (power of 2).
const DEFAULT_CAPACITY: usize = 1024;

/// Slot com sequence number (ABA-safe via monotonic sequences).
struct Slot<T: Copy> {
    sequence: AtomicUsize,
    data: UnsafeCell<T>,
}

unsafe impl<T: Copy> Send for Slot<T> {}
unsafe impl<T: Copy> Sync for Slot<T> {}

/// Bounded MPMC queue com CAS. T: Copy + Default.
///
/// **Invariantes:**
/// - Capacidade é sempre power of 2 (bitmask para indexação).
/// - Sequence number monotônico previne ABA.
/// - Cada slot sincroniza produtor↔consumidor via sequence gap.
pub struct LockFreeQueue<T: Copy + Default> {
    buffer: Vec<Slot<T>>,
    capacity: usize,
    mask: usize,
    enqueue_pos: AtomicUsize,
    dequeue_pos: AtomicUsize,
}

unsafe impl<T: Copy + Default> Send for LockFreeQueue<T> {}
unsafe impl<T: Copy + Default> Sync for LockFreeQueue<T> {}

impl<T: Copy + Default> LockFreeQueue<T> {
    /// Cria queue com capacidade `cap` (arredonda para power of 2).
    pub fn new(cap: usize) -> Self {
        let cap = cap.max(1).next_power_of_two();
        let mut buffer = Vec::with_capacity(cap);
        for i in 0..cap {
            buffer.push(Slot {
                sequence: AtomicUsize::new(i),
                data: UnsafeCell::new(T::default()),
            });
        }
        Self {
            buffer,
            capacity: cap,
            mask: cap - 1,
            enqueue_pos: AtomicUsize::new(0),
            dequeue_pos: AtomicUsize::new(0),
        }
    }

    /// Enfileira (spin CAS). Retorna `true` se OK, `false` se cheia.
    /// Spin é seguro em no_std sem preempção — produtor avança ou cede.
    pub fn enqueue(&self, item: T) -> bool {
        let mut pos = self.enqueue_pos.load(Ordering::Relaxed);
        loop {
            let slot = &self.buffer[pos & self.mask];
            let seq = slot.sequence.load(Ordering::Acquire);
            let diff = seq as isize - pos as isize;
            if diff == 0 {
                // Slot livre — CAS para avançar
                if self.enqueue_pos.compare_exchange_weak(
                    pos,
                    pos.wrapping_add(1),
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ).is_ok() {
                    unsafe { *slot.data.get() = item; }
                    slot.sequence.store(pos.wrapping_add(1), Ordering::Release);
                    return true;
                }
                // CAS falhou — recarrega posição
                pos = self.enqueue_pos.load(Ordering::Relaxed);
            } else if diff < 0 {
                return false; // cheia
            } else {
                // Avançou — recarrega
                pos = self.enqueue_pos.load(Ordering::Relaxed);
            }
        }
    }

    /// Non-blocking enqueue (uma tentativa).
    pub fn try_enqueue(&self, item: T) -> bool {
        let pos = self.enqueue_pos.load(Ordering::Acquire);
        let slot = &self.buffer[pos & self.mask];
        let seq = slot.sequence.load(Ordering::Acquire);
        if seq as isize - pos as isize == 0 {
            if self.enqueue_pos.compare_exchange_weak(
                pos,
                pos.wrapping_add(1),
                Ordering::Relaxed,
                Ordering::Relaxed,
            ).is_ok() {
                unsafe { *slot.data.get() = item; }
                slot.sequence.store(pos.wrapping_add(1), Ordering::Release);
                return true;
            }
        }
        false
    }

    /// Desenfileira. Retorna Some(item) ou None se vazia.
    pub fn dequeue(&self) -> Option<T> {
        let mut pos = self.dequeue_pos.load(Ordering::Relaxed);
        loop {
            let slot = &self.buffer[pos & self.mask];
            let seq = slot.sequence.load(Ordering::Acquire);
            let diff = seq as isize - (pos.wrapping_add(1)) as isize;
            if diff == 0 {
                if self.dequeue_pos.compare_exchange_weak(
                    pos,
                    pos.wrapping_add(1),
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ).is_ok() {
                    let item = unsafe { *slot.data.get() };
                    slot.sequence.store(
                        pos.wrapping_add(self.capacity),
                        Ordering::Release,
                    );
                    return Some(item);
                }
                pos = self.dequeue_pos.load(Ordering::Relaxed);
            } else if diff < 0 {
                return None; // vazia
            } else {
                pos = self.dequeue_pos.load(Ordering::Relaxed);
            }
        }
    }

    /// Non-blocking dequeue (uma tentativa).
    pub fn try_dequeue(&self) -> Option<T> {
        let pos = self.dequeue_pos.load(Ordering::Acquire);
        let slot = &self.buffer[pos & self.mask];
        let seq = slot.sequence.load(Ordering::Acquire);
        if seq as isize - (pos.wrapping_add(1)) as isize == 0 {
            if self.dequeue_pos.compare_exchange_weak(
                pos,
                pos.wrapping_add(1),
                Ordering::Relaxed,
                Ordering::Relaxed,
            ).is_ok() {
                let item = unsafe { *slot.data.get() };
                slot.sequence.store(
                    pos.wrapping_add(self.capacity),
                    Ordering::Release,
                );
                return Some(item);
            }
        }
        None
    }

    /// Comprimento aproximado (lock-free snapshot).
    pub fn len(&self) -> usize {
        let enq = self.enqueue_pos.load(Ordering::Acquire);
        let deq = self.dequeue_pos.load(Ordering::Acquire);
        enq.wrapping_sub(deq).min(self.capacity)
    }

    pub fn is_empty(&self) -> bool {
        self.enqueue_pos.load(Ordering::Acquire)
            == self.dequeue_pos.load(Ordering::Acquire)
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_thread_enqueue_dequeue() {
        let q = LockFreeQueue::<u64>::new(8);
        assert!(q.is_empty());
        assert!(q.enqueue(42));
        assert!(!q.is_empty());
        assert_eq!(q.dequeue(), Some(42));
        assert!(q.is_empty());
    }

    #[test]
    fn fill_and_drain() {
        let q = LockFreeQueue::<u32>::new(4);
        for i in 0..4 {
            assert!(q.enqueue(i));
        }
        assert!(!q.enqueue(99)); // cheia
        assert_eq!(q.len(), 4);
        for i in 0..4 {
            assert_eq!(q.dequeue(), Some(i));
        }
        assert!(q.is_empty());
    }

    #[test]
    fn wrap_around() {
        let q = LockFreeQueue::<u8>::new(4);
        for cycle in 0..20u8 {
            assert!(q.enqueue(cycle));
            assert_eq!(q.dequeue(), Some(cycle));
        }
        assert!(q.is_empty());
    }

    #[test]
    fn try_enqueue_dequeue() {
        let q = LockFreeQueue::<u16>::new(2);
        assert!(q.try_enqueue(10));
        assert!(q.try_enqueue(20));
        assert!(!q.try_enqueue(30)); // cheia
        assert_eq!(q.try_dequeue(), Some(10));
        assert_eq!(q.try_dequeue(), Some(20));
        assert_eq!(q.try_dequeue(), None);
    }
}
