//! exFAT Filesystem — leitura/escrita para pendrives/SDHC >4GB.
//! Formato: Microsoft exFAT, setor 512 bytes, cluster bitmap, FAT chain.
//! Baseado na especificação exFAT 1.0 (Microsoft 2019).

use alloc::vec;
use alloc::vec::Vec;
use alloc::string::String;
use crate::block_dev::BlockDevice;

const EBPB_SIGNATURE: [u8; 11] = *b"EXFAT      ";
const BYTES_PER_SECTOR: u64 = 512;
const BYTES_PER_CLUSTER: u64 = 4096; // default

pub struct ExfatReader<'a> {
    dev: &'a mut dyn BlockDevice,
    pub total_sectors: u64,
    pub start_lba: u64,
    pub bytes_per_cluster: u64,
    pub clusters: u32,
    fat_lba: u64,
    cluster_heap_lba: u64,
    pub volume_label: String,
}

impl<'a> ExfatReader<'a> {
    pub fn new(dev: &'a mut dyn BlockDevice, start_lba: u64) -> Option<Self> {
        let mut vbr = [0u8; 512];
        if !dev.read_sectors(start_lba, &mut vbr) { return None; }
        if &vbr[3..14] != &EBPB_SIGNATURE[..] { return None; }

        let total_sectors = u64::from_le_bytes([
            vbr[56], vbr[57], vbr[58], vbr[59],
            vbr[60], vbr[61], vbr[62], vbr[63],
        ]);
        let fat_offset = u32::from_le_bytes([vbr[80], vbr[81], vbr[82], vbr[83]]);
        let _fat_length = u32::from_le_bytes([vbr[84], vbr[85], vbr[86], vbr[87]]);
        let cluster_heap_offset = u32::from_le_bytes([vbr[88], vbr[89], vbr[90], vbr[91]]);
        let cluster_count = u32::from_le_bytes([vbr[92], vbr[93], vbr[94], vbr[95]]);
        let bytes_per_cluster_shift = vbr[109];
        let bytes_per_cluster = 512u64 << (bytes_per_cluster_shift as u64);

        let mut label = String::new();
        for i in 0..11 {
            let c = vbr[114 + i] as char;
            if c != '\0' { label.push(c); }
        }

        let fat_lba = start_lba + fat_offset as u64;
        let cluster_heap_lba = start_lba + cluster_heap_offset as u64;

        Some(ExfatReader {
            dev,
            total_sectors,
            start_lba,
            bytes_per_cluster,
            clusters: cluster_count,
            fat_lba,
            cluster_heap_lba,
            volume_label: label,
        })
    }

    fn read_fat_entry(&mut self, cluster: u32) -> Option<u32> {
        let fat_offset = cluster as u64 * 4;
        let sector = self.fat_lba + fat_offset / BYTES_PER_SECTOR;
        let offset = (fat_offset % BYTES_PER_SECTOR) as usize;
        let mut buf = [0u8; 512];
        if !self.dev.read_sectors(sector, &mut buf) { return None; }
        Some(u32::from_le_bytes([buf[offset], buf[offset+1], buf[offset+2], buf[offset+3]]))
    }

    fn cluster_to_lba(&self, cluster: u32) -> u64 {
        self.cluster_heap_lba + (cluster - 2) as u64 * self.bytes_per_cluster / BYTES_PER_SECTOR
    }

    pub fn read_file(&mut self, first_cluster: u32, size: usize) -> Option<Vec<u8>> {
        let mut data = Vec::with_capacity(size);
        let mut cluster = first_cluster;
        let cluster_bytes = self.bytes_per_cluster as usize;
        while cluster >= 2 && cluster < 0xFFFF_FFF0 && data.len() < size {
            let lba = self.cluster_to_lba(cluster);
            let mut buf = vec![0u8; cluster_bytes];
            for i in 0..cluster_bytes / 512 {
                if data.len() + i * 512 >= size { break; }
                if !self.dev.read_sectors(lba + i as u64, &mut buf[i * 512..(i+1) * 512]) { break; }
            }
            let remaining = size - data.len();
            let copy_end = remaining.min(cluster_bytes);
            data.extend_from_slice(&buf[..copy_end]);
            cluster = self.read_fat_entry(cluster)?;
        }
        Some(data)
    }
}
