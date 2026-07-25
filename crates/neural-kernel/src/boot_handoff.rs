//! ADR-0062 E2 — BootloaderHandoff wrapper.
//! Adapta `bootloader_api::BootInfo` ao trait `BootHandoff`.

use k_nano::boot_handoff::{BootHandoff, MemRegion};

/// Wrapper ao redor de `bootloader_api::BootInfo` que implementa `BootHandoff`.
pub struct BootloaderHandoff {
    /// Referência crua para o `BootInfo` (vida toda do kernel).
    pub inner: &'static bootloader_api::BootInfo,
    /// Regiões utilizáveis convertidas (pré-calculadas no construtor).
    regions: [MemRegion; 64],
    region_count: usize,
}

impl BootloaderHandoff {
    /// Constrói o wrapper, convertendo regiões `Usable` do bootloader para
    /// `MemRegion` (base + len).
    pub fn new(bi: &'static bootloader_api::BootInfo) -> Self {
        let mut regions = [MemRegion { base: 0, len: 0 }; 64];
        let mut n = 0;
        for r in bi.memory_regions.iter() {
            if n >= 64 {
                break;
            }
            if r.kind == bootloader_api::info::MemoryRegionKind::Usable {
                regions[n] = MemRegion {
                    base: r.start,
                    len: r.end - r.start,
                };
                n += 1;
            }
        }
        BootloaderHandoff {
            inner: bi,
            regions,
            region_count: n,
        }
    }
}

impl BootHandoff for BootloaderHandoff {
    fn raw_boot_info(&self) -> Option<&'static bootloader_api::BootInfo> {
        Some(self.inner)
    }

    fn phys_mem_offset(&self) -> u64 {
        self.inner
            .physical_memory_offset
            .into_option()
            .unwrap_or(0)
    }

    fn rsdp_addr(&self) -> Option<u64> {
        self.inner.rsdp_addr.into_option()
    }

    fn boot_tag(&self) -> &'static str {
        "rust-bootloader"
    }

    fn usable_regions(&self) -> &[MemRegion] {
        &self.regions[..self.region_count]
    }

    fn has_addr_in_any_region(&self, addr: u64) -> bool {
        self.inner
            .memory_regions
            .iter()
            .any(|r| r.start <= addr && r.end > addr)
    }
}
