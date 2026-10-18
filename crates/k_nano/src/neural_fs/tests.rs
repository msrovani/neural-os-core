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

/// F4b: árvore de checksums POR BLOCO (ItemType::Checksum) — write grava um
/// item por página, read_range verifica página a página e detecta bit-flip no
/// bloco corrompido SEM reler o arquivo inteiro; range limpo continua OK.
pub fn smoke_block_crc_tree() -> bool {
    let mut disk = MemoryDisk::new(4 * 1024 * 1024);
    let total = disk.sector_count();
    if !NeuralVolume::format(&mut disk, 0, total) {
        return false;
    }
    let Some(mut vol) = NeuralVolume::mount(&mut disk, 0) else {
        return false;
    };
    let Ok(ino) = vol.create_file(&mut disk, 1, "blk.bin") else {
        return false;
    };
    // 2 blocos cheios + 1 parcial (última página com padding — o CRC da
    // página inclui o padding, determinístico).
    let payload = alloc::vec![0x7Bu8; 9000];
    if vol.write_file(&mut disk, ino, &payload).is_err() {
        return false;
    }

    // read_range íntegro (3 páginas) devolve exatamente o payload.
    if vol.read_range(&mut disk, ino, 0, 9000).map(|d| d == payload).unwrap_or(false) == false {
        return false;
    }
    // Range parcial cruzando fronteira de bloco também OK.
    if vol.read_range(&mut disk, ino, 4000, 1000).map(|d| d == &payload[4000..5000]).unwrap_or(false) == false {
        return false;
    }

    // Corrompe um byte no MEIO da 2ª página (bloco 1 do extent).
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
    let corrupt_block = start + 1;
    disk.data[corrupt_block as usize * 4096 + 333] ^= 0xFF;

    // Range que cruza o bloco corrompido → recusa "block crc mismatch".
    match vol.read_range(&mut disk, ino, 4096, 4096) {
        Err("block crc mismatch") => {}
        _ => return false,
    }
    // Range que NÃO toca o bloco corrompido continua OK (bloco 0 íntegro).
    if vol.read_range(&mut disk, ino, 0, 4096).map(|d| d == &payload[..4096]).unwrap_or(false) == false {
        return false;
    }
    true
}

#[cfg(test)]
#[test]
fn block_crc_tree_detects_corruption() {
    assert!(smoke_block_crc_tree(), "smoke_block_crc_tree falhou");
}

/// s493: write_file_stream roundtrip (F1 anti-OOM path).
#[cfg(test)]
#[test]
fn write_file_stream_roundtrip() {
    let mut disk = MemoryDisk::new(8 * 1024 * 1024);
    let total = disk.sector_count();
    assert!(NeuralVolume::format(&mut disk, 0, total));
    let mut vol = NeuralVolume::mount(&mut disk, 0).expect("mount");
    let ino = vol.create_file(&mut disk, 1, "s.bin").expect("create");
    let payload: alloc::vec::Vec<u8> = (0..50_000u32).map(|i| (i % 251) as u8).collect();
    let mut off = 0usize;
    vol.write_file_stream(&mut disk, ino, payload.len(), |buf| {
        let n = buf.len().min(payload.len() - off);
        buf[..n].copy_from_slice(&payload[off..off + n]);
        off += n;
        Ok(n)
    })
    .expect("stream write");
    let got = vol.read_file(&mut disk, ino).expect("read");
    assert_eq!(got, payload);
}

/// Host seed file-backed (multi-GB): `MKNEURALFS_OUT` + `MKNEURALFS_MANIFEST` + size.
/// Evita MemoryDisk (OOM / STATUS_PRIVILEGED_INSTRUCTION no Windows com blobs GB).
#[cfg(test)]
struct FileDisk {
    f: std::fs::File,
    sectors: u64,
}

#[cfg(test)]
impl FileDisk {
    fn create(path: &str, size_bytes: u64) -> Self {
        use std::io::{Seek, SeekFrom, Write};
        // Preferir ficheiro sparse pré-criado (mkneuralfs.py / fsutil) — set_len
        // denso no Windows em multi-GB = OOM / STATUS_PRIVILEGED_INSTRUCTION.
        let pre = std::path::Path::new(path).exists()
            && std::fs::metadata(path).map(|m| m.len() == size_bytes).unwrap_or(false);
        let f = if pre {
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(path)
                .expect("open sparse disk")
        } else {
            let f = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(path)
                .expect("create disk file");
            f.set_len(size_bytes).expect("set_len");
            let _ = f.sync_all();
            f
        };
        let sectors = size_bytes / 512;
        let mut d = FileDisk { f, sectors };
        let mut mbr = [0u8; 512];
        mbr[510] = 0x55;
        mbr[511] = 0xAA;
        let off = 446usize;
        mbr[off + 4] = 0x7F;
        let part_lba: u32 = 2048;
        mbr[off + 8..off + 12].copy_from_slice(&part_lba.to_le_bytes());
        let sectors_u32 = (sectors as u32).saturating_sub(part_lba);
        mbr[off + 12..off + 16].copy_from_slice(&sectors_u32.to_le_bytes());
        d.f.seek(SeekFrom::Start(0)).expect("seek mbr");
        d.f.write_all(&mbr).expect("write mbr");
        let _ = d.f.flush();
        d
    }
}

