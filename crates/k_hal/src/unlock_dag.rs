//! UnlockDAG — tokens cross-subsystem (ADR-0056).
//! Stages honestos; Partial ≠ Ready. Sem deadlock USB↔FAT inventado.

use core::sync::atomic::{AtomicU64, Ordering};

/// Capacidade lógica liberada por um estágio (não é Cap syscall).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CapToken {
    PciEnumerated = 0,
    ApicReady = 1,
    FatReadable = 2,
    UsbHostReset = 3,
    UsbHostSched = 4,
    UsbPortReady = 5,
    UsbEp0 = 6,
    WifiFwAlive = 7,
    GpuAcrBooted = 8,
    GpuCompute = 9,
    GpuDisplay = 10,
    BtHciReady = 11,
    /// Labor 8 — MemoryCore alcançado (BOOT_011_SMOKE).
    BootSmokeOk = 12,
    /// Labor 14 — ath10k assoc OK com RF (ATH10K_ASSOC).
    WifiAssociated = 13,
    /// Labor 15 — xHCI hub enumerated (USB_HUB).
    UsbHubOk = 14,
    /// Blit 2D acelerado verificado via canário gradiente.
    GpuBlitReady = 15,
}

impl CapToken {
    pub fn as_str(self) -> &'static str {
        match self {
            CapToken::PciEnumerated => "PciEnumerated",
            CapToken::ApicReady => "ApicReady",
            CapToken::FatReadable => "FatReadable",
            CapToken::UsbHostReset => "UsbHostReset",
            CapToken::UsbHostSched => "UsbHostSched",
            CapToken::UsbPortReady => "UsbPortReady",
            CapToken::UsbEp0 => "UsbEp0",
            CapToken::WifiFwAlive => "WifiFwAlive",
            CapToken::GpuAcrBooted => "GpuAcrBooted",
            CapToken::GpuCompute => "GpuCompute",
            CapToken::GpuDisplay => "GpuDisplay",
            CapToken::BtHciReady => "BtHciReady",
            CapToken::BootSmokeOk => "BootSmokeOk",
            CapToken::WifiAssociated => "WifiAssociated",
            CapToken::UsbHubOk => "UsbHubOk",
            CapToken::GpuBlitReady => "GpuBlitReady",
        }
    }

    fn bit(self) -> u64 {
        1u64 << (self as u8)
    }
}

/// Estado de um nó rebelde / classe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum UnlockStage {
    Locked = 0,
    NeedsFw = 1,
    BringingUp = 2,
    Partial = 3,
    Ready = 4,
    Failed = 5,
    Quarantined = 6,
}

impl UnlockStage {
    pub fn as_str(self) -> &'static str {
        match self {
            UnlockStage::Locked => "Locked",
            UnlockStage::NeedsFw => "NeedsFw",
            UnlockStage::BringingUp => "BringingUp",
            UnlockStage::Partial => "Partial",
            UnlockStage::Ready => "Ready",
            UnlockStage::Failed => "Failed",
            UnlockStage::Quarantined => "Quarantined",
        }
    }
}

static TOKENS: AtomicU64 = AtomicU64::new(0);

pub fn grant(token: CapToken) {
    let prev = TOKENS.fetch_or(token.bit(), Ordering::SeqCst);
    if prev & token.bit() == 0 {
        k_nano::slog_hal!("UnlockDAG", "grant", "token={}", token.as_str());
    }
}

pub fn revoke(token: CapToken) {
    TOKENS.fetch_and(!token.bit(), Ordering::SeqCst);
}

pub fn has(token: CapToken) -> bool {
    (TOKENS.load(Ordering::SeqCst) & token.bit()) != 0
}

/// Todos os `requires` presentes?
pub fn requires_met(requires: &[CapToken]) -> bool {
    requires.iter().all(|t| has(*t))
}

/// Bootstrap plataforma após PCI scan (não implica Ready de WiFi/GPU).
pub fn boot_platform_tokens(pci_ok: bool, fat_hint: bool) {
    if pci_ok {
        grant(CapToken::PciEnumerated);
    }
    // APIC tipicamente já up no bring-up; token honesto se PCI ok.
    if pci_ok {
        grant(CapToken::ApicReady);
    }
    if fat_hint {
        grant(CapToken::FatReadable);
    }
    k_nano::slog_hal!(
        "UnlockDAG",
        "boot",
        "pci={} fat={} mask={:#x}",
        pci_ok,
        fat_hint,
        TOKENS.load(Ordering::SeqCst)
    );
}

pub fn token_mask() -> u64 {
    TOKENS.load(Ordering::SeqCst)
}
