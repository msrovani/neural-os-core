//! Testes NeuralFS — MemoryDisk (ativar com cfg(test) se lib target existir).
//! Smoke host: `python tools/test_neuralfs_smoke.py` espelha format/create/read.

#![allow(dead_code)]

use crate::neural_fs::volume::{MemoryDisk, NeuralVolume};

/// Smoke in-kernel (chamavel do boot se desejado).
pub fn smoke_ram_roundtrip() -> bool {
    let mut disk = MemoryDisk::new(4 * 1024 * 1024);
    let total = disk.sector_count();
    if !NeuralVolume::format(&mut disk, 0, total) {
        return false;
    }
    let Some(mut vol) = NeuralVolume::mount(&mut disk, 0) else {
        return false;
    };
    let Ok(ino) = vol.create_file(&mut disk, 1, "t.txt") else {
        return false;
    };
    if vol.write_file(&mut disk, ino, b"ok").is_err() {
        return false;
    }
    match vol.read_file(&mut disk, ino) {
        Ok(d) => d == b"ok",
        Err(_) => false,
    }
}

/// Reclaim: reescrever arquivo deve reutilizar blocos via free_stack.
pub fn smoke_reclaim() -> bool {
    let mut disk = MemoryDisk::new(4 * 1024 * 1024);
    let total = disk.sector_count();
    if !NeuralVolume::format(&mut disk, 0, total) {
        return false;
    }
    let Some(mut vol) = NeuralVolume::mount(&mut disk, 0) else {
        return false;
    };
    let Ok(ino) = vol.create_file(&mut disk, 1, "r.txt") else {
        return false;
    };
    let big = alloc::vec![0xABu8; 8192];
    if vol.write_file(&mut disk, ino, &big).is_err() {
        return false;
    }
    let after_first = vol.sb.next_cow_block;
    if vol.write_file(&mut disk, ino, b"small").is_err() {
        return false;
    }
    vol.sb.next_cow_block <= after_first + 8
}

/// Split: muitos arquivos forcam folha cheia → root level >= 1.
pub fn smoke_split() -> bool {
    let mut disk = MemoryDisk::new(8 * 1024 * 1024);
    let total = disk.sector_count();
    if !NeuralVolume::format(&mut disk, 0, total) {
        return false;
    }
    let Some(mut vol) = NeuralVolume::mount(&mut disk, 0) else {
        return false;
    };
    for i in 0..50u32 {
        let name = alloc::format!("f{:03}", i);
        if vol.create_file(&mut disk, 1, &name).is_err() {
            return false;
        }
    }
    let root = crate::neural_fs::btree::BTreeNode::read(&mut disk, 0, vol.sb.inode_tree_root);
    root.map(|n| n.level() >= 1).unwrap_or(false)
}

/// Multi-nivel: 200 keys forcam splits; nao pode falhar com "parent full".
pub fn smoke_multilevel() -> bool {
    let mut disk = MemoryDisk::new(16 * 1024 * 1024);
    let total = disk.sector_count();
    if !NeuralVolume::format(&mut disk, 0, total) {
        return false;
    }
    let Some(mut vol) = NeuralVolume::mount(&mut disk, 0) else {
        return false;
    };
    match vol.test_insert_many(&mut disk, 200) {
        Ok(level) => level >= 1,
        Err(_) => false,
    }
}
