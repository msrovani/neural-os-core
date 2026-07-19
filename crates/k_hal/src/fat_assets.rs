//! Leitura de arquivos no root do volume de dados (FAT32 ou exFAT via ATA).
//! Usado por DeviceRecipe / LEGO boot (ADR-0056).

use alloc::string::String;
use alloc::vec::Vec;
use k_nano::block_dev::BlockDevice;

/// Lê um arquivo no root (nome 8.3 ou longo exFAT). ATA only (QEMU/HW IDE).
pub fn read_root_file(name: &str) -> Option<Vec<u8>> {
    if let Some(d) = read_fat32(name) {
        return Some(d);
    }
    read_exfat(name)
}

pub fn root_has(name: &str) -> bool {
    read_root_file(name).is_some()
}

fn read_fat32(name: &str) -> Option<Vec<u8>> {
    unsafe {
        let ata = k_nano::ATA_DRIVER.lock();
        let ata = ata.as_ref()?;
        let parts = k_nano::fat32::read_mbr(ata);
        for p in &parts {
            if !matches!(p.type_code, 0x0B | 0x0C | 0x1C | 0x73) {
                continue;
            }
            if let Some(fs) = k_nano::fat32::Fat32Reader::new(ata, p) {
                if let Some(data) = fs.read_file(name) {
                    return Some(data);
                }
            }
        }
    }
    None
}

fn read_exfat(name: &str) -> Option<Vec<u8>> {
    let parts = {
        let ata = k_nano::ATA_DRIVER.lock();
        let ata = ata.as_ref()?;
        k_nano::fat32::read_mbr(ata)
    };
    let mut ata_guard = k_nano::ATA_DRIVER.lock();
    let ata = ata_guard.as_mut()?;
    let want = name.to_ascii_uppercase();
    for part in &parts {
        let start = part.lba_start as u64;
        let mut vbr = [0u8; 512];
        if !ata.read_sectors(start, &mut vbr) {
            continue;
        }
        if &vbr[3..11] != b"EXFAT   " {
            continue;
        }
        if let Some(mut ex) = k_nano::exfat::ExfatReader::new(ata, start) {
            let entries = ex.list_root();
            let mut saw_lego = false;
            for (fname, is_dir, cluster, size) in &entries {
                if *is_dir {
                    continue;
                }
                if fname.len() >= 4 && fname.as_bytes()[..4].eq_ignore_ascii_case(b"LEGO") {
                    saw_lego = true;
                }
                if fname.eq_ignore_ascii_case(want.as_str()) {
                    if let Some(data) = ex.read_file(*cluster, *size as usize) {
                        return Some(data);
                    }
                }
            }
            if want.starts_with("LEGO") {
                k_nano::slog_hal!(
                    "LEGO",
                    "fat",
                    "exfat root entries={} saw_lego_prefix={} want={}",
                    entries.len(),
                    saw_lego,
                    want
                );
            }
        }
    }
    None
}
