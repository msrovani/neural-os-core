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
        if name_bytes.len() > 239 {
            return Vec::new(); // nome muito longo, falha
        }
        b[16] = name_bytes.len() as u8;
        b[17..17 + name_bytes.len()].copy_from_slice(&name_bytes);
        let crc = crate::neural_fs::checksum::crc32c(&b[0..248]);
        b[248..252].copy_from_slice(&crc.to_le_bytes());
        b
    }

    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() < 252 { return None; }
        let stored_crc = u32::from_le_bytes([b[248], b[249], b[250], b[251]]);
        let computed_crc = crate::neural_fs::checksum::crc32c(&b[0..248]);
        if stored_crc != computed_crc { return None; }
        let hash = u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]);
        let inode = u64::from_le_bytes([b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]]);
        let name_len = (b[16] as usize).min(239);
        if name_len == 0 { return Some(DirEntry { name_hash: hash, inode, name: String::new() }); }
        let name = core::str::from_utf8(&b[17..17 + name_len])
            .map(|s| s.to_string())
            .unwrap_or_else(|_| {
                // Fallback: substitui UTF-8 invalido por replacement character
                alloc::string::String::from_utf8_lossy(&b[17..17 + name_len]).into_owned()
            });
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








