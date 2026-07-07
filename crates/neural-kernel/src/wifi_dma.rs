//! DMA infrastructure — PhysicalBuffer alinhado a 4K, clflushopt + sfence.
//! clflushopt e assincrono e permite paralelismo no cache flushing.

use core::arch::asm;

#[repr(C, align(4096))]
pub struct PhysicalBuffer<const N: usize> {
    pub data: [u8; N],
}

impl<const N: usize> PhysicalBuffer<N> {
    pub const fn new() -> Self { Self { data: [0u8; N] } }

    #[inline(always)]
    pub fn phys_addr(&self) -> u64 { self.data.as_ptr() as u64 }

    #[inline(always)]
    pub unsafe fn invalidate_cache(&self) {
        let mut addr = self.data.as_ptr() as usize;
        let end = addr + N;
        while addr < end {
            asm!("clflushopt [{0}]", in(reg) addr, options(nostack, preserves_flags));
            addr += 64;
        }
        asm!("sfence", options(nostack, preserves_flags));
    }
}
