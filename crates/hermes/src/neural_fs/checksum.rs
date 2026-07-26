//! CRC32C (Castagnoli) com lookup table — checksums para NeuralFS.
//! Polinomio 0x1EDC6F41, tabela 256 entradas, ~20× mais rapido que bit-a-bit.

const POLY: u32 = 0x82F63B78;

static CRC32C_TABLE: [u32; 256] = make_table();

const fn make_table() -> [u32; 256] {
    let mut t = [0u32; 256];
    let mut i = 0u32;
    while i < 256 {
        let mut crc = i;
        let mut j = 0;
        while j < 8 {
            if crc & 1 != 0 { crc = (crc >> 1) ^ POLY; } else { crc >>= 1; }
            j += 1;
        }
        t[i as usize] = crc;
        i += 1;
    }
    t
}

pub fn crc32c(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in data {
        crc = CRC32C_TABLE[((crc as u8) ^ b) as usize] ^ (crc >> 8);
    }
    !crc
}

pub fn crc32c_block(block: &[u8; 4096]) -> u32 {
    crc32c(&block[4..4096])
}

pub fn verify_block(block: &[u8; 4096]) -> bool {
    let stored = u32::from_le_bytes([block[0], block[1], block[2], block[3]]);
    let computed = crc32c_block(block);
    stored == computed
}








