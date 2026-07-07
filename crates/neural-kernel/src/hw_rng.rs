//! Hardware Random Number Generator — instrucao RDRAND x86_64.
//! Usa ruido termico do silicio (TRNG) para entropia criptografica.
//! Essencial para WPA2 nonces, WPA3-SAE Dragonfly, e chaves efemeras.

use core::arch::asm;

pub struct HardwareRandom;

impl HardwareRandom {
    /// Executa RDRAND, retorna 64 bits aleatorios do hardware.
    /// CF=1 sucesso, CF=0 falha (pool de entropia esgotado).
    #[inline(always)]
    pub fn next_u64() -> Option<u64> {
        let mut val: u64 = 0;
        let mut ok: u8 = 0;
        unsafe {
            asm!(
                "rdrand {val}",
                "setc {ok}",
                val = out(reg) val,
                ok = out(reg_byte) ok,
                options(nomem, nostack, preserves_flags),
            );
        }
        if ok == 1 { Some(val) } else { None }
    }

    /// Tenta RDRAND com retentativas.
    /// Usado em caminhos criptograficos que nao podem falhar.
    pub fn next_u64_retry(max: usize) -> Result<u64, &'static str> {
        for _ in 0..max {
            if let Some(v) = Self::next_u64() { return Ok(v); }
            unsafe { asm!("pause", options(nomem, nostack)); }
        }
        Err("RDRAND esgotado")
    }

    /// Preenche buffer com bytes aleatorios do hardware.
    pub fn fill_bytes(buf: &mut [u8]) -> Result<(), &'static str> {
        for chunk in buf.chunks_mut(8) {
            let r = Self::next_u64_retry(10)?;
            let bytes = r.to_ne_bytes();
            let n = chunk.len().min(8);
            chunk.copy_from_slice(&bytes[..n]);
        }
        Ok(())
    }

    /// Gera um nonce de 32 bytes para WPA2 handshake.
    pub fn wpa2_nonce() -> Result<[u8; 32], &'static str> {
        let mut n = [0u8; 32];
        Self::fill_bytes(&mut n)?;
        Ok(n)
    }
}
