//! exFAT create-file write — opt-in (`EXFAT_WRITE=1` no CONFIG.TXT).
//! Atualiza bitmap + FAT + entradas 0x85/0xC0/0xC1 no root (ADR-0040 / IDEA #417).

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use crate::block_dev::BlockDevice;
use crate::exfat::ExfatFs;
use crate::fs_driver::FilesystemDriver;

const BYTES_PER_SECTOR: u64 = 512;
const EOC: u32 = 0xFFFF_FFFF;
/// Limite MVP: evita scan/alocacao enorme em PIO.
const MAX_WRITE_BYTES: usize = 64 * 1024;

/// Escreve arquivo novo no root. Falha se ja existir ou sem bitmap.
pub fn write_file(
    fs: &mut ExfatFs,
    dev: &mut dyn BlockDevice,
    name: &str,
    data: &[u8],
) -> Result<(), &'static str> {
    if !fs.mounted {
        return Err("exFAT not mounted");
    }
    if fs.bitmap_cluster < 2 {
        return Err("exFAT bitmap not found (0x81)");
    }
    if data.len() > MAX_WRITE_BYTES {
        return Err("exFAT write: file too large (MVP 64KiB)");
    }
    let name = sanitize_name(name)?;
    if fs
        .root_cache
        .iter()
        .any(|(n, _, _, _)| n.eq_ignore_ascii_case(&name))
    {
        return Err("exFAT write: file exists");
    }

    let cluster_bytes = fs.bytes_per_cluster as usize;
    let need = if data.is_empty() {
        0u32
    } else {
        ((data.len() + cluster_bytes - 1) / cluster_bytes) as u32
    };

    let clusters = if need == 0 {
        Vec::new()
    } else {
        alloc_clusters(fs, dev, need)?
    };

    if !clusters.is_empty() {
        write_data_clusters(fs, dev, &clusters, data)?;
    }

    let first = clusters.first().copied().unwrap_or(0);
    let entries = make_file_entries(&name, first, data.len() as u64);
    append_root_entries(fs, dev, &entries)?;

    fs.root_cache
        .push((name, false, first, data.len() as u64));
    Ok(())
}

/// Smoke QEMU: cria `EXFATWR.TXT` e le de volta. Requer volume montavel.
pub fn smoke_write_roundtrip(
    dev: &mut dyn BlockDevice,
    start_lba: u64,
) -> Result<(), &'static str> {
    let mut fs = ExfatFs::detect(dev, start_lba).ok_or("exFAT detect failed")?;
    let _ = fs.mount(dev, start_lba)?;
    // Remover smoke anterior (so se for o unico slot livre — reusa nome fixo:
    // se existir, sobrescreve via delete soft: limpa entradas e libera — MVP: skip se existe
    if fs
        .root_cache
        .iter()
        .any(|(n, _, _, _)| n.eq_ignore_ascii_case("EXFATWR.TXT"))
    {
        return Ok(()); // ja validado em boot anterior
    }
    let payload = b"neural-os exfat write #417\n";
    write_file(&mut fs, dev, "EXFATWR.TXT", payload)?;
    // Remount + read via reader path
    let mut fs2 = ExfatFs::detect(dev, start_lba).ok_or("exFAT redetect failed")?;
    let _ = fs2.mount(dev, start_lba)?;
    let entry = fs2
        .root_cache
        .iter()
        .find(|(n, _, _, _)| n.eq_ignore_ascii_case("EXFATWR.TXT"))
        .ok_or("EXFATWR.TXT missing after write")?;
    let (_n, _d, cluster, size) = entry.clone();
    let mut reader = crate::exfat::ExfatReader::new(dev, start_lba).ok_or("reader")?;
    let got = reader
        .read_file(cluster, size as usize)
        .ok_or("readback failed")?;
    if got.as_slice() != payload {
        return Err("exFAT write smoke mismatch");
    }
    Ok(())
}

