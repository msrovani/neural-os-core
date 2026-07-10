//! Superbloco NeuralFS — formato, leitura, escrita, formatacao.
//! 512 bytes no primeiro bloco do FS, com backup no segundo bloco.

use crate::block_dev::BlockDevice;

pub const SUPERBLOCK_MAGIC: [u8; 8] = *b"NEURALFS";
pub const SUPERBLOCK_VERSION: u32 = 1;
pub const BLOCK_SIZE: u64 = 4096;
pub const BLOCK_SIZE_LOG2: u8 = 12; // 2^12 = 4096

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
        let next_cow = 2 + journal_blocks; // bloco 0=reserved, 1=super, 2=backup, 3+=journal
        Superblock {
            magic: SUPERBLOCK_MAGIC,
            version: SUPERBLOCK_VERSION,
            total_blocks,
            free_blocks: total_blocks - next_cow - 1,
            allocated_inodes: 1,
            last_tx_id: 0,
            root_inode: 1,
            inode_tree_root: next_cow,
            free_extent_root: 0,
            checksum_tree_root: 0,
            journal_start: 3,
            journal_blocks,
            uuid: [0x4E55, 0x52414C],
            label: [0; 4],
            next_cow_block: next_cow + 1,
        }
    }

    pub fn read(dev: &mut dyn BlockDevice, start_lba: u64) -> Option<Self> {
        let mut block = [0u8; 4096];
        for i in 0..8 {
            let off = i * 512;
            if !dev.read_sectors(start_lba + (1 * 8 + i) as u64, &mut block[off..off + 512]) {
                return None;
            }
        }
        if &block[0..8] != SUPERBLOCK_MAGIC { return None; }
        Some(Superblock {
            magic: SUPERBLOCK_MAGIC,
            version: u32::from_le_bytes([block[8], block[9], block[10], block[11]]),
            total_blocks: u64::from_le_bytes([block[16], block[17], block[18], block[19], block[20], block[21], block[22], block[23]]),
            free_blocks: u64::from_le_bytes([block[24], block[25], block[26], block[27], block[28], block[29], block[30], block[31]]),
            allocated_inodes: u64::from_le_bytes([block[32], block[33], block[34], block[35], block[36], block[37], block[38], block[39]]),
            last_tx_id: u64::from_le_bytes([block[40], block[41], block[42], block[43], block[44], block[45], block[46], block[47]]),
            root_inode: u64::from_le_bytes([block[56], block[57], block[58], block[59], block[60], block[61], block[62], block[63]]),
            inode_tree_root: u64::from_le_bytes([block[64], block[65], block[66], block[67], block[68], block[69], block[70], block[71]]),
            free_extent_root: u64::from_le_bytes([block[72], block[73], block[74], block[75], block[76], block[77], block[78], block[79]]),
            checksum_tree_root: u64::from_le_bytes([block[80], block[81], block[82], block[83], block[84], block[85], block[86], block[87]]),
            journal_start: u64::from_le_bytes([block[88], block[89], block[90], block[91], block[92], block[93], block[94], block[95]]),
            journal_blocks: u64::from_le_bytes([block[96], block[97], block[98], block[99], block[100], block[101], block[102], block[103]]),
            uuid: [u64::from_le_bytes([block[104], block[105], block[106], block[107], block[108], block[109], block[110], block[111]]),
                   u64::from_le_bytes([block[112], block[113], block[114], block[115], block[116], block[117], block[118], block[119]])],
            label: [0; 4],
            next_cow_block: u64::from_le_bytes([block[120], block[121], block[122], block[123], block[124], block[125], block[126], block[127]]),
        })
    }

    pub fn write(&self, dev: &mut dyn BlockDevice, start_lba: u64) -> bool {
        let mut block = [0u8; 4096];
        block[0..8].copy_from_slice(&SUPERBLOCK_MAGIC);
        block[8..12].copy_from_slice(&self.version.to_le_bytes());
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
        for i in 0..8 {
            let off = i * 512;
            if !dev.write_sectors(start_lba + (1 * 8 + i) as u64, &block[off..off + 512]) {
                return false;
            }
        }
        true
    }
}
