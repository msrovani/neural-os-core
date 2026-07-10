//! B-tree CoW para NeuralFS. Formato unificado para inodes, diretorios, extents.
//! Cada no = 4096 bytes, CRC32C no cabecalho, niveis 0 (leaf) ate N (internal).

use crate::block_dev::BlockDevice;

pub const BTREE_ORDER: usize = 32; // max items per node (internal: 32 keys, 33 children)
pub const KEY_SIZE: usize = 17;    // object_id(8) + item_type(1) + offset(8)

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum ItemType {
    Inode = 0x01,
    DirEntry = 0x02,
    FileExtent = 0x03,
    FreeExtent = 0x04,
    Checksum = 0x05,
}

pub type ObjectId = u64;

#[derive(Debug, Clone, Copy)]
pub struct Key {
    pub object_id: ObjectId,
    pub item_type: ItemType,
    pub offset: u64,
}

impl Key {
    pub fn from_bytes(b: &[u8]) -> Self {
        Key {
            object_id: u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]),
            item_type: match b[8] {
                0x01 => ItemType::Inode,
                0x02 => ItemType::DirEntry,
                0x03 => ItemType::FileExtent,
                0x04 => ItemType::FreeExtent,
                _ => ItemType::Checksum,
            },
            offset: u64::from_le_bytes([b[9], b[10], b[11], b[12], b[13], b[14], b[15], b[16]]),
        }
    }

    pub fn to_bytes(&self, b: &mut [u8]) {
        b[0..8].copy_from_slice(&self.object_id.to_le_bytes());
        b[8] = self.item_type as u8;
        b[9..17].copy_from_slice(&self.offset.to_le_bytes());
    }

    pub fn cmp(&self, other: &Key) -> core::cmp::Ordering {
        match self.object_id.cmp(&other.object_id) {
            core::cmp::Ordering::Equal => {
                match (self.item_type as u8).cmp(&(other.item_type as u8)) {
                    core::cmp::Ordering::Equal => self.offset.cmp(&other.offset),
                    o => o,
                }
            }
            o => o,
        }
    }
}

pub struct BTreeNode {
    pub block_addr: u64,
    pub data: [u8; 4096],
}

impl BTreeNode {
    pub fn new(block_addr: u64) -> Self {
        let mut node = BTreeNode { block_addr, data: [0u8; 4096] };
        node.data[4] = 0; // level 0 (leaf)
        node.data[6..8].copy_from_slice(&[0, 0]); // item_count = 0
        node.data[8..16].copy_from_slice(&block_addr.to_le_bytes());
        node
    }

    pub fn level(&self) -> u8 { self.data[4] }
    pub fn item_count(&self) -> u16 { u16::from_le_bytes([self.data[6], self.data[7]]) }
    pub fn set_item_count(&mut self, n: u16) { self.data[6..8].copy_from_slice(&n.to_le_bytes()); }
    pub fn generation(&self) -> u64 { u64::from_le_bytes([self.data[16], self.data[17], self.data[18], self.data[19], self.data[20], self.data[21], self.data[22], self.data[23]]) }
    pub fn set_generation(&mut self, gen: u64) { self.data[16..24].copy_from_slice(&gen.to_le_bytes()); }

    pub fn compute_checksum(&self) -> u32 {
        crate::neural_fs::checksum::crc32c(&self.data[4..4096])
    }

    pub fn write_checksum(&mut self) {
        let crc = self.compute_checksum();
        self.data[0..4].copy_from_slice(&crc.to_le_bytes());
    }

    pub fn read(dev: &mut dyn BlockDevice, start_lba: u64, block_addr: u64) -> Option<Self> {
        let mut node = BTreeNode { block_addr, data: [0u8; 4096] };
        for i in 0..8usize {
            let lba = start_lba + block_addr * 8 + i as u64;
            let off = i * 512;
            if !dev.read_sectors(lba, &mut node.data[off..off + 512]) {
                return None;
            }
        }
        Some(node)
    }

    pub fn write(&self, dev: &mut dyn BlockDevice, start_lba: u64) -> bool {
        for i in 0..8usize {
            let lba = start_lba + self.block_addr * 8 + i as u64;
            let off = i * 512;
            let sector: &[u8] = &self.data[off..off + 512];
            if !dev.write_sectors(lba, sector) {
                return false;
            }
        }
        true
    }

    /// Retorna slice de bytes do item no indice `idx`
    pub fn get_item(&self, idx: usize) -> Option<&[u8]> {
        let count = self.item_count() as usize;
        if idx >= count { return None; }
        // Items no header (leaf): key(17) + value(15) = 32 bytes cada
        // Internal: key(17) + child_ptr(8) + child_gen(8) = 33 bytes cada
        let item_size: usize = if self.level() == 0 { 32 } else { 33 };
        let off = 24 + idx * item_size;
        Some(&self.data[off..off + item_size])
    }

    pub fn find_key(&self, key: &Key) -> Result<usize, usize> {
        let count = self.item_count() as usize;
        for i in 0..count {
            let item = self.get_item(i).unwrap();
            let k = Key::from_bytes(item);
            match k.cmp(key) {
                core::cmp::Ordering::Equal => return Ok(i),
                core::cmp::Ordering::Greater => return Err(i),
                _ => {}
            }
        }
        Err(count)
    }
}
