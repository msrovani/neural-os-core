//! ADR-0062 E2 — Boot handoff abstraction.
//! Trait `BootHandoff` unifica rust-bootloader 0.11 `BootInfo` e Limine,
//! eliminando os `if boot_info.is_some()` em `kernel_boot`.
//! Cada implementador fornece offset físico, RSDP, regiões utilizáveis,
//! e uma consulta unificada de endereço-em-região.

/// Região de memória utilizável (base + tamanho).
#[derive(Clone, Copy, Debug)]
pub struct MemRegion {
    pub base: u64,
    pub len: u64,
}

/// Trait que abstrai o handoff do bootloader (BootInfo ou Limine).
pub trait BootHandoff {
    /// Physical memory offset (HHDM) para converter endereços físicos em virtuais.
    fn phys_mem_offset(&self) -> u64;

    /// Endereço físico da RSDP (ACPI), se disponível.
    fn rsdp_addr(&self) -> Option<u64>;

    /// Nome legível do protocolo de boot ("rust-bootloader" | "limine").
    fn boot_tag(&self) -> &'static str;

    /// Fatia das regiões de memória utilizáveis.
    fn usable_regions(&self) -> &[MemRegion];

    /// Endereço físico onde o bootloader carregou o kernel image (se conhecido).
    /// Usado para marcar a RAM do kernel como OCUPADA no frame allocator
    /// (SESSION_252/ora-1: sem isso, frames do kernel/heap podem ser entregues
    /// a DMA — e1000 RX sobrescreve o heap). Default None (bootloader legado).
    fn kernel_phys(&self) -> Option<u64> {
        None
    }

    /// Região KernelAndModules do memmap (base, len) — fallback do
    /// kernel_phys quando o KernelAddressRequest não é processado pelo
    /// bootloader (SESSION_252: Limine response null). Default (0,0).
    fn kernel_region(&self) -> (u64, u64) {
        (0, 0)
    }

    /// Verdadeiro se `addr` físico cai em alguma região de memória.
    /// Usado para verificar se o QEMU loader depositou dados em 4GB.
    fn has_addr_in_any_region(&self, addr: u64) -> bool {
        self.usable_regions()
            .iter()
            .any(|r| r.base <= addr && r.base.saturating_add(r.len) > addr)
    }

    /// Endereço final da região que contém `addr`, se existir.
    /// Usado para truncar o tamanho do modelo carregado pelo QEMU loader.
    fn region_end_containing(&self, addr: u64) -> Option<u64> {
        self.usable_regions().iter().find_map(|r| {
            let end = r.base.saturating_add(r.len);
            if r.base <= addr && end > addr {
                Some(end)
            } else {
                None
            }
        })
    }

}
