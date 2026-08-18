//! Superbloco NeuralFS — formato, leitura, escrita, formatacao.
//! 4096 bytes (8 setores). Bloco 1 = primario, Bloco 2 = backup.

use crate::block_dev::BlockDevice;

pub const SUPERBLOCK_MAGIC: [u8; 8] = *b"NEURALFS";
pub const SUPERBLOCK_VERSION: u32 = 1;
pub const BLOCK_SIZE: u64 = 4096;
pub const BLOCK_SIZE_LOG2: u8 = 12;

#[derive(Debug, Clone)]
pub struct Superblock {
    pub magic: [u8; 8],
    pub version: u32,
    pub total_blocks: u64,
    pub free_blocks: u64,
    pub allocated_inodes: u64,
    pub last_tx_id: u64,
    pub root_inode: u64,
    pub inode_tree_root: u64,
    pub free_extent_root: u64,
    /// F4/F4b: checksums implementados na MESMA b-tree via ItemType::Checksum
    /// (0x05) — F4 = CRC32C do arquivo no inode (bytes 22..26, read_file +
    /// verify_file); F4b = CRC32C POR PÁGINA (key (ino, Checksum, bloco),
    /// verificado no read_range streaming). Este campo fica 0: a "árvore de
    /// checksums" não é um root separado — os items convivem com inodes/extents
    /// na inode_tree_root, ordenados por (object_id, item_type, offset).
    pub checksum_tree_root: u64,
    pub journal_start: u64,
    pub journal_blocks: u64,
    pub uuid: [u64; 2],
    pub label: [u64; 4],
    pub next_cow_block: u64,
}

impl Superblock {
    // F13: `Superblock::new` removido — era morto e com layout divergente do
    // format() (journal_blocks/free_blocks/next_cow diferentes). Duas fontes de
    // verdade do layout on-disk quebrariam o disco. O formato real vive no
    // `NeuralVolume::format` (volume.rs).

    fn read_block(dev: &mut dyn BlockDevice, start_lba: u64, block_num: u64) -> Option<[u8; 4096]> {
        let block_lba = block_num.checked_mul(8);
        if block_lba.is_none() { crate::slog_nano!("NEURALFS", "info", "read_block FAIL: checked_mul block={}", block_num); return None; }
        let block_lba = block_lba.unwrap();
        let mut block = [0u8; 4096];
        for i in 0..8 {
            let off = i * 512;
            let sum = block_lba.checked_add(i as u64);
            if sum.is_none() { crate::slog_nano!("NEURALFS", "info", "read_block FAIL: checked_add1 block={} i={}", block_num, i); return None; }
            let lba = start_lba.checked_add(sum.unwrap());
            if lba.is_none() { crate::slog_nano!("NEURALFS", "info", "read_block FAIL: checked_add2 block={} i={}", block_num, i); return None; }
            if !dev.read_sectors(lba.unwrap(), &mut block[off..off + 512]) {
                crate::slog_nano!("NEURALFS", "info", "read_block FAIL: read_sectors block={} i={} lba={}", block_num, i, lba.unwrap());
                return None;
            }
        }
        if &block[0..8] != SUPERBLOCK_MAGIC {
            crate::slog_nano!("NEURALFS", "info", "read_block: magic FAIL block={} (got={:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x})",
                block_num, block[0], block[1], block[2], block[3], block[4], block[5], block[6], block[7]);
            return None;
        }
        let stored_crc = u32::from_le_bytes([block[12], block[13], block[14], block[15]]);
        block[12..16].copy_from_slice(&0u32.to_le_bytes());
        let computed_crc = crate::neural_fs::checksum::crc32c(&block[4..4096]);
        if stored_crc != computed_crc {
            crate::slog_nano!("NEURALFS", "info", "read_block: CRC FAIL block={} stored={:#x} computed={:#x}", block_num, stored_crc, computed_crc);
            return None;
        }
        Some(block)
    }