fn sanitize_name(name: &str) -> Result<String, &'static str> {
    let name = name.trim_matches('/').trim();
    if name.is_empty() || name.len() > 255 {
        return Err("exFAT bad name");
    }
    if name.contains('/') || name.contains('\\') {
        return Err("exFAT bad name");
    }
    Ok(String::from(name))
}

fn write_fat_entry(
    fs: &ExfatFs,
    dev: &mut dyn BlockDevice,
    cluster: u32,
    value: u32,
) -> Result<(), &'static str> {
    let fat_offset = cluster as u64 * 4;
    let sector = fs.fat_lba + fat_offset / BYTES_PER_SECTOR;
    let offset = (fat_offset % BYTES_PER_SECTOR) as usize;
    let mut buf = [0u8; 512];
    if !dev.read_sectors(sector, &mut buf) {
        return Err("exFAT FAT read failed");
    }
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    if !dev.write_sectors(sector, &buf) {
        return Err("exFAT FAT write failed");
    }
    Ok(())
}

fn bitmap_set(
    fs: &ExfatFs,
    dev: &mut dyn BlockDevice,
    cluster: u32,
) -> Result<(), &'static str> {
    if cluster < 2 {
        return Err("exFAT bitmap bad cluster");
    }
    let bit_index = (cluster - 2) as u64;
    let byte_i = (bit_index / 8) as usize;
    let bit = (bit_index % 8) as u8;
    let cluster_bytes = fs.bytes_per_cluster as usize;
    let cluster_off = byte_i / cluster_bytes;
    let within = byte_i % cluster_bytes;
    let bitmap_cl = fs.bitmap_cluster + cluster_off as u32;
    let lba = fs.cluster_heap_lba
        + (bitmap_cl - 2) as u64 * fs.bytes_per_cluster / BYTES_PER_SECTOR;
    let sector_within = within / 512;
    let byte_within = within % 512;
    let mut sector = [0u8; 512];
    if !dev.read_sectors(lba + sector_within as u64, &mut sector) {
        return Err("exFAT bitmap read failed");
    }
    sector[byte_within] |= 1 << bit;
    if !dev.write_sectors(lba + sector_within as u64, &sector) {
        return Err("exFAT bitmap write failed");
    }
    Ok(())
}

fn alloc_clusters(
    fs: &ExfatFs,
    dev: &mut dyn BlockDevice,
    count: u32,
) -> Result<Vec<u32>, &'static str> {
    let mut free = Vec::with_capacity(count as usize);
    // Scan FAT: 0 = livre (mesmo criterio mkexfat)
    let max_cl = fs.clusters + 1;
    let start = fs.root_cluster.saturating_add(1).max(2);
    for cl in start..=max_cl {
        if free.len() >= count as usize {
            break;
        }
        let Some(val) = crate::exfat::read_fat_entry_pub(dev, fs.fat_lba, cl) else {
            continue;
        };
        if val == 0 {
            free.push(cl);
        }
    }
    if free.len() < count as usize {
        return Err("exFAT no free clusters");
    }
    for i in 0..free.len() {
        let nxt = if i + 1 < free.len() {
            free[i + 1]
        } else {
            EOC
        };
        write_fat_entry(fs, dev, free[i], nxt)?;
        bitmap_set(fs, dev, free[i])?;
    }
    Ok(free)
}

fn write_data_clusters(
    fs: &ExfatFs,
    dev: &mut dyn BlockDevice,
    clusters: &[u32],
    data: &[u8],
) -> Result<(), &'static str> {
    let cluster_bytes = fs.bytes_per_cluster as usize;
    let mut offset = 0usize;
    for &cl in clusters {
        let lba = fs.cluster_heap_lba
            + (cl - 2) as u64 * fs.bytes_per_cluster / BYTES_PER_SECTOR;
        let mut buf = vec![0u8; cluster_bytes];
        let n = (data.len() - offset).min(cluster_bytes);
        buf[..n].copy_from_slice(&data[offset..offset + n]);
        offset += n;
        for i in 0..cluster_bytes / 512 {
            if !dev.write_sectors(lba + i as u64, &buf[i * 512..(i + 1) * 512]) {
                return Err("exFAT data write failed");
            }
        }
    }
    Ok(())
}

