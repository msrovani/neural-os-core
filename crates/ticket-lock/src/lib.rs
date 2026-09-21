//! TicketLock — FIFO ticket spinlock bare-metal (`no_std`).
//!
//! # Contrato AIOS (SESSION_316 / SESSION_378)
//! - **Não-reentrante:** `lock()` duas vezes no mesmo contexto = deadlock eterno.
//! - **Não usar em IRQ** enquanto uma thread pode segurar o mesmo lock — use
//!   `k_nano::sync::IrqSafeLock` (cli) ou `try_lock`.
//! - `#[repr(C)]`: `ticket` @0, `serving` @8 (x86_64) — instrumentos leem offsets fixos.
//! - FIFO entre tickets emitidos; **não** é MCS (todos fazem load no mesmo `serving`).

#![cfg_attr(not(test), no_std)]

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicUsize, Ordering};

/// repr(C): ticket/serving em offsets fixos (0/8) — instrumentos de diagnóstico
/// leem os contadores via ponteiro cru; repr(Rust) pode reordenar e o read
/// pegaria bytes do `data` (freeze s325: t/s eram VAs de kernel = data).
#[repr(C)]
pub struct TicketLock<T> {
    ticket: AtomicUsize,
    serving: AtomicUsize,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Send for TicketLock<T> {}
unsafe impl<T: Send> Sync for TicketLock<T> {}

impl<T> TicketLock<T> {
    pub const fn new(value: T) -> Self {
        TicketLock {
            ticket: AtomicUsize::new(0),
            serving: AtomicUsize::new(0),
            data: UnsafeCell::new(value),
        }
    }

    /// Adquire o lock (spin). Ver contrato no módulo: não-reentrante; não usar em IRQ.
    pub fn lock(&self) -> TicketLockGuard<'_, T> {
        let my_ticket = self.ticket.fetch_add(1, Ordering::Relaxed);
        while self.serving.load(Ordering::Acquire) != my_ticket {
            core::hint::spin_loop();
        }
        TicketLockGuard { lock: self }
    }

    /// Tenta adquirir sem esperar. `None` se ocupado.
    ///
    /// Usa CAS (não `fetch_add`): incrementar ticket sem criar guard deixaria
    /// um ticket órfão e `serving` nunca alcançaria — starvation permanente.
    pub fn try_lock(&self) -> Option<TicketLockGuard<'_, T>> {
        let now_serving = self.serving.load(Ordering::Acquire);
        match self.ticket.compare_exchange(
            now_serving,
            now_serving.wrapping_add(1),
            Ordering::AcqRel,
            Ordering::Relaxed,
        ) {
            Ok(_) => Some(TicketLockGuard { lock: self }),
            Err(_) => None,
        }
    }

    /// Acesso exclusivo sem lock (borrow `&mut self`).
    pub fn get_mut(&mut self) -> &mut T {
        self.data.get_mut()
    }

    /// Consome o lock e devolve o valor.
    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }

    /// Contador ticket (Relaxed) — diagnóstico / instrumentos sem raw ptr.
    pub fn ticket(&self) -> usize {
        self.ticket.load(Ordering::Relaxed)
    }

    /// Contador serving (Relaxed) — diagnóstico / instrumentos sem raw ptr.
    pub fn serving(&self) -> usize {
        self.serving.load(Ordering::Relaxed)
    }

    /// `true` se há waiters ou holder (ticket != serving).
    pub fn is_locked(&self) -> bool {
        self.ticket.load(Ordering::Relaxed) != self.serving.load(Ordering::Relaxed)
    }
}

pub struct TicketLockGuard<'a, T> {
    lock: &'a TicketLock<T>,
}

/// Igual `MutexGuard`: `&Guard` só é `Sync` se `T: Sync`.
unsafe impl<T: Sync> Sync for TicketLockGuard<'_, T> {}

impl<T> Deref for TicketLockGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> DerefMut for TicketLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T> Drop for TicketLockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.serving.fetch_add(1, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cell::Cell;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn exclusive_mutation() {
        let lock = TicketLock::new(0u32);
        {
            let mut g = lock.lock();
            *g += 1;
        }
        assert_eq!(*lock.lock(), 1);
    }

    #[test]
    fn try_lock_none_when_held() {
        let lock = TicketLock::new(0u32);
        let _g = lock.lock();
        assert!(lock.try_lock().is_none());
        assert!(lock.is_locked());
    }

    #[test]
    fn try_lock_ok_when_free() {
        let lock = TicketLock::new(7u32);
        let g = lock.try_lock().expect("free");
        assert_eq!(*g, 7);
    }

    #[test]
    fn fifo_threads() {
        let lock = Arc::new(TicketLock::new(Vec::<u32>::new()));
        let mut handles = Vec::new();
        for i in 0..8u32 {
            let l = Arc::clone(&lock);
            handles.push(thread::spawn(move || {
                let mut g = l.lock();
                g.push(i);
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let g = lock.lock();
        assert_eq!(g.len(), 8);
        for i in 0..8u32 {
            assert!(g.contains(&i));
        }
    }

    #[test]
    fn cell_send_exclusive() {
        let lock = TicketLock::new(Cell::new(0));
        {
            let g = lock.lock();
            g.set(g.get() + 1);
        }
        assert_eq!(lock.lock().get(), 1);
    }

    #[test]
    fn get_mut_into_inner() {
        let mut lock = TicketLock::new(3u32);
        *lock.get_mut() = 9;
        assert_eq!(lock.into_inner(), 9);
    }

    #[test]
    fn getters_track() {
        let lock = TicketLock::new(0u32);
        assert_eq!(lock.ticket(), 0);
        assert_eq!(lock.serving(), 0);
        let g = lock.lock();
        assert_eq!(lock.ticket(), 1);
        assert_eq!(lock.serving(), 0);
        drop(g);
        assert_eq!(lock.serving(), 1);
    }
}
