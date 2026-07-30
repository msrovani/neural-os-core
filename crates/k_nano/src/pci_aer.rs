//! PCIe AER — Advanced Error Reporting para isolamento elétrico de falhas.
//! Previne que dispositivo PCIe com defeito derrube o kernel.
//!
//! Integração: NetDriverAgent chama `check_aer()` no probe de cada dispositivo.
//! Se AER reportar Surprise Down / Data Poisoning / Completer Abort, o agente
//! desabilita o device e publica HEALTH_ISSUE no EventBus.

use core::ptr::read_volatile;

#[repr(C)]
struct AerBlock {
    unc_err_status: u32,
    unc_err_mask: u32,
    cor_err_status: u32,
    adv_caps_ctrl: u32,
}

/// Verifica status de erro AER em um dispositivo PCIe.
/// `aer_base` = endereço MMIO do bloco AER (obtido via PCI cap pointer).
pub unsafe fn check_aer(aer_base: usize) -> Result<(), &'static str> {
    let aer = aer_base as *const AerBlock;
    let unc = read_volatile(&(*aer).unc_err_status);
    if unc == 0 {
        return Ok(());
    }
    if (unc & (1 << 20)) != 0 {
        return Err("Surprise Down — placa removida");
    }
    if (unc & (1 << 12)) != 0 {
        return Err("Data Poisoning — falha elétrica");
    }
    if (unc & (1 << 4)) != 0 {
        return Err("Completer Abort — placa incapaz");
    }
    Ok(())
}
