//! Checksum tree — armazena CRC32C checksums por bloco de dados.
//! Key: (object_id=inode, item_type=Checksum, offset=block_addr).
//! Value: u32 CRC32C.

use super::btree::{Key, ItemType};

pub struct ChecksumEntry {
    pub block_addr: u64,
    pub checksum: u32,
}

impl ChecksumEntry {
    pub fn new(block_addr: u64, checksum: u32) -> Self {
        ChecksumEntry { block_addr, checksum }
    }

    pub fn make_key(inode: u64, block_addr: u64) -> Key {
        Key {
            object_id: inode,
            item_type: ItemType::Checksum,
            offset: block_addr,
        }
    }

    pub fn value_to_bytes(crc: u32) -> [u8; 4] {
        crc.to_le_bytes()
    }

    pub fn value_from_bytes(b: &[u8]) -> u32 {
        u32::from_le_bytes([b[0], b[1], b[2], b[3]])
    }
}








