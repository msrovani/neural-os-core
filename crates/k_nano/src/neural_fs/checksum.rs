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

/// CRC32C streaming (estado incremental) — permite verificar arquivos grandes
/// bloco a bloco sem materializar o buffer inteiro (padrão redoxfs verify).
#[derive(Clone, Copy)]
pub struct Crc32c(u32);

impl Crc32c {
    pub const fn new() -> Self {
        Crc32c(!0u32)
    }

    pub fn update(&mut self, data: &[u8]) {
        for &b in data {
            self.0 = CRC32C_TABLE[((self.0 as u8) ^ b) as usize] ^ (self.0 >> 8);
        }
    }

    pub fn finish(self) -> u32 {
        !self.0
    }
}

pub fn crc32c(data: &[u8]) -> u32 {
    let mut c = Crc32c::new();
    c.update(data);
    c.finish()
}

pub fn crc32c_block(block: &[u8; 4096]) -> u32 {
    crc32c(&block[4..4096])
}

pub fn verify_block(block: &[u8; 4096]) -> bool {
    let stored = u32::from_le_bytes([block[0], block[1], block[2], block[3]]);
    let computed = crc32c_block(block);
    stored == computed
}
