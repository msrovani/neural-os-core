//! Diretorio — entradas de diretorio com hash xxHash-64 para lookup rapido.
//! Cada entrada: xxhash(64) do nome (8) + inode (8) + nome (ate 240 bytes) = 256 bytes.

use alloc::vec;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use super::btree::{Key, ItemType};

pub const DIR_ENTRY_SIZE: usize = 256;

#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name_hash: u64,
    pub inode: u64,
    pub name: String,
}

impl DirEntry {
    pub fn new(name: &str, inode: u64) -> Self {
        DirEntry {
            name_hash: xxhash64(name.as_bytes()),
            inode,
            name: String::from(name),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut b = vec![0u8; DIR_ENTRY_SIZE];
        b[0..8].copy_from_slice(&self.name_hash.to_le_bytes());
        b[8..16].copy_from_slice(&self.inode.to_le_bytes());
        let name_bytes = self.name.as_bytes();
        let len = name_bytes.len().min(239);
        b[16] = len as u8;
        b[17..17 + len].copy_from_slice(&name_bytes[..len]);
        b
    }

    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() < 16 { return None; }
        let hash = u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]);
        let inode = u64::from_le_bytes([b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]]);
        let name_len = (b[16] as usize).min(239);
        let name = core::str::from_utf8(&b[17..17 + name_len]).unwrap_or("").to_string();
        Some(DirEntry { name_hash: hash, inode, name })
    }

    pub fn make_key(parent_inode: u64, name: &str) -> Key {
        Key {
            object_id: parent_inode,
            item_type: ItemType::DirEntry,
            offset: xxhash64(name.as_bytes()),
        }
    }
}

fn xxhash64(data: &[u8]) -> u64 {
    const PRIME1: u64 = 0x9E3779B185EBCA87;
    const PRIME2: u64 = 0xC2B2AE3D27D4EB4F;
    const PRIME5: u64 = 0x165667B19E3779F9;
    let mut h = PRIME5.wrapping_add(data.len() as u64);
    let mut i = 0;
    while i + 8 <= data.len() {
        let v = u64::from_le_bytes([data[i], data[i+1], data[i+2], data[i+3], data[i+4], data[i+5], data[i+6], data[i+7]]);
        h = h.wrapping_add(v.wrapping_mul(PRIME2));
        h = (h.rotate_left(31)).wrapping_mul(PRIME1);
        i += 8;
    }
    while i < data.len() {
        h = h.wrapping_add((data[i] as u64).wrapping_mul(PRIME5));
        h = (h.rotate_left(11)).wrapping_mul(PRIME1);
        i += 1;
    }
    h ^= h >> 33;
    h = h.wrapping_mul(PRIME2);
    h ^= h >> 29;
    h = h.wrapping_mul(PRIME3);
    h ^= h >> 32;
    h
}
const PRIME3: u64 = 0x85EBCA6B;

/// Converte diretorio (lista de entradas) para string legivel
pub fn dir_list_to_string(entries: &[DirEntry]) -> String {
    let mut s = String::new();
    for e in entries {
        s.push_str(&alloc::format!("  {} (inode {})\n", e.name, e.inode));
    }
    s
}