#[cfg(test)]
impl crate::block_dev::BlockDevice for FileDisk {
    fn read_sectors(&mut self, lba: u64, buf: &mut [u8]) -> bool {
        use std::io::{Read, Seek, SeekFrom};
        if buf.len() % 512 != 0 || lba + (buf.len() as u64 / 512) > self.sectors {
            return false;
        }
        if self.f.seek(SeekFrom::Start(lba * 512)).is_err() {
            return false;
        }
        self.f.read_exact(buf).is_ok()
    }
    fn write_sectors(&mut self, lba: u64, buf: &[u8]) -> bool {
        use std::io::{Seek, SeekFrom, Write};
        if buf.len() % 512 != 0 || lba + (buf.len() as u64 / 512) > self.sectors {
            return false;
        }
        if self.f.seek(SeekFrom::Start(lba * 512)).is_err() {
            return false;
        }
        self.f.write_all(buf).is_ok()
    }
    fn total_sectors(&self) -> u64 {
        self.sectors
    }
}

/// Host seed: `MKNEURALFS_OUT=path MKNEURALFS_MANIFEST=lines cargo test … export_lab_seed -- --ignored`
#[cfg(test)]
#[test]
#[ignore]
fn export_lab_seed() {
    use std::io::{Read, Write};
    let out = std::env::var("MKNEURALFS_OUT").expect("MKNEURALFS_OUT");
    let size_mb: u64 = std::env::var("MKNEURALFS_SIZE_MB")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(512);
    let part_lba: u64 = 2048;
    let total_bytes = size_mb * 1024 * 1024;
    let mut disk = FileDisk::create(&out, total_bytes);
    let total_lba = disk.sectors.saturating_sub(part_lba);
    assert!(
        NeuralVolume::format(&mut disk, part_lba, total_lba),
        "format"
    );
    let mut vol = NeuralVolume::mount(&mut disk, part_lba).expect("mount");
    let eco = vol.create_dir(&mut disk, 1, "ecosystem").expect("eco");
    let models = vol.create_dir(&mut disk, eco, "models").expect("models");
    let _fw = vol.create_dir(&mut disk, eco, "firmware");
    let _logs = vol.create_dir(&mut disk, 1, "logs");

    if let Ok(man) = std::env::var("MKNEURALFS_MANIFEST") {
        for line in std::fs::read_to_string(&man).unwrap_or_default().lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((nrfs_path, host_path)) = line.split_once('|') else {
                continue;
            };
            let host_path = host_path.trim();
            let name = nrfs_path.trim().rsplit('/').next().unwrap_or(nrfs_path);
            if name.len() > 22 {
                eprintln!("skip name too long {}", name);
                continue;
            }
            let Ok(meta) = std::fs::metadata(host_path) else {
                eprintln!("skip missing {}", host_path);
                continue;
            };
            let file_len = meta.len() as usize;
            let Ok(mut src) = std::fs::File::open(host_path) else {
                continue;
            };
            let Ok(ino) = vol.create_file(&mut disk, models, name) else {
                eprintln!("create fail {}", name);
                continue;
            };
            let mut remaining = file_len;
            // Lab seed: sem F4b page-checksum tree (multi-GB).
            let wr = vol.write_file_stream_ex(&mut disk, ino, file_len, false, |buf| {
                let n = buf.len().min(remaining);
                src.read_exact(&mut buf[..n]).map_err(|_| "read")?;
                remaining -= n;
                Ok(n)
            });
            if let Err(e) = wr {
                eprintln!("write fail {} err={}", name, e);
                continue;
            }
            eprintln!("ok {} <- {} ({} MB)", name, host_path, file_len / (1024 * 1024));
        }
    } else {
        let ino = vol.create_file(&mut disk, models, "active.v7").expect("marker");
        let marker = b"NEURALFS-SEED-EMPTY\n";
        vol.write_file(&mut disk, ino, marker).expect("write marker");
    }
    let _ = disk.f.flush();
    let _ = disk.f.sync_all();
    eprintln!("wrote {} ({} MB)", out, size_mb);
}
