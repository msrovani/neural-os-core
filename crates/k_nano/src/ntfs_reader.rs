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

    /// Lê o $DATA attribute residente (type 0x80, resident flag = 0).
    /// Retorna os bytes do arquivo quando o dado está embutido no MFT record.
    fn read_resident_data(&self, record: &[u8]) -> Option<Vec<u8>> {
        let attr = self.find_attribute(record, 0x80)?; // $DATA
        let flags = u16::from_le_bytes([attr[12], attr[13]]);
        if flags & 0x0001 != 0 {
            // Non-resident: não suportado neste reader mínimo
            return None;
        }
        // Resident: data_off[20] aponta p/ o dado dentro do attribute
        let data_len = u32::from_le_bytes([attr[16], attr[17], attr[18], attr[19]]) as usize;
        let data_off = attr[20] as usize;
        if data_off + data_len > attr.len() { return None; }
        Some(attr[data_off..data_off + data_len].to_vec())
    }

    /// Encontra o MFT record number de um arquivo no diretório raiz (record 5).
    /// Busca linear no $INDEX_ROOT (0x90) do root — funciona p/ diretórios pequenos.
    fn find_file_in_root(&self, dev: &mut dyn BlockDevice, name: &str) -> Option<u64> {
        let root = self.read_mft_record(dev, 5)?;
        let idx_root = self.find_attribute(&root, 0x90)?; // $INDEX_ROOT
        // $INDEX_ROOT header: flags@16, total_size@20, alloc_size@24
        if idx_root.len() < 32 { return None; }
        // Header do $INDEX_ROOT: type(4)+len(4)+non_res(1)+name_len(1)+name_off(2)+flags(4)+total_size(4)+alloc_size(4) = ~24
        let mut off = 24; // simplificado: pula header
        while off + 80 < idx_root.len() {
            let magic = &idx_root[off..off+4];
            if magic != b"INDX" && magic[0] != 0 {
                // Index entry: ref@0(8)+len@8(2)+content_len@10(2)+flags@12(4)
                let ref_lo = u32::from_le_bytes([idx_root[off], idx_root[off+1], idx_root[off+2], idx_root[off+3]]);
                let ref_hi = u16::from_le_bytes([idx_root[off+4], idx_root[off+5]]) as u64;
                let mft_ref = (ref_hi << 32) | ref_lo as u64;
                let entry_len = u16::from_le_bytes([idx_root[off+8], idx_root[off+9]]) as usize;
                if entry_len == 0 || off + entry_len > idx_root.len() { break; }
                // $FILE_NAME attribute dentro do index entry: offset 16
                if off + 16 + 80 <= idx_root.len() {
                    let fn_off = off + 16;
                    let fn_attr = &idx_root[fn_off..fn_off + 80];
                    if let Some(fname) = Self::parse_filename(fn_attr) {
                        if fname.eq_ignore_ascii_case(name) {
                            return Some(mft_ref);
                        }
                    }
                }
                off += entry_len;
            } else {
                break;
            }
        }
        None
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

    fn read(&self, path: &str, offset: u64, buf: &mut [u8]) -> Result<usize, &'static str> {
        // path formato: "/ARQUIVO.TXT" ou "ARQUIVO.TXT" — busca case-insensitive no root
        let name = path.trim_start_matches('/').trim();
        if name.is_empty() || name.contains('/') {
            return Err("NTFS read: apenas arquivos no diretorio raiz (sem subdirs)");
        }
        let mut dev = crate::ATA_DRIVER.lock();
        let Some(ata) = dev.as_mut() else { return Err("NTFS: sem ATA driver"); };
        let ata_ptr = &mut *ata as *mut dyn BlockDevice;
        // SAFETY: ata é único (lock held), e não usamos ata diretamente durante o read
        let ata_ref = unsafe { &mut *ata_ptr };

        // Encontra o MFT record do arquivo no root
        let mft_ref = self.find_file_in_root(ata_ref, name)
            .ok_or("NTFS read: arquivo nao encontrado")?;
        let record = self.read_mft_record(ata_ref, mft_ref)
            .ok_or("NTFS read: falha ao ler MFT record")?;

        // Tenta ler $DATA residente
        let data = self.read_resident_data(&record)
            .ok_or("NTFS read: apenas dados residentes (arquivos pequenos)")?;

        let off = offset as usize;
        if off >= data.len() { return Ok(0); }
        let n = core::cmp::min(buf.len(), data.len() - off);
        buf[..n].copy_from_slice(&data[off..off + n]);
        Ok(n)
    }

    fn write(&mut self, _path: &str, _offset: u64, _data: &[u8]) -> Result<(), &'static str> {
        Err("NTFS read-only")
    }

    fn list(&self, path: &str) -> Result<Vec<(String, bool)>, &'static str> {
        // Apenas diretório raiz suportado
        if path != "/" && path != "" {
            return Err("NTFS list: apenas diretorio raiz");
        }
        let mut dev = crate::ATA_DRIVER.lock();
        let Some(ata) = dev.as_mut() else { return Err("NTFS: sem ATA driver"); };
        let ata_ptr = &mut *ata as *mut dyn BlockDevice;
        let ata_ref = unsafe { &mut *ata_ptr };

        let root = self.read_mft_record(ata_ref, 5).ok_or("NTFS list: falha root")?;
        let idx_root = self.find_attribute(&root, 0x90).ok_or("NTFS list: sem $INDEX_ROOT")?;

        let mut entries = Vec::new();
        let mut off = 24; // após header do $INDEX_ROOT
        while off + 80 < idx_root.len() {
            let magic = &idx_root[off..off+4];
            if magic != b"INDX" && magic[0] == 0 { break; }
            let entry_len = u16::from_le_bytes([idx_root[off+8], idx_root[off+9]]) as usize;
            if entry_len == 0 || off + entry_len > idx_root.len() { break; }
            // $FILE_NAME no offset 16 do index entry
            if off + 16 + 80 <= idx_root.len() {
                let fn_attr = &idx_root[off + 16..off + 16 + 80];
                if let Some(fname) = Self::parse_filename(fn_attr) {
                    // flags no index entry @12: bit 0 = directory
                    let flags = u32::from_le_bytes([idx_root[off+12], idx_root[off+13], idx_root[off+14], idx_root[off+15]]);
                    let is_dir = (flags & 1) != 0 || fname == "." || fname == "..";
                    if fname != "." && fname != ".." {
                        entries.push((fname, is_dir));
                    }
                }
            }
            off += entry_len;
        }
        Ok(entries)
    }

    fn free_space(&self) -> u64 { 0 }
    fn total_space(&self) -> u64 { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Testa parse_filename com um $FILE_NAME attribute sintético.
    #[test]
    fn ntfs_parse_filename_synthetic() {
        // $FILE_NAME attribute: header 24B, content 66B (name_len @ 64, name @ 66)
        let mut attr = vec![0u8; 128];
        attr[0..4].copy_from_slice(&0x30u32.to_le_bytes()); // type = $FILE_NAME
        attr[4..6].copy_from_slice(&0x80u16.to_le_bytes()); // len = 128
        attr[20] = 24; // data_off: conteúdo começa no offset 24
        // $FILE_NAME content @ offset 24: name_len @ 64 (relativo ao attr = 88)
        attr[88] = 8; // name_len = 8 ("TEST.TXT")
        let name = b"TEST.TXT";
        for (i, &c) in name.iter().enumerate() {
            attr[90 + i * 2] = c as u8; // name @ offset 66 (relativo = 90)
            attr[91 + i * 2] = 0;
        }
        let parsed = NtfsReader::parse_filename(&attr);
        assert_eq!(parsed.as_deref(), Some("TEST.TXT"));
    }

    /// Testa detect() com VBR NTFS sintético.
    #[test]
    fn ntfs_detect_synthetic() {
        // VBR NTFS mínimo: bytes_per_sector=512, sectors_per_cluster=8, mft_lcn=4, record_size=1024
        struct FakeDev { data: Vec<u8> }
        impl BlockDevice for FakeDev {
            fn read_sectors(&mut self, lba: u64, buf: &mut [u8]) -> bool {
                let start = (lba as usize) * 512;
                if start + buf.len() > self.data.len() { return false; }
                buf.copy_from_slice(&self.data[start..start + buf.len()]);
                true
            }
            fn write_sectors(&mut self, _lba: u64, _buf: &[u8]) -> bool { false }
            fn total_sectors(&self) -> u64 { self.data.len() as u64 / 512 }
            fn name(&self) -> &str { "fake" }
        }
        let mut vbr = vec![0u8; 512];
        vbr[3..11].copy_from_slice(b"NTFS    ");
        vbr[11..13].copy_from_slice(&512u16.to_le_bytes()); // bps
        vbr[13] = 8; // spc
        vbr[48..56].copy_from_slice(&4i64.to_le_bytes()); // mft_lcn
        vbr[64] = 0x8A; // mft_record_size = 1024 (high bit set, 2^10 = 1024)
        vbr[0x1FE] = 0x55; vbr[0x1FF] = 0xAA;
        let mut dev = FakeDev { data: vbr };
        let reader = NtfsReader::detect(&mut dev, 0);
        assert!(reader.is_some());
        let r = reader.unwrap();
        assert_eq!(r.mft_lcn, 4);
        assert_eq!(r.mft_record_size, 1024);
    }
}
