//! exFAT Filesystem — leitura + list root para pendrives/SDHC >4GB.
//! Formato: Microsoft exFAT 1.0. Escrita de arquivo deferida (bitmap/FAT — risco de corromper).
//! FilesystemDriver: detect/mount/list(cache)/read(cache path).

use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use crate::block_dev::BlockDevice;
use crate::fs_driver::{FilesystemDriver, FsInfo};

const EBPB_SIGNATURE: [u8; 8] = *b"EXFAT   ";
const BYTES_PER_SECTOR: u64 = 512;

/// Legacy helper that borrows a BlockDevice for one-shot reads.
pub struct ExfatReader<'a> {
    dev: &'a mut dyn BlockDevice,
    pub total_sectors: u64,
    pub start_lba: u64,
    pub bytes_per_cluster: u64,
    pub clusters: u32,
    fat_lba: u64,
    cluster_heap_lba: u64,
    root_cluster: u32,
    pub volume_label: String,
}

impl<'a> ExfatReader<'a> {
    pub fn new(dev: &'a mut dyn BlockDevice, start_lba: u64) -> Option<Self> {
        let mut fs = ExfatFs::detect(dev, start_lba)?;
        let _ = fs.mount(dev, start_lba);
        Some(ExfatReader {
            dev,
            total_sectors: fs.total_sectors,
            start_lba: fs.start_lba,
            bytes_per_cluster: fs.bytes_per_cluster,
            clusters: fs.clusters,
            fat_lba: fs.fat_lba,
            cluster_heap_lba: fs.cluster_heap_lba,
            root_cluster: fs.root_cluster,
            volume_label: fs.volume_label,
        })
    }

    fn read_fat_entry(&mut self, cluster: u32) -> Option<u32> {
        let fat_offset = cluster as u64 * 4;
        let sector = self.fat_lba + fat_offset / BYTES_PER_SECTOR;
        let offset = (fat_offset % BYTES_PER_SECTOR) as usize;
        let mut buf = [0u8; 512];
        if !self.dev.read_sectors(sector, &mut buf) {
            return None;
        }
        Some(u32::from_le_bytes([
            buf[offset],
            buf[offset + 1],
            buf[offset + 2],
            buf[offset + 3],
        ]))
    }

    fn cluster_to_lba(&self, cluster: u32) -> u64 {
        self.cluster_heap_lba
            + (cluster - 2) as u64 * self.bytes_per_cluster / BYTES_PER_SECTOR
    }

    pub fn read_file(&mut self, first_cluster: u32, size: usize) -> Option<Vec<u8>> {
        let mut data = Vec::with_capacity(size);
        let mut cluster = first_cluster;
        let cluster_bytes = self.bytes_per_cluster as usize;
        let mut visited = BTreeSet::new();
        while cluster >= 2 && cluster < 0xFFFF_FFF0 && data.len() < size {
            if !visited.insert(cluster) {
                return None;
            }
            let lba = self.cluster_to_lba(cluster);
            let mut buf = vec![0u8; cluster_bytes];
            for i in 0..cluster_bytes / 512 {
                if data.len() + i * 512 >= size {
                    break;
                }
                if !self
                    .dev
                    .read_sectors(lba + i as u64, &mut buf[i * 512..(i + 1) * 512])
                {
                    return None;
                }
            }
            let remaining = size - data.len();
            let copy_end = remaining.min(cluster_bytes);
            data.extend_from_slice(&buf[..copy_end]);
            cluster = self.read_fat_entry(cluster)?;
        }
        Some(data)
    }

    pub fn list_root(&mut self) -> Vec<(String, bool, u32, u64)> {
        list_directory(
            self.dev,
            self.fat_lba,
            self.cluster_heap_lba,
            self.bytes_per_cluster,
            self.root_cluster,
        )
    }
}

/// Owned exFAT driver (no BlockDevice lifetime) — mount caches root listing.
pub struct ExfatFs {
    pub start_lba: u64,
    pub total_sectors: u64,
    pub bytes_per_cluster: u64,
    pub clusters: u32,
    fat_lba: u64,
    cluster_heap_lba: u64,
    root_cluster: u32,
    pub volume_label: String,
    /// Cached root: (name, is_dir, first_cluster, size)
    root_cache: Vec<(String, bool, u32, u64)>,
    mounted: bool,
}

