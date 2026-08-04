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
        // Host build/tests: `cli` is privileged (STATUS_PRIVILEGED_INSTRUCTION).
        // Gate on the kernel target so it also applies when k_nano is a
        // dependency (deps are compiled without `cfg(test)`).
        #[cfg(target_os = "none")]
        x86_64::instructions::interrupts::disable();

        let my_ticket = self.ticket.fetch_add(1, Ordering::Relaxed);
        while self.serving.load(Ordering::Acquire) != my_ticket {
            core::hint::spin_loop();
        }

        IrqSafeGuard { lock: self, irq_was_enabled }
    }

    /// Tenta adquirir sem esperar. Retorna None se lockado por outro core.
    ///
    /// Usa CAS (compare_exchange) em vez de fetch_add incondicional: se o
    /// ticket estivesse sempre incrementando mesmo quando o lock já está
    /// ocupado, o ticket "roubado" nunca seria liberado (nenhum guard seria
    /// criado para chamar `serving.fetch_add` no Drop), causando starvation
    /// permanente de `lock()`/`try_lock()` (deadlock: `serving` nunca alcança
    /// o ticket perdido).
    pub fn try_lock(&self) -> Option<IrqSafeGuard<'_, T>> {
        let irq_was_enabled = are_irqs_enabled();
        #[cfg(target_os = "none")]
        x86_64::instructions::interrupts::disable();

        let now_serving = self.serving.load(Ordering::Acquire);
        // So reivindica o ticket se ninguem estiver na fila (ticket == serving).
        // CAS garante que nao incrementamos o contador se a condicao deixou de
        // valer entre o load e a tentativa (outro core pode ter adquirido).
        match self.ticket.compare_exchange(now_serving, now_serving + 1, Ordering::AcqRel, Ordering::Relaxed) {
            Ok(_) => Some(IrqSafeGuard { lock: self, irq_was_enabled }),
            Err(_) => {
                // Lock ocupado — restaura IRQ e retorna None (nenhum ticket foi consumido)
                if irq_was_enabled {
                    #[cfg(target_os = "none")]
                    unsafe { x86_64::instructions::interrupts::enable(); }
                }
                None
            }
        }
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
            #[cfg(target_os = "none")]
            unsafe { x86_64::instructions::interrupts::enable(); }
        }
    }
}

