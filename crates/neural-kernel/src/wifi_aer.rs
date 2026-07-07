//! PCIe AER — Advanced Error Reporting para isolamento eletrico de falhas.
//! Previne que placa WiFi com defeito derrube o kernel.

use core::ptr::read_volatile;

#[repr(C)]
struct AerBlock {
    unc_err_status: u32,
    unc_err_mask: u32,
    cor_err_status: u32,
    adv_caps_ctrl: u32,
}

pub unsafe fn check_aer(aer_base: usize) -> Result<(), &'static str> {
    let aer = aer_base as *const AerBlock;
    let unc = read_volatile(&(*aer).unc_err_status);
    if unc == 0 { return Ok(()); }
    if (unc & (1 << 20)) != 0 { return Err("Surprise Down — placa removida"); }
    if (unc & (1 << 12)) != 0 { return Err("Data Poisoning — falha eletrica"); }
    if (unc & (1 << 4)) != 0 { return Err("Completer Abort — placa incapaz"); }
    Ok(())
}