fn parse_vbr(vbr: &[u8; 512], start_lba: u64) -> Option<ExfatFs> {
    if &vbr[3..11] != &EBPB_SIGNATURE[..] || vbr[11] != 0x00 {
        return None;
    }
    // Microsoft exFAT: VolumeLength @72 (nao @56 — bytes 11..63 MustBeZero)
    let total_sectors = u64::from_le_bytes([
        vbr[72], vbr[73], vbr[74], vbr[75], vbr[76], vbr[77], vbr[78], vbr[79],
    ]);
    let fat_offset = u32::from_le_bytes([vbr[80], vbr[81], vbr[82], vbr[83]]);
    let cluster_heap_offset = u32::from_le_bytes([vbr[88], vbr[89], vbr[90], vbr[91]]);
    let cluster_count = u32::from_le_bytes([vbr[92], vbr[93], vbr[94], vbr[95]]);
    let root_cluster = u32::from_le_bytes([vbr[96], vbr[97], vbr[98], vbr[99]]);
    let bytes_per_cluster_shift = vbr[109];
    let bytes_per_cluster = 512u64 << (bytes_per_cluster_shift as u64);

    let mut label = String::new();
    for i in (0..22).step_by(2) {
        let lo = vbr[114 + i] as u16;
        let hi = vbr[114 + i + 1] as u16;
        let cp = lo | (hi << 8);
        if cp == 0 {
            break;
        }
        if let Some(c) = core::char::from_u32(cp as u32) {
            label.push(c);
        }
    }

    Some(ExfatFs {
        start_lba,
        total_sectors,
        bytes_per_cluster,
        clusters: cluster_count,
        fat_lba: start_lba + fat_offset as u64,
        cluster_heap_lba: start_lba + cluster_heap_offset as u64,
        root_cluster,
        volume_label: label,
        root_cache: Vec::new(),
        mounted: false,
    })
}

fn read_fat_entry(
    dev: &mut dyn BlockDevice,
    fat_lba: u64,
    cluster: u32,
) -> Option<u32> {
    let fat_offset = cluster as u64 * 4;
    let sector = fat_lba + fat_offset / BYTES_PER_SECTOR;
    let offset = (fat_offset % BYTES_PER_SECTOR) as usize;
    let mut buf = [0u8; 512];
    if !dev.read_sectors(sector, &mut buf) {
        return None;
    }
    Some(u32::from_le_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
    ]))
}

fn cluster_to_lba(cluster_heap_lba: u64, bytes_per_cluster: u64, cluster: u32) -> u64 {
    cluster_heap_lba + (cluster - 2) as u64 * bytes_per_cluster / BYTES_PER_SECTOR
}

fn list_directory(
    dev: &mut dyn BlockDevice,
    fat_lba: u64,
    cluster_heap_lba: u64,
    bytes_per_cluster: u64,
    first_cluster: u32,
) -> Vec<(String, bool, u32, u64)> {
    let mut out = Vec::new();
    let mut cluster = first_cluster;
    let cluster_bytes = bytes_per_cluster as usize;
    let mut visited = BTreeSet::new();
    let mut pending_attrs: u16 = 0;
    let mut pending_name = String::new();
    let mut pending_cluster: u32 = 0;
    let mut pending_size: u64 = 0;
    let mut secondary_left: i32 = 0;

    while cluster >= 2 && cluster < 0xFFFF_FFF0 {
        if !visited.insert(cluster) {
            break;
        }
        let lba = cluster_to_lba(cluster_heap_lba, bytes_per_cluster, cluster);
        let mut buf = vec![0u8; cluster_bytes];
        for i in 0..cluster_bytes / 512 {
            if !dev.read_sectors(lba + i as u64, &mut buf[i * 512..(i + 1) * 512]) {
                return out;
            }
        }
        let mut off = 0;
        while off + 32 <= buf.len() {
            let entry = &buf[off..off + 32];
            let etype = entry[0];
            if etype == 0x00 {
                return out;
            }
            if etype & 0x80 == 0 {
                off += 32;
                continue;
            }
            match etype {
                0x85 => {
                    secondary_left = entry[1] as i32;
                    pending_attrs = u16::from_le_bytes([entry[4], entry[5]]);
                    pending_name.clear();
                    pending_cluster = 0;
                    pending_size = 0;
                }
                0xC0 if secondary_left > 0 => {
                    pending_size = u64::from_le_bytes([
                        entry[8], entry[9], entry[10], entry[11], entry[12], entry[13],
                        entry[14], entry[15],
                    ]);
                    pending_cluster =
                        u32::from_le_bytes([entry[20], entry[21], entry[22], entry[23]]);
                    secondary_left -= 1;
                }
                0xC1 if secondary_left > 0 => {
                    for i in (2..32).step_by(2) {
                        let cp = entry[i] as u16 | ((entry[i + 1] as u16) << 8);
                        if cp == 0 {
                            break;
                        }
                        if let Some(c) = core::char::from_u32(cp as u32) {
                            pending_name.push(c);
                        }
                    }
                    secondary_left -= 1;
                    if secondary_left <= 0 && !pending_name.is_empty() {
                        let is_dir = (pending_attrs & 0x10) != 0;
                        out.push((
                            pending_name.clone(),
                            is_dir,
                            pending_cluster,
                            pending_size,
                        ));
                        pending_name.clear();
                    }
                }
                _ => {}
            }
            off += 32;
        }
        match read_fat_entry(dev, fat_lba, cluster) {
            Some(next) => cluster = next,
            None => break,
        }
    }
    out
}

