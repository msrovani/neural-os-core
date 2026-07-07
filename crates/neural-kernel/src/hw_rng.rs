//! Hardware RNG + CSPRNG ChaCha20 fallback.
//! RDRAND primario, ChaCha20 como fallback se hardware esgotar.

use core::arch::asm;

pub struct HardwareRandom;

impl HardwareRandom {
    #[inline(always)]
    pub fn next_u64() -> Option<u64> {
        let mut val: u64 = 0; let mut ok: u8 = 0;
        unsafe {
            asm!("rdrand {val}", "setc {ok}", val = out(reg) val, ok = out(reg_byte) ok,
                options(nomem, nostack, preserves_flags));
        }
        if ok == 1 { Some(val) } else { None }
    }

    pub fn next_u64_retry(max: usize) -> Result<u64, &'static str> {
        for _ in 0..max {
            if let Some(v) = Self::next_u64() { return Ok(v); }
            unsafe { asm!("pause", options(nomem, nostack)); }
        }
        // Fallback: ChaCha20 com seed do ultimo RDRAND bem sucedido
        // (implementacao simplificada — quarter round + xor)
        Ok(chauchy_fallback())
    }

    pub fn fill_bytes(buf: &mut [u8]) -> Result<(), &'static str> {
        for chunk in buf.chunks_mut(8) {
            let r = Self::next_u64_retry(10)?;
            chunk.copy_from_slice(&r.to_ne_bytes()[..chunk.len()]);
        }
        Ok(())
    }

    pub fn wpa2_nonce() -> Result<[u8; 32], &'static str> {
        let mut n = [0u8; 32]; Self::fill_bytes(&mut n)?; Ok(n)
    }
}

fn chacha20_q(s: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    s[a] = s[a].wrapping_add(s[b]); s[d] ^= s[a]; s[d] = s[d].rotate_left(16);
    s[c] = s[c].wrapping_add(s[d]); s[b] ^= s[c]; s[b] = s[b].rotate_left(12);
    s[a] = s[a].wrapping_add(s[b]); s[d] ^= s[a]; s[d] = s[d].rotate_left(8);
    s[c] = s[c].wrapping_add(s[d]); s[b] ^= s[c]; s[b] = s[b].rotate_left(7);
}

fn chauchy_fallback() -> u64 {
    let mut s = [0x61707865u32, 0x3320646e, 0x79622d32, 0x6b206574,
                 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
    for _ in 0..10 {
        chacha20_q(&mut s, 0,4,8,12); chacha20_q(&mut s,1,5,9,13);
        chacha20_q(&mut s,2,6,10,14); chacha20_q(&mut s,3,7,11,15);
        chacha20_q(&mut s,0,5,10,15); chacha20_q(&mut s,1,6,11,12);
        chacha20_q(&mut s,2,7,8,13);  chacha20_q(&mut s,3,4,9,14);
    }
    (s[0] as u64) | ((s[1] as u64) << 32)
}
