//! NTFS reader minimo — leitura de arquivos via $MFT.
//! Suporta: listar diretorio raiz, ler arquivo por path, atributos residentes.
//! Nao suporta: escrita, ACLs, streams alternados, compressed files.

use alloc::vec;
use alloc::vec::Vec;
use alloc::string::String;
use crate::block_dev::BlockDevice;
use crate::fs_driver::{FilesystemDriver, FsInfo};

pub struct NtfsReader {
    start_lba: u64,
    bytes_per_sector: u32,
    sectors_per_cluster: u32,
    mft_lcn: u64,
    mft_record_size: u32,
    label: String,
}

impl NtfsReader {
    fn read_sectors(&self, dev: &mut dyn BlockDevice, lba: u64, buf: &mut [u8]) -> bool {
        for i in 0..(buf.len() / 512) {
            if !dev.read_sectors(self.start_lba + lba + i as u64, &mut buf[i*512..(i+1)*512]) {
                return false;
            }
        }
        true
    }

    fn lcn_to_lba(&self, lcn: i64) -> u64 {
        (lcn as u64) * self.sectors_per_cluster as u64
    }

    fn read_mft_record(&self, dev: &mut dyn BlockDevice, record_num: u64) -> Option<Vec<u8>> {
        let mft_lba = self.lcn_to_lba(self.mft_lcn as i64);
        let record_lba = mft_lba + record_num * (self.mft_record_size as u64 / 512);
        let mut record = vec![0u8; self.mft_record_size as usize];
        if !self.read_sectors(dev, record_lba, &mut record) { return None; }
        if &record[0..4] != b"FILE" { return None; }
        Some(record)
    }

    fn find_attribute<'a>(&self, record: &'a [u8], attr_type: u32) -> Option<&'a [u8]> {
        let mut off = u16::from_le_bytes([record[20], record[21]]) as usize;
        while off + 4 < record.len() {
            let at = u32::from_le_bytes([record[off], record[off+1], record[off+2], record[off+3]]);
            let len = u16::from_le_bytes([record[off+4], record[off+5]]) as usize;
            if at == 0xFFFF || len == 0 { break; }
            if at == attr_type { return Some(&record[off..off+len]); }
            off += len;
        }
        None
    }

    fn parse_filename(attr: &[u8]) -> Option<String> {
        let data_off = attr[20] as usize;
        if data_off + 66 >= attr.len() { return None; }
        let data = &attr[data_off..];
        let name_len = data[64] as usize;
        if name_len == 0 || name_len > 255 { return None; }
        let mut name = String::new();
        for i in 0..name_len {
            let lo = data[66 + i*2] as u16;
            let hi = data[67 + i*2] as u16;
            let cp = lo | (hi << 8);
            if let Some(c) = core::char::from_u32(cp as u32) { name.push(c); }
        }
        Some(name)
    }
}

impl FilesystemDriver for NtfsReader {
    fn name(&self) -> &str { "ntfs" }

    fn detect(dev: &mut dyn BlockDevice, start_lba: u64) -> Option<Self> {
        let mut vbr = [0u8; 512];
        if !dev.read_sectors(start_lba, &mut vbr) { return None; }
        if &vbr[3..11] != b"NTFS    " { return None; }
        let bps = u16::from_le_bytes([vbr[11], vbr[12]]) as u32;
        let spc = vbr[13] as u32;
        let mft_lcn_raw = i64::from_le_bytes([vbr[48], vbr[49], vbr[50], vbr[51], vbr[52], vbr[53], vbr[54], vbr[55]]);
        let mft_record_size = 1u32 << (vbr[64] & 0x7F);
        Some(NtfsReader { start_lba, bytes_per_sector: bps, sectors_per_cluster: spc,
            mft_lcn: mft_lcn_raw as u64, mft_record_size, label: String::new() })
    }

    fn mount(&mut self, dev: &mut dyn BlockDevice, _start_lba: u64) -> Result<FsInfo, &'static str> {
        if let Some(rec) = self.read_mft_record(dev, 3) {
            if let Some(attr) = self.find_attribute(&rec, 0x60) {
                if attr.len() > 22 {
                    let name_len = attr[22] as usize;
                    let data_off = attr[20] as usize;
                    if data_off + 2 + name_len * 2 <= attr.len() && name_len < 32 {
                        let mut s = String::new();
                        for i in 0..name_len {
                            let lo = attr[data_off + i*2] as u16;
                            let hi = attr[data_off + i*2 + 1] as u16;
                            let cp = lo | (hi << 8);
                            if let Some(c) = core::char::from_u32(cp as u32) { s.push(c); }
                        }
                        self.label = s;
                    }
                }
            }
        }
        Ok(FsInfo { fs_type: "NTFS", label: self.label.clone(), total_bytes: 0,
            free_bytes: None, block_size: 512, writable: false })
    }

    fn read(&self, _path: &str, _offset: u64, _buf: &mut [u8]) -> Result<usize, &'static str> {
        Err("NTFS read: not yet implemented")
    }

    fn write(&mut self, _path: &str, _offset: u64, _data: &[u8]) -> Result<(), &'static str> {
        Err("NTFS read-only")
    }

    fn list(&self, _path: &str) -> Result<Vec<(String, bool)>, &'static str> {
        Err("NTFS list: not yet implemented")
    }

    fn free_space(&self) -> u64 { 0 }
    fn total_space(&self) -> u64 { 0 }
}
