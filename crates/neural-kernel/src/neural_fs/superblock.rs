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
    pub checksum_tree_root: u64,
    pub journal_start: u64,
    pub journal_blocks: u64,
    pub uuid: [u64; 2],
    pub label: [u64; 4],
    pub next_cow_block: u64,
}

impl Superblock {
    pub fn new(total_blocks: u64) -> Self {
        let journal_blocks = (total_blocks / 100).max(256).min(16384);
        let next_cow = 2 + journal_blocks;
        Superblock {
            magic: SUPERBLOCK_MAGIC, version: SUPERBLOCK_VERSION,
            total_blocks, free_blocks: total_blocks - next_cow - 1,
            allocated_inodes: 1, last_tx_id: 0, root_inode: 1,
            inode_tree_root: next_cow, free_extent_root: 0,
            checksum_tree_root: 0, journal_start: 3, journal_blocks,
            uuid: [0x4E55, 0x52414C], label: [0; 4],
            next_cow_block: next_cow + 1,
        }
    }

    fn read_block(dev: &mut dyn BlockDevice, start_lba: u64, block_num: u64) -> Option<[u8; 4096]> {
        let block_lba = block_num.checked_mul(8)?;
        let mut block = [0u8; 4096];
        for i in 0..8 {
            let off = i * 512;
            let lba = start_lba.checked_add(block_lba.checked_add(i as u64)?)?;
            if !dev.read_sectors(lba, &mut block[off..off + 512]) {
                return None;
            }
        }
        if &block[0..8] != SUPERBLOCK_MAGIC { return None; }
        let stored_crc = u32::from_le_bytes([block[12], block[13], block[14], block[15]]);
        let computed_crc = crate::neural_fs::checksum::crc32c(&block[4..4096]);
        if stored_crc != computed_crc { return None; }
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
