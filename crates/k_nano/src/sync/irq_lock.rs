//! IrqSafeLock — TicketLock FIFO com desabilitação temporária de IRQ.
//! Previne deadlock quando um handler de interrupção tenta adquirir
//! um lock já segurando pelo código interrompido.
//!
//! Uso: `let guard = LOCK.lock();` — desabilita IRQs na aquisição,
//! restaura o estado anterior no drop do guard.

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicUsize, Ordering};

/// Lê a flag IF (Interrupt Flag) das RFLAGS. Retorna true se interrupções
/// estão habilitadas no momento da chamada.
fn are_irqs_enabled() -> bool {
    let rflags: u64;
    unsafe { core::arch::asm!("pushfq; pop {}", out(reg) rflags, options(nostack, preserves_flags)); }
    (rflags & 0x200) != 0
}

/// Lock FIFO (TicketLock) com IRQ-safe: desabilita interrupções enquanto
/// o lock é segurando, restaura o estado anterior ao liberar.
pub struct IrqSafeLock<T> {
    ticket: AtomicUsize,
    serving: AtomicUsize,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Send for IrqSafeLock<T> {}
unsafe impl<T: Send> Sync for IrqSafeLock<T> {}

impl<T> IrqSafeLock<T> {
    pub const fn new(value: T) -> Self {
        IrqSafeLock {
            ticket: AtomicUsize::new(0),
            serving: AtomicUsize::new(0),
            data: UnsafeCell::new(value),
        }
    }

    /// Adquire o lock desabilitando IRQs.
    /// Restaura o estado anterior (IF flag) quando o guard é dropado.
    pub fn lock(&self) -> IrqSafeGuard<'_, T> {
        let irq_was_enabled = are_irqs_enabled();
        x86_64::instructions::interrupts::disable();

        let my_ticket = self.ticket.fetch_add(1, Ordering::Relaxed);
        while self.serving.load(Ordering::Acquire) != my_ticket {
            core::hint::spin_loop();
        }

        IrqSafeGuard { lock: self, irq_was_enabled }
    }

    /// Tenta adquirir sem esperar. Retorna None se lockado por outro core.
    pub fn try_lock(&self) -> Option<IrqSafeGuard<'_, T>> {
        let irq_was_enabled = are_irqs_enabled();
        x86_64::instructions::interrupts::disable();

        let my_ticket = self.ticket.load(Ordering::Relaxed);
        let now_serving = self.serving.load(Ordering::Acquire);
        if now_serving == my_ticket {
            // Ninguém na fila — tenta tomar o ticket
            let ticket = self.ticket.fetch_add(1, Ordering::Relaxed);
            if ticket == my_ticket {
                return Some(IrqSafeGuard { lock: self, irq_was_enabled });
            }
        }

        // Lock ocupado — restaura IRQ e retorna None
        if irq_was_enabled {
            unsafe { x86_64::instructions::interrupts::enable(); }
        }
        None
    }
}

pub struct IrqSafeGuard<'a, T> {
    lock: &'a IrqSafeLock<T>,
    irq_was_enabled: bool,
}

impl<T> Deref for IrqSafeGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> DerefMut for IrqSafeGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T> Drop for IrqSafeGuard<'_, T> {
    fn drop(&mut self) {
        // Libera o ticket (próximo na fila pode prosseguir)
        self.lock.serving.fetch_add(1, Ordering::Release);
        // Restaura IRQ ao estado anterior (se estava enabled, re-enable)
        if self.irq_was_enabled {
            unsafe { x86_64::instructions::interrupts::enable(); }
        }
    }
}
