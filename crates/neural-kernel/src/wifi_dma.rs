//! DMA infrastructure — PhysicalBuffer alinhado a 4K, clflush, IOMMU compat.
//! Garante coerencia de cache entre CPU e chips WiFi via PCIe.

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
            asm!("clflush [{0}]", in(reg) addr, options(nostack, preserves_flags));
            addr += 64;
        }
        asm!("mfence", options(nostack, preserves_flags));
    }
}
