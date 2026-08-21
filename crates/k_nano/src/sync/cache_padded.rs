//! CachePadded — wrapper genérico para prevenção de false sharing.
//!
//! # Problema
//! Em SMP, duas variáveis atômicas na mesma cache line (64B) causam
//! *cache line bouncing* — cada escrita invalida a linha no core remoto.
//! Em filas lock-free (head/tail), isso degrada throughput em 10-50×.
//!
//! # Solução
//! `CachePadded<T>` alinha `T` a 64 bytes (x86_64 L1/L2 cache line).
//! Head e tail de filas ficam em cache lines separadas — zero contention.
//!
//! # Uso
//! ```ignore
//! use k_nano::sync::cache_padded::CachePadded;
//!
//! pub struct LockFreeRing<T, const CAP: usize> {
//!     head: CachePadded<AtomicUsize>,
//!     tail: CachePadded<AtomicUsize>,
//!     buffer: [UnsafeCell<MaybeUninit<T>>; CAP],
//! }
//! ```

use core::ops::{Deref, DerefMut};

/// Cache line size padrão x86_64 (64 bytes).
/// AMD Zen 4+ usa 128B (L2), mas 64B é o mínimo garantido por Intel e AMD L1.
pub const CACHE_LINE_SIZE: usize = 64;

/// Wrapper que alinha `T` a cache line para prevenir false sharing.
///
/// `CachePadded<T>` tem o mesmo ABI que `T` mas com `align(64)`.
/// Em filas lock-free, head e tail devem ser `CachePadded` para que
/// producer e consumer não compartilhem cache lines.
#[repr(C, align(64))]
pub struct CachePadded<T> {
    value: T,
}

impl<T> CachePadded<T> {
    /// Cria novo CachePadded com o valor dado.
    #[inline]
    pub const fn new(value: T) -> Self {
        Self { value }
    }

    /// Extrai o valor (consuming).
    #[inline]
    pub fn into_inner(self) -> T {
        self.value
    }
}

impl<T> Deref for CachePadded<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> DerefMut for CachePadded<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

impl<T: Default> Default for CachePadded<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: Clone> Clone for CachePadded<T> {
    fn clone(&self) -> Self {
        Self::new(self.value.clone())
    }
}

impl<T: Copy> Copy for CachePadded<T> {}

/// Verifica em compile-time que CachePadded alinha corretamente.
const _: () = assert!(
    core::mem::align_of::<CachePadded<u64>>() >= CACHE_LINE_SIZE,
    "CachePadded must align to at least CACHE_LINE_SIZE"
);

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn alignment_is_cache_line() {
        assert!(core::mem::align_of::<CachePadded<u64>>() >= 64);
        assert!(core::mem::align_of::<CachePadded<AtomicUsize>>() >= 64);
    }

    #[test]
    fn size_is_at_least_cache_line() {
        // CachePadded<u64> deve ter >= 64 bytes (alignment impõe padding)
        assert!(core::mem::size_of::<CachePadded<u64>>() >= 64);
    }

    #[test]
    fn deref_works() {
        let padded = CachePadded::new(42u64);
        assert_eq!(*padded, 42);
    }

    #[test]
    fn deref_mut_works() {
        let mut padded = CachePadded::new(10u32);
        *padded = 20;
        assert_eq!(*padded, 20);
    }

    #[test]
    fn two_padeds_are_separate_cache_lines() {
        // Dois CachePadded adjacentes devem estar em cache lines diferentes
        let a = CachePadded::new(1u64);
        let b = CachePadded::new(2u64);
        let addr_a = &a as *const _ as usize;
        let addr_b = &b as *const _ as usize;
        // Pelo menos CACHE_LINE_SIZE de distância
        let dist = if addr_a > addr_b { addr_a - addr_b } else { addr_b - addr_a };
        assert!(dist >= CACHE_LINE_SIZE, "Two CachePads too close: {} bytes apart", dist);
    }
}
