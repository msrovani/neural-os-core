//! SPSC lock-free ring buffer em memória compartilhada (MVP C / ADR-0041).

use core::sync::atomic::{AtomicUsize, Ordering};

/// Capacidade de dados no slot de 4 KiB (cabeçalho = 2 × usize).
pub const RING_CAP: usize = 4096 - 2 * core::mem::size_of::<AtomicUsize>();

/// Layout fixo numa página compartilhada entre address spaces.
#[repr(C, align(4096))]
pub struct SharedSpscRing {
    pub head: AtomicUsize,
    pub tail: AtomicUsize,
    pub data: [u8; RING_CAP],
}

impl SharedSpscRing {
    pub unsafe fn init_at(ptr: *mut SharedSpscRing) {
        let r = &mut *ptr;
        r.head.store(0, Ordering::Relaxed);
        r.tail.store(0, Ordering::Relaxed);
        r.data.fill(0);
    }

    #[inline]
    fn mask(i: usize) -> usize {
        i % RING_CAP
    }

    /// Produtor único: enfileira um byte (interior mutability via raw ptr).
    pub fn push(&self, byte: u8) -> Result<(), &'static str> {
        let head = self.head.load(Ordering::Relaxed);
        let next = head.wrapping_add(1);
        if Self::mask(next) == Self::mask(self.tail.load(Ordering::Acquire)) {
            return Err("mvp-c: ring cheio");
        }
        let idx = Self::mask(head);
        unsafe {
            core::ptr::write_volatile(self.data.as_ptr().add(idx) as *mut u8, byte);
        }
        self.head.store(next, Ordering::Release);
        Ok(())
    }

    /// Consumidor único: retira um byte.
    pub fn pop(&self) -> Result<u8, &'static str> {
        let tail = self.tail.load(Ordering::Relaxed);
        if Self::mask(tail) == Self::mask(self.head.load(Ordering::Acquire)) {
            return Err("mvp-c: ring vazio");
        }
        let idx = Self::mask(tail);
        let byte = unsafe { core::ptr::read_volatile(self.data.as_ptr().add(idx)) };
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        Ok(byte)
    }
}

const _: () = assert!(core::mem::size_of::<SharedSpscRing>() <= 4096);
