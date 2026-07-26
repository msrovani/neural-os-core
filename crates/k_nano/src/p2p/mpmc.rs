//! MPMC lock-free bounded queue.
//! BEI Onda 0: comunicação inter-agente sem locks.
//! Base: SPSC extendido com CAS multi-produtor/multi-consumidor.

use core::sync::atomic::{AtomicU64, Ordering};
use alloc::vec::Vec;

/// Capacidade padrão do buffer circular (must be power of 2).
const DEFAULT_CAPACITY: usize = 4096;

/// Bounded MPMC queue com ABA-safe sequências.
/// `T` deve ser `Send + Copy` idealmente; para tipos grandes,
/// use `Box<[u8]>` ou índices.
pub struct MpmcQueue<T: Copy + Default> {
    /// Buffer circular com slots pré-alocados
    buffer: Vec<AtomicU64>, // sequence numbers per slot
    data: Vec<core::cell::UnsafeCell<T>>,
    capacity: usize,
    capacity_mask: usize,
    /// Próximo índice a ser escrito (produtor)
    enqueue_pos: AtomicU64,
    /// Próximo índice a ser lido (consumidor)
    dequeue_pos: AtomicU64,
}

unsafe impl<T: Copy + Default> Send for MpmcQueue<T> {}
unsafe impl<T: Copy + Default> Sync for MpmcQueue<T> {}

impl<T: Copy + Default> MpmcQueue<T> {
    /// Cria uma nova MPMC queue com capacidade `capacity` (power of 2).
    pub fn new(capacity: Option<usize>) -> Self {
        let cap = capacity.unwrap_or(DEFAULT_CAPACITY).next_power_of_two();
        let mut buffer = Vec::with_capacity(cap);
        let mut data = Vec::with_capacity(cap);
        for i in 0..cap {
            buffer.push(AtomicU64::new(i as u64));
            data.push(core::cell::UnsafeCell::new(T::default()));
        }
        MpmcQueue {
            buffer,
            data,
            capacity: cap,
            capacity_mask: cap - 1,
            enqueue_pos: AtomicU64::new(0),
            dequeue_pos: AtomicU64::new(0),
        }
    }

    /// Tenta enfileirar um item. Retorna `true` se bem-sucedido.
    /// Bloqueia? Não — spin-lock CAS (lock-free, mas busy-wait).
    /// Use `try_enqueue` para non-blocking.
    pub fn enqueue(&self, item: T) -> bool {
        loop {
            let pos = self.enqueue_pos.load(Ordering::Acquire);
            let slot = pos as usize & self.capacity_mask;
            let seq = self.buffer[slot].load(Ordering::Acquire);
            let diff = seq as i64 - pos as i64;
            if diff == 0 {
                // Slot disponível — tenta CAS
                if self.enqueue_pos.compare_exchange_weak(
                    pos,
                    pos.wrapping_add(1),
                    Ordering::Release,
                    Ordering::Relaxed,
                ).is_ok() {
                    unsafe { *self.data[slot].get() = item; }
                    self.buffer[slot].store(pos.wrapping_add(1), Ordering::Release);
                    return true;
                }
            } else if diff < 0 {
                // Fila cheia
                return false;
            }
            // Outro produtor avançou — tenta de novo (diff > 0)
            // Sem yield em bare-metal; spin é OK em no_std sem preempção.
        }
    }

    /// Non-blocking try_enqueue: retorna false se cheia.
    pub fn try_enqueue(&self, item: T) -> bool {
        let pos = self.enqueue_pos.load(Ordering::Acquire);
        let slot = pos as usize & self.capacity_mask;
        let seq = self.buffer[slot].load(Ordering::Acquire);
        let diff = seq as i64 - pos as i64;
        if diff == 0 {
            if self.enqueue_pos.compare_exchange_weak(
                pos,
                pos.wrapping_add(1),
                Ordering::Release,
                Ordering::Relaxed,
            ).is_ok() {
                unsafe { *self.data[slot].get() = item; }
                self.buffer[slot].store(pos.wrapping_add(1), Ordering::Release);
                return true;
            }
        }
        false
    }

    /// Tenta desenfileirar. Retorna `Some(item)` se bem-sucedido, `None` se vazio.
    pub fn dequeue(&self) -> Option<T> {
        loop {
            let pos = self.dequeue_pos.load(Ordering::Acquire);
            let slot = pos as usize & self.capacity_mask;
            let seq = self.buffer[slot].load(Ordering::Acquire);
            let diff = seq as i64 - pos.wrapping_add(1) as i64;
            if diff == 0 {
                // Slot disponível para leitura
                if self.dequeue_pos.compare_exchange_weak(
                    pos,
                    pos.wrapping_add(1),
                    Ordering::Release,
                    Ordering::Relaxed,
                ).is_ok() {
                    let item = unsafe { *self.data[slot].get() };
                    self.buffer[slot].store(pos.wrapping_add(self.capacity as u64), Ordering::Release);
                    return Some(item);
                }
            } else if diff < 0 {
                // Fila vazia
                return None;
            }
            // Outro consumidor avançou — tenta de novo
        }
    }

    /// Non-blocking try_dequeue.
    pub fn try_dequeue(&self) -> Option<T> {
        let pos = self.dequeue_pos.load(Ordering::Acquire);
        let slot = pos as usize & self.capacity_mask;
        let seq = self.buffer[slot].load(Ordering::Acquire);
        let diff = seq as i64 - pos.wrapping_add(1) as i64;
        if diff == 0 {
            if self.dequeue_pos.compare_exchange_weak(
                pos,
                pos.wrapping_add(1),
                Ordering::Release,
                Ordering::Relaxed,
            ).is_ok() {
                let item = unsafe { *self.data[slot].get() };
                self.buffer[slot].store(pos.wrapping_add(self.capacity as u64), Ordering::Release);
                return Some(item);
            }
        }
        None
    }

    /// Retorna comprimento aproximado (não lock-free exato).
    pub fn len(&self) -> usize {
        let enq = self.enqueue_pos.load(Ordering::Acquire);
        let deq = self.dequeue_pos.load(Ordering::Acquire);
        (enq.wrapping_sub(deq) as usize).min(self.capacity)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_thread() {
        let q = MpmcQueue::new(Some(16));
        assert!(q.is_empty());
        assert_eq!(q.len(), 0);

        assert!(q.enqueue(42));
        assert!(!q.is_empty());
        assert_eq!(q.len(), 1);

        assert_eq!(q.dequeue(), Some(42));
        assert!(q.is_empty());
    }

    #[test]
    fn test_fill_and_drain() {
        let q = MpmcQueue::new(Some(16));
        for i in 0..16 {
            assert!(q.try_enqueue(i));
        }
        // Full
        assert!(!q.try_enqueue(99));
        assert_eq!(q.len(), 16);

        for i in 0..16 {
            assert_eq!(q.dequeue(), Some(i));
        }
        assert!(q.is_empty());
    }

    #[test]
    fn test_wrap_around() {
        let q = MpmcQueue::new(Some(4));
        for _ in 0..10 {
            assert!(q.enqueue(1));
            assert!(q.dequeue().is_some());
        }
        assert!(q.is_empty());
    }
}
