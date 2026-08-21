//! PCID (Process Context ID) — preservação de TLB entre trocas de CR3.
//!
//! # Problema resolvido
//! Sem PCID, toda escrita em CR3 faz um **full TLB flush** — invalida todas as
//! 512+ entradas de tradução de memória cacheadas. Em contextos com muitas
//! trocas de CR3 (Ring3→Ring0→Ring3, demand-page, DMA mappings), isso custa
//! ~400 ciclos de CPU por troca.
//!
//! # Solução
//! Com PCID (CPUID.01H:ECX[17]), cada CR3 write pode携带 um ID de 12 bits.
//! O hardware mantém múltiplos conjuntos de TLB, um por PCID. A troca de CR3
//! com bit 63=1 (NOFLUSH) preserva as entradas TLB do PCID anterior.
//!
//! # Layout do registrador CR3 (com PCID)
//! ```
//! 63    62         12  11    0
//! ┌────┬─────────────┬──────┐
//! │FLAG│  PML4 Addr  │ PCID │
//! └────┴─────────────┴──────┘
//! ```
//! - Bits 11:0 = PCID (Process Context ID, 0-4095)
//! - Bit 63 = CR3.NOFLUSH (1 = preserva TLB ao escrever este CR3)
//! - Bits 62:12 = endereço físico da PML4
//!
//! # Design
//! - PCID 0 = kernel (sempre acessível, sem NOFLUSH)
//! - PCID 1..MAX_PCIDs = user address spaces
//! - Bitmap de PCIDs em uso (lock-free, `AtomicU32`)
//! - `activate_pcid()` seta NOFLUSH se o PCID já foi usado antes

use core::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, Ordering};
use x86_64::structures::paging::PhysFrame;
use x86_64::PhysAddr;

/// Máximo de PCIDs simultâneos (limitado por hardware: 12 bits = 4096,
/// mas usamos 64 para simplicity — mais que suficiente para o OS).
const MAX_PCIDS: usize = 64;

/// PCID 0 reservado para o kernel.
const KERNEL_PCID: u16 = 0;

/// CR4 bit 17 = PCIDE (Process Context Identifiers Enable).
const CR4_PCIDE_BIT: u64 = 1 << 17;

/// CR3 bit 63 = NOFLUSH.
const CR3_NOFLUSH_BIT: u64 = 1 << 63;

/// PCID do kernel (sempre 0).
static KERNEL_PCID_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Bitmap de PCIDs em uso (bit i = PCID i está alocado).
/// Bit 0 (kernel) é sempre setado.
static PCID_BITMAP: AtomicU32 = AtomicU32::new(1); // PCID 0 = kernel

/// PCID mais recentemente usado (para decidir NOFLUSH).
static LAST_USED_PCID: AtomicU16 = AtomicU16::new(0);

/// Detecta suporte a PCID via CPUID.
#[cfg(target_arch = "x86_64")]
pub fn has_pcid_support() -> bool {
    unsafe {
        let cpuid = core::arch::x86_64::__cpuid(1);
        (cpuid.ecx >> 17) & 1 == 1
    }
}

#[cfg(not(target_arch = "x86_64"))]
pub fn has_pcid_support() -> bool {
    false
}

/// Detecta suporte a INVPCID (necessário para invalidar PCIDs individuais).
#[cfg(target_arch = "x86_64")]
pub fn has_invpcid_support() -> bool {
    unsafe {
        let cpuid = core::arch::x86_64::__cpuid(7);
        (cpuid.edx >> 28) & 1 == 1
    }
}

#[cfg(not(target_arch = "x86_64"))]
pub fn has_invpcid_support() -> bool {
    false
}

/// Habilita PCID no CR4.
///
/// # Safety
/// - Deve ser chamado UMA VEZ no boot, antes de qualquer CR3 write com PCID
/// - Requer ring 0
/// - Se o hypervisor esconde PCID (WHPX/TCG), o gate falha gracefully
pub unsafe fn enable_pcid() -> bool {
    if !has_pcid_support() {
        crate::slog_nano!(
            "PCID", "warn",
            "PCID NÃO suportado (CPUID.01H:ECX[17]=0) — TLB flush completo em cada CR3 write"
        );
        return false;
    }

    // Lê CR4 atual
    let cr4: u64;
    core::arch::asm!("mov {}, cr4", out(reg) cr4, options(nostack));

    // Seta bit 17 (PCIDE)
    let new_cr4 = cr4 | CR4_PCIDE_BIT;
    core::arch::asm!("mov cr4, {}", in(reg) new_cr4, options(nostack));

    // Verifica se pegou
    let verify: u64;
    core::arch::asm!("mov {}, cr4", out(reg) verify, options(nostack));
    if verify & CR4_PCIDE_BIT == 0 {
        crate::slog_nano!(
            "PCID", "warn",
            "CR4.PCIDE não persistiu (hypervisor interceptou?) — sem PCID"
        );
        return false;
    }

    KERNEL_PCID_ACTIVE.store(true, Ordering::Release);

    crate::slog_nano!(
        "PCID", "info",
        "PCID ATIVADO (CR4 bit 17) — TLB preservado entre trocas de CR3"
    );
    true
}