    pub fn read(dev: &mut dyn BlockDevice, start_lba: u64) -> Option<Self> {
        let block = Self::read_block(dev, start_lba, 1)
            .or_else(|| Self::read_block(dev, start_lba, 2))?;
        Some(Superblock {
            magic: SUPERBLOCK_MAGIC,
            version: u32::from_le_bytes([block[8], block[9], block[10], block[11]]),
            total_blocks: u64::from_le_bytes(block[16..24].try_into().unwrap()),
            free_blocks: u64::from_le_bytes(block[24..32].try_into().unwrap()),
            allocated_inodes: u64::from_le_bytes(block[32..40].try_into().unwrap()),
            last_tx_id: u64::from_le_bytes(block[40..48].try_into().unwrap()),
            root_inode: u64::from_le_bytes(block[56..64].try_into().unwrap()),
            inode_tree_root: u64::from_le_bytes(block[64..72].try_into().unwrap()),
            free_extent_root: u64::from_le_bytes(block[72..80].try_into().unwrap()),
            checksum_tree_root: u64::from_le_bytes(block[80..88].try_into().unwrap()),
            journal_start: u64::from_le_bytes(block[88..96].try_into().unwrap()),
            journal_blocks: u64::from_le_bytes(block[96..104].try_into().unwrap()),
            uuid: [u64::from_le_bytes(block[104..112].try_into().unwrap()),
                   u64::from_le_bytes(block[112..120].try_into().unwrap())],
            label: [0; 4],
            next_cow_block: u64::from_le_bytes(block[120..128].try_into().unwrap()),
        })
    }

    fn encode_block(&self) -> [u8; 4096] {
        let mut block = [0u8; 4096];
        block[0..8].copy_from_slice(&SUPERBLOCK_MAGIC);
        block[8..12].copy_from_slice(&self.version.to_le_bytes());
        block[12..16].copy_from_slice(&0u32.to_le_bytes());
        block[16..24].copy_from_slice(&self.total_blocks.to_le_bytes());
        block[24..32].copy_from_slice(&self.free_blocks.to_le_bytes());
        block[32..40].copy_from_slice(&self.allocated_inodes.to_le_bytes());
        block[40..48].copy_from_slice(&self.last_tx_id.to_le_bytes());
        block[56..64].copy_from_slice(&self.root_inode.to_le_bytes());
        block[64..72].copy_from_slice(&self.inode_tree_root.to_le_bytes());
        block[72..80].copy_from_slice(&self.free_extent_root.to_le_bytes());
        block[80..88].copy_from_slice(&self.checksum_tree_root.to_le_bytes());
        block[88..96].copy_from_slice(&self.journal_start.to_le_bytes());
        block[96..104].copy_from_slice(&self.journal_blocks.to_le_bytes());
        block[104..112].copy_from_slice(&self.uuid[0].to_le_bytes());
        block[112..120].copy_from_slice(&self.uuid[1].to_le_bytes());
        block[120..128].copy_from_slice(&self.next_cow_block.to_le_bytes());
        let crc = crate::neural_fs::checksum::crc32c(&block[4..4096]);
        block[12..16].copy_from_slice(&crc.to_le_bytes());
        block
    }

    fn write_block(dev: &mut dyn BlockDevice, start_lba: u64, block_num: u64, data: &[u8; 4096]) -> bool {
        let block_lba = block_num.checked_mul(8).unwrap_or(u64::MAX);
        if block_lba == u64::MAX { return false; }
        for i in 0..8 {
            let off = i * 512;
            let lba = start_lba.checked_add(block_lba.checked_add(i as u64).unwrap_or(u64::MAX)).unwrap_or(u64::MAX);
            if lba == u64::MAX { return false; }
            if !dev.write_sectors(lba, &data[off..off + 512]) {
                return false;
            }
        }
        true
    }

    pub fn write(&self, dev: &mut dyn BlockDevice, start_lba: u64) -> bool {
        let block = self.encode_block();
        // Escreve primario (bloco 1) e backup (bloco 2)
        Self::write_block(dev, start_lba, 1, &block)
            && Self::write_block(dev, start_lba, 2, &block)
    }
}
