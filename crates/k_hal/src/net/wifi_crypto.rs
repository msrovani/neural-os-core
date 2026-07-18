//! Injeção de chave WPA2 nos regs crypto do chip (ADR-0041 H3 — MMIO em k-hal).

pub struct CryptoOffsets {
    pub reg_crypto_ctrl: usize,
    pub reg_key_idx: usize,
    pub reg_key_data: usize,
}

pub const INTEL_CRYPTO: CryptoOffsets = CryptoOffsets {
    reg_crypto_ctrl: 0x3000,
    reg_key_idx: 0x3004,
    reg_key_data: 0x3008,
};
pub const REALTEK_CRYPTO: CryptoOffsets = CryptoOffsets {
    reg_crypto_ctrl: 0x00E0,
    reg_key_idx: 0x00E4,
    reg_key_data: 0x00E8,
};
pub const ATHEROS_CRYPTO: CryptoOffsets = CryptoOffsets {
    reg_crypto_ctrl: 0x0D00,
    reg_key_idx: 0x0D04,
    reg_key_data: 0x0D08,
};
pub const BROADCOM_CRYPTO: CryptoOffsets = CryptoOffsets {
    reg_crypto_ctrl: 0x0700,
    reg_key_idx: 0x0704,
    reg_key_data: 0x0708,
};

/// Injeta PMK 32 bytes nos registradores criptográficos (AES-CCMP HW).
pub unsafe fn inject_wpa2_key(mmio_base: usize, crypto: &CryptoOffsets, pmk: &[u8; 32]) {
    use core::ptr::{read_volatile, write_volatile};
    write_volatile((mmio_base + crypto.reg_key_idx) as *mut u32, 0x00);
    let key_reg = (mmio_base + crypto.reg_key_data) as *mut u32;
    let pmk_words = pmk.as_ptr() as *const u32;
    for i in 0..8 {
        write_volatile(key_reg.add(i), read_volatile(pmk_words.add(i)));
    }
    write_volatile((mmio_base + crypto.reg_crypto_ctrl) as *mut u32, 0x01);
}
