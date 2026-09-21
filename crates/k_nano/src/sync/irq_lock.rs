//! IrqSafeLock — TicketLock FIFO com desabilitação temporária de IRQ.
//! Previne deadlock quando um handler de interrupção tenta adquirir
//! um lock já segurado pelo código interrompido.
//!
//! SESSION_378: algoritmo = `ticket_lock::TicketLock` (fonte única + `#[repr(C)]`).
//! Uso: `let guard = LOCK.lock();` — desabilita IRQs na aquisição,
//! restaura o estado anterior no drop do guard.

use core::mem::ManuallyDrop;
use core::ops::{Deref, DerefMut};
use ticket_lock::{TicketLock, TicketLockGuard};

/// Lê a flag IF (Interrupt Flag) das RFLAGS. Retorna true se interrupções
/// estão habilitadas no momento da chamada.
fn are_irqs_enabled() -> bool {
    let rflags: u64;
    unsafe {
        core::arch::asm!("pushfq; pop {}", out(reg) rflags, options(nostack, preserves_flags));
    }
    (rflags & 0x200) != 0
}

/// Lock FIFO com IRQ-safe: desabilita interrupções enquanto o lock é segurado.
/// Layout: `TicketLock` interno já é `#[repr(C)]` (ticket@0 / serving@8).
#[repr(C)]
pub struct IrqSafeLock<T> {
    inner: TicketLock<T>,
}

unsafe impl<T: Send> Send for IrqSafeLock<T> {}
unsafe impl<T: Send> Sync for IrqSafeLock<T> {}

impl<T> IrqSafeLock<T> {
    pub const fn new(value: T) -> Self {
        IrqSafeLock {
            inner: TicketLock::new(value),
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

        let guard = self.inner.lock();
        IrqSafeGuard {
            guard: ManuallyDrop::new(guard),
            irq_was_enabled,
        }
    }

    /// Tenta adquirir sem esperar. Retorna None se lockado por outro core.
    pub fn try_lock(&self) -> Option<IrqSafeGuard<'_, T>> {
        let irq_was_enabled = are_irqs_enabled();
        #[cfg(target_os = "none")]
        x86_64::instructions::interrupts::disable();

        match self.inner.try_lock() {
            Some(guard) => Some(IrqSafeGuard {
                guard: ManuallyDrop::new(guard),
                irq_was_enabled,
            }),
            None => {
                if irq_was_enabled {
                    #[cfg(target_os = "none")]
                    unsafe {
                        x86_64::instructions::interrupts::enable();
                    }
                }
                None
            }
        }
    }
}

pub struct IrqSafeGuard<'a, T> {
    /// Drop manual: libera ticket **antes** de re-enable IRQ.
    guard: ManuallyDrop<TicketLockGuard<'a, T>>,
    irq_was_enabled: bool,
}

unsafe impl<T: Sync> Sync for IrqSafeGuard<'_, T> {}

impl<T> Deref for IrqSafeGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.guard
    }
}

impl<T> DerefMut for IrqSafeGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.guard
    }
}

impl<T> Drop for IrqSafeGuard<'_, T> {
    fn drop(&mut self) {
        // Libera o ticket (próximo na fila pode prosseguir) antes de sti.
        unsafe {
            ManuallyDrop::drop(&mut self.guard);
        }
        if self.irq_was_enabled {
            #[cfg(target_os = "none")]
            unsafe {
                x86_64::instructions::interrupts::enable();
            }
        }
    }
}