impl FilesystemDriver for ExfatFs {
    fn name(&self) -> &str {
        "exfat"
    }

    fn detect(dev: &mut dyn BlockDevice, start_lba: u64) -> Option<Self> {
        let mut vbr = [0u8; 512];
        if !dev.read_sectors(start_lba, &mut vbr) {
            return None;
        }
        parse_vbr(&vbr, start_lba)
    }

    fn mount(&mut self, dev: &mut dyn BlockDevice, start_lba: u64) -> Result<FsInfo, &'static str> {
        if start_lba != self.start_lba {
            let mut vbr = [0u8; 512];
            if !dev.read_sectors(start_lba, &mut vbr) {
                return Err("exFAT read VBR failed");
            }
            let fresh = parse_vbr(&vbr, start_lba).ok_or("not exFAT")?;
            *self = fresh;
        }
        self.root_cache = list_directory(
            dev,
            self.fat_lba,
            self.cluster_heap_lba,
            self.bytes_per_cluster,
            self.root_cluster,
        );
        self.mounted = true;
        Ok(FsInfo {
            fs_type: "exFAT",
            label: self.volume_label.clone(),
            total_bytes: self.total_sectors * 512,
            free_bytes: None,
            block_size: self.bytes_per_cluster as u32,
            writable: false,
        })
    }

    fn read(&self, path: &str, offset: u64, buf: &mut [u8]) -> Result<usize, &'static str> {
        if !self.mounted {
            return Err("exFAT not mounted");
        }
        let name = path.trim_matches('/');
        if name.is_empty() {
            let listing = self
                .root_cache
                .iter()
                .map(|(n, d, _, _)| {
                    if *d {
                        alloc::format!("{}/", n)
                    } else {
                        n.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            let bytes = listing.as_bytes();
            if offset as usize >= bytes.len() {
                return Ok(0);
            }
            let start = offset as usize;
            let n = (bytes.len() - start).min(buf.len());
            buf[..n].copy_from_slice(&bytes[start..start + n]);
            return Ok(n);
        }
        Err("exFAT path read needs BlockDevice — use ExfatReader::read_file")
    }

    fn write(&mut self, _path: &str, _offset: u64, _data: &[u8]) -> Result<(), &'static str> {
        Err("exFAT write deferred (bitmap/FAT — ADR-0040)")
    }

    fn list(&self, path: &str) -> Result<Vec<(String, bool)>, &'static str> {
        if !self.mounted {
            return Err("exFAT not mounted");
        }
        let p = path.trim_matches('/');
        if !p.is_empty() && p != "." {
            return Err("exFAT list: only root cached in MVP");
        }
        Ok(self
            .root_cache
            .iter()
            .map(|(n, d, _, _)| (n.clone(), *d))
            .collect())
    }

    fn free_space(&self) -> u64 {
        0
    }

    fn total_space(&self) -> u64 {
        self.total_sectors * 512
    }
}