/// Aloca um PCID livre (bitmap lock-free com CAS).
///
/// Retorna `Some(pcid)` se alocado, `None` se todos os 64 estão em uso.
pub fn alloc_pcid() -> Option<u16> {
    loop {
        let bitmap = PCID_BITMAP.load(Ordering::Acquire);
        // Encontra o primeiro bit livre (bit 0 = kernel, reservado)
        let free = (!bitmap).trailing_zeros();
        if free >= MAX_PCIDS as u32 {
            return None; // todos em uso
        }
        let new_bitmap = bitmap | (1 << free);
        if PCID_BITMAP.compare_exchange_weak(
            bitmap,
            new_bitmap,
            Ordering::Release,
            Ordering::Relaxed,
        ).is_ok()
        {
            return Some(free as u16);
        }
        // CAS falhou — retry
    }
}

/// Libera um PCID (bitmap lock-free).
///
/// # Safety
/// O PCID deve ter sido alocado por `alloc_pcid()` e não estar em uso.
pub fn free_pcid(pcid: u16) {
    if pcid == KERNEL_PCID {
        return; // nunca liberar o kernel
    }
    PCID_BITMAP.fetch_and(!(1u32 << pcid as u32), Ordering::Release);
}

/// Calcula o valor de CR3 com PCID + NOFLUSH flag.
///
/// Se o PCID foi usado anteriormente (bit setado no bitmap e é o último usado),
/// seta NOFLUSH para preservar o TLB.
///
/// # Safety
/// - `l4_phys` deve ser o endereço físico válido de uma PML4
/// - O PCID deve estar alocado via `alloc_pcid()`
#[inline]
pub unsafe fn cr3_with_pcid(l4_phys: PhysAddr, pcid: u16, previously_used: bool) -> u64 {
    let mut cr3_val = l4_phys.as_u64();

    // Bits 11:0 = PCID
    cr3_val |= pcid as u64;

    // Bit 63 = NOFLUSH (preserva TLB se já usado antes)
    if previously_used {
        cr3_val |= CR3_NOFLUSH_BIT;
    }

    cr3_val
}

/// Troca CR3 com PCID + NOFLUSH (se aplicável).
///
/// Retorna o PCID anterior (para tracking).
///
/// # Safety
/// Requer ring 0 + CR4.PCIDE habilitado.
#[inline]
pub unsafe fn switch_cr3_pcid(
    new_l4: PhysFrame<x86_64::structures::paging::Size4KiB>,
    new_pcid: u16,
) -> u16 {
    let old_pcid = LAST_USED_PCID.load(Ordering::Relaxed);

    // Se o PCID novo é o mesmo do anterior, não precisa trocar
    if new_pcid == old_pcid && old_pcid != 0 {
        return old_pcid;
    }

    // Determina se o PCID novo já foi usado antes (para NOFLUSH)
    let previously_used = new_pcid == 0 || (PCID_BITMAP.load(Ordering::Relaxed) & (1 << new_pcid)) != 0;

    let cr3_val = cr3_with_pcid(new_l4.start_address(), new_pcid, previously_used);

    // Write CR3 — com PCID preserva TLB do PCID anterior
    core::arch::asm!("mov cr3, {}", in(reg) cr3_val, options(nostack));

    LAST_USED_PCID.store(new_pcid, Ordering::Release);
    old_pcid
}

/// Retorna o PCID atualmente ativo.
#[inline]
pub fn current_pcid() -> u16 {
    LAST_USED_PCID.load(Ordering::Relaxed)
}

/// Verifica se PCID está ativo (CR4.PCIDE = 1).
pub fn is_enabled() -> bool {
    KERNEL_PCID_ACTIVE.load(Ordering::Acquire)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcid_bitmap_initial() {
        // Bit 0 (kernel) deve estar setado
        assert!(PCID_BITMAP.load(Ordering::Relaxed) & 1 == 1);
    }

    #[test]
    fn alloc_and_free_pcid() {
        // Aloca PCID 1
        let pcid = alloc_pcid();
        assert_eq!(pcid, Some(1));

        // Libera PCID 1
        free_pcid(pcid.unwrap());

        // Aloca de novo — deve ser 1 (primeiro livre)
        let pcid2 = alloc_pcid();
        assert_eq!(pcid2, Some(1));
        free_pcid(pcid2.unwrap());
    }

    #[test]
    fn kernel_pcid_never_freed() {
        free_pcid(KERNEL_PCID);
        assert!(PCID_BITMAP.load(Ordering::Relaxed) & 1 == 1);
    }

    #[test]
    fn cr3_with_pcid_encoding() {
        let l4 = PhysAddr::new(0x1000);
        let val = unsafe { cr3_with_pcid(l4, 5, false) };
        assert_eq!(val & 0xFFF, 5); // PCID in bits 11:0
        assert_eq!(val & (1 << 63), 0); // NOFLUSH not set

        let val_flush = unsafe { cr3_with_pcid(l4, 5, true) };
        assert_eq!(val_flush & (1 << 63), 1 << 63); // NOFLUSH set
    }

    #[test]
    fn constants_correct() {
        assert_eq!(CR4_PCIDE_BIT, 1 << 17);
        assert_eq!(CR3_NOFLUSH_BIT, 1u64 << 63);
        assert_eq!(KERNEL_PCID, 0);
    }
}