fn make_file_entries(name: &str, first_cluster: u32, size: u64) -> Vec<u8> {
    let name_chars: Vec<char> = name.chars().collect();
    let name_entries = (name_chars.len() + 14) / 15;
    let secondary = 1 + name_entries;
    let mut out = Vec::with_capacity((2 + name_entries) * 32);

    let mut file = [0u8; 32];
    file[0] = 0x85;
    file[1] = secondary as u8;
    file[4] = 0x20; // archive
    file[5] = 0x00;
    out.extend_from_slice(&file);

    let mut stream = [0u8; 32];
    stream[0] = 0xC0;
    stream[1] = 0x01; // AllocationPossible, use FAT
    stream[3] = name_chars.len() as u8;
    stream[8..16].copy_from_slice(&size.to_le_bytes());
    stream[20..24].copy_from_slice(&first_cluster.to_le_bytes());
    stream[24..32].copy_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&stream);

    for i in 0..name_entries {
        let mut ne = [0u8; 32];
        ne[0] = 0xC1;
        let chunk = &name_chars[i * 15..(i * 15 + 15).min(name_chars.len())];
        let mut raw = Vec::new();
        for &c in chunk {
            let u = c as u16;
            raw.extend_from_slice(&u.to_le_bytes());
        }
        ne[2..2 + raw.len()].copy_from_slice(&raw);
        out.extend_from_slice(&ne);
    }

    // SetChecksum no File entry
    let csum = entry_set_checksum(&out);
    out[2] = (csum & 0xFF) as u8;
    out[3] = (csum >> 8) as u8;
    out
}

fn entry_set_checksum(entries: &[u8]) -> u16 {
    let mut sum: u16 = 0;
    for (i, &b) in entries.iter().enumerate() {
        if i == 2 || i == 3 {
            continue;
        }
        sum = if sum & 1 != 0 {
            0x8000 | (sum >> 1)
        } else {
            sum >> 1
        };
        sum = sum.wrapping_add(b as u16);
    }
    sum
}

fn append_root_entries(
    fs: &ExfatFs,
    dev: &mut dyn BlockDevice,
    entries: &[u8],
) -> Result<(), &'static str> {
    let cluster_bytes = fs.bytes_per_cluster as usize;
    let need = entries.len();
    let mut cluster = fs.root_cluster;
    let mut visited = alloc::collections::BTreeSet::new();
    while cluster >= 2 && cluster < 0xFFFF_FFF0 {
        if !visited.insert(cluster) {
            break;
        }
        let lba = fs.cluster_heap_lba
            + (cluster - 2) as u64 * fs.bytes_per_cluster / BYTES_PER_SECTOR;
        let mut buf = vec![0u8; cluster_bytes];
        for i in 0..cluster_bytes / 512 {
            if !dev.read_sectors(lba + i as u64, &mut buf[i * 512..(i + 1) * 512]) {
                return Err("exFAT root read failed");
            }
        }
        for off in (0..=cluster_bytes.saturating_sub(need)).step_by(32) {
            if buf[off] == 0x00 {
                buf[off..off + need].copy_from_slice(entries);
                for i in 0..cluster_bytes / 512 {
                    if !dev.write_sectors(lba + i as u64, &buf[i * 512..(i + 1) * 512]) {
                        return Err("exFAT root write failed");
                    }
                }
                return Ok(());
            }
        }
        cluster = crate::exfat::read_fat_entry_pub(dev, fs.fat_lba, cluster)
            .unwrap_or(EOC);
    }
    Err("exFAT root full (no extend in MVP)")
}
