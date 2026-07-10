//! Journal — write-ahead log para crash recovery.
//! Non-circular v1: um commit por vez. Recovery: se tx_id > superblock.last_committed, replay.

use alloc::vec::Vec;
use crate::block_dev::BlockDevice;
use super::checksum;
use super::superblock::Superblock;

pub const JOURNAL_MAGIC: [u8; 8] = *b"NRFSJRNL";

pub struct Journal {
    pub tx_id: u64,
    pub dirty_blocks: Vec<(u64, [u8; 4096])>, // (block_addr, data)
}

impl Journal {
    pub fn new() -> Self {
        Journal { tx_id: 0, dirty_blocks: Vec::new() }
    }

    pub fn begin_tx(&mut self, tx_id: u64) {
        self.tx_id = tx_id;
        self.dirty_blocks.clear();
    }

    pub fn log_block(&mut self, block_addr: u64, data: &[u8; 4096]) {
        self.dirty_blocks.push((block_addr, *data));
    }

    /// Comita transacao: escreve journal no disco, depois atualiza superbloco.
    pub fn commit(&self, dev: &mut dyn BlockDevice, start_lba: u64, sb: &Superblock) -> bool {
        let count = self.dirty_blocks.len();
        if count == 0 { return true; }
        if count > sb.journal_blocks as usize { return false; }

        // Escreve header + blocos sujos no journal
        let journal_lba = start_lba + sb.journal_start * 8;
        let mut header = [0u8; 512];
        header[0..8].copy_from_slice(&JOURNAL_MAGIC);
        header[8..16].copy_from_slice(&self.tx_id.to_le_bytes());
        header[16..20].copy_from_slice(&(count as u32).to_le_bytes());

        for (i, (ba, data)) in self.dirty_blocks.iter().enumerate() {
            let off = 20 + i * 8;
            header[off..off + 8].copy_from_slice(&ba.to_le_bytes());

            let block_lba = journal_lba + 1 + i as u64 * 8;
            for s in 0..8usize {
                let sector: &[u8] = &data[s * 512..(s + 1) * 512];
                if !dev.write_sectors(block_lba + s as u64, sector) { return false; }
            }
        }

        let journal_crc = checksum::crc32c(&header[..20 + count * 8]);
        header[504..508].copy_from_slice(&journal_crc.to_le_bytes());

        let header_lba = journal_lba;
        for s in 0..1usize {
            if !dev.write_sectors(header_lba + s as u64, &header[s * 512..(s + 1) * 512]) {
                return false;
            }
        }

        true
    }

    /// Recovery: se ultima transacao do journal > superblock.last_tx, reescreve os blocos.
    pub fn recover(dev: &mut dyn BlockDevice, start_lba: u64, sb: &Superblock) -> bool {
        let journal_lba = start_lba + sb.journal_start * 8;
        let mut header = [0u8; 512];
        if !dev.read_sectors(journal_lba, &mut header) { return false; }
        if &header[0..8] != JOURNAL_MAGIC { return true; }

        let journal_tx = u64::from_le_bytes([header[8], header[9], header[10], header[11], header[12], header[13], header[14], header[15]]);
        if journal_tx <= sb.last_tx_id { return true; }

        let count = u32::from_le_bytes([header[16], header[17], header[18], header[19]]) as usize;
        if count == 0 || count > 16384 { return true; }

        // CRC check
        let stored_crc = u32::from_le_bytes([header[504], header[505], header[506], header[507]]);
        let computed_crc = checksum::crc32c(&header[..20 + count * 8]);
        if stored_crc != computed_crc { return false; }

        // Replay: reescreve blocos sujos
        for i in 0..count {
            let off = 20 + i * 8;
            let ba = u64::from_le_bytes([header[off], header[off+1], header[off+2], header[off+3], header[off+4], header[off+5], header[off+6], header[off+7]]);
            let block_lba = journal_lba + 1 + i as u64 * 8;
            let mut data = [0u8; 4096];
            for s in 0..8usize {
                if !dev.read_sectors(block_lba + s as u64, &mut data[s * 512..(s + 1) * 512]) {
                    return false;
                }
            }
            let target_lba = start_lba + ba * 8;
            for s in 0..8usize {
                if !dev.write_sectors(target_lba + s as u64, &data[s * 512..(s + 1) * 512]) {
                    return false;
                }
            }
        }
        true
    }
}
