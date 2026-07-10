//! Extent — gerenciamento de extents (alocacao de blocos de dados).
//! Free-extent tree armazena extents livres. File extent tree armazena extents de arquivos.

use super::btree::{Key, ItemType};

#[derive(Debug, Clone, Copy)]
pub struct Extent {
    pub start_block: u64,
    pub block_count: u64,
}

impl Extent {
    pub const SIZE: usize = 16;

    pub fn new(start: u64, count: u64) -> Self {
        Extent { start_block: start, block_count: count }
    }

    pub fn to_bytes(&self) -> [u8; 16] {
        let mut b = [0u8; 16];
        b[0..8].copy_from_slice(&self.start_block.to_le_bytes());
        b[8..16].copy_from_slice(&self.block_count.to_le_bytes());
        b
    }

    pub fn from_bytes(b: &[u8]) -> Self {
        Extent {
            start_block: u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]),
            block_count: u64::from_le_bytes([b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]]),
        }
    }

    pub fn make_free_key(start_block: u64) -> Key {
        Key {
            object_id: 0,
            item_type: ItemType::FreeExtent,
            offset: start_block,
        }
    }

    pub fn make_file_key(inode: u64, offset: u64) -> Key {
        Key {
            object_id: inode,
            item_type: ItemType::FileExtent,
            offset,
        }
    }
}

/// Aloca blocos da free-extent tree (last-fit: do maior extent livre)
pub fn alloc_from_free_tree(tree_entries: &[Extent], count: u64) -> Option<(u64, Extent, Option<Extent>)> {
    let largest = tree_entries.iter().max_by_key(|e| e.block_count)?;
    if largest.block_count < count { return None; }
    let start = largest.start_block + largest.block_count - count; // last-fit
    let allocated = Extent::new(start, count);
    let remainder = if largest.block_count > count {
        Some(Extent::new(largest.start_block, largest.block_count - count))
    } else {
        None
    };
    Some((start, allocated, remainder))
}
