//! Testes NeuralFS — MemoryDisk (ativar com cfg(test) se lib target existir).
//! Smoke host: `python tools/test_neuralfs_smoke.py` espelha format/create/read.

#![allow(dead_code)]

use crate::neural_fs::btree::{btree_lookup, ItemType, Key};
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

/// Stress B-tree: milhares de keys → root level >= 2 (Onda 1 evidência).
/// Com 84 items/folha e chaves monotônicas, nivel-2 requer ~3528+ items
/// (85 splits p/ encher no interno). 4000 é seguro.
pub fn smoke_level2() -> bool {
    let mut disk = MemoryDisk::new(64 * 1024 * 1024);
    let total = disk.sector_count();
    if !NeuralVolume::format(&mut disk, 0, total) {
        return false;
    }
    let Some(mut vol) = NeuralVolume::mount(&mut disk, 0) else {
        return false;
    };
    match vol.test_insert_many(&mut disk, 4000) {
        Ok(level) => level >= 2,
        Err(e) => {
            crate::slog_nano!("NEURALFS", "info", "smoke_level2 erro={}", e);
            false
        }
    }
}

/// F4/redoxfs: data checksums — write grava CRC32C no inode, read_file verifica
/// e verify_file detecta corrupção de bloco de dados (bit flip).
pub fn smoke_data_crc() -> bool {
    let mut disk = MemoryDisk::new(4 * 1024 * 1024);
    let total = disk.sector_count();
    if !NeuralVolume::format(&mut disk, 0, total) {
        return false;
    }
    let Some(mut vol) = NeuralVolume::mount(&mut disk, 0) else {
        return false;
    };
    let Ok(ino) = vol.create_file(&mut disk, 1, "crc.bin") else {
        return false;
    };
    // 3 blocos: 2 cheios + 1 parcial (exercita o span exato do CRC).
    let payload = alloc::vec![0x5Au8; 9000];
    if vol.write_file(&mut disk, ino, &payload).is_err() {
        return false;
    }
    if !vol.verify_file(&mut disk, ino).unwrap_or(false) {
        return false;
    }
    if vol.read_file(&mut disk, ino).map(|d| d == payload).unwrap_or(false) == false {
        return false;
    }

    // Corrompe um byte no meio de um bloco de dados (o 2º do extent).
    let Some((start, count)) = btree_lookup(
        &mut disk,
        0,
        vol.sb.inode_tree_root,
        &Key {
            object_id: ino,
            item_type: ItemType::FileExtent,
            offset: 0,
        },
    )
    .map(|v| v.as_extent())
    else {
        return false;
    };
    if count < 2 {
        return false;
    }
    let corrupt_block = start + count / 2;
    let byte_off = corrupt_block as usize * 4096 + 100; // bloco = 8 setores (start_lba=0)
    disk.data[byte_off] ^= 0xFF;

    // verify_file detecta; read_file recusa dados corrompidos.
    if vol.verify_file(&mut disk, ino).unwrap_or(true) {
        return false;
    }
    match vol.read_file(&mut disk, ino) {
        Err("data crc mismatch") => true,
        _ => false,
    }
}

#[cfg(test)]
#[test]
fn data_crc_detects_corruption() {
    assert!(smoke_data_crc(), "smoke_data_crc falhou");
}

#[cfg(test)]
#[test]
fn ram_roundtrip_ok() {
    assert!(smoke_ram_roundtrip());
}

/// Power-loss soft: write+commit → drop vol → remount → read (journal recover path).
/// USB power-cycle real fica AWAITING_HW ([NRFS-HW]).
pub fn smoke_power_loss_soft() -> bool {
    let mut disk = MemoryDisk::new(4 * 1024 * 1024);
    let total = disk.sector_count();
    if !NeuralVolume::format(&mut disk, 0, total) {
        return false;
    }
    {
        let Some(mut vol) = NeuralVolume::mount(&mut disk, 0) else {
            return false;
        };
        let Ok(ino) = vol.create_file(&mut disk, 1, "ploss.txt") else {
            return false;
        };
        if vol.write_file(&mut disk, ino, b"survive-reboot\n").is_err() {
            return false;
        }
        // drop vol — simula queda apos commit
    }
    let Some(vol) = NeuralVolume::mount(&mut disk, 0) else {
        return false;
    };
    let Some(ino) = vol.resolve_path(&mut disk, "ploss.txt") else {
        return false;
    };
    match vol.read_file(&mut disk, ino) {
        Ok(d) => d == b"survive-reboot\n",
        Err(_) => false,
    }
}
