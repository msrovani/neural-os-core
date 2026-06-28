use alloc::vec::Vec;
use crate::ata::AtaDriver;
use crate::serial_println;

#[derive(Debug)]
pub struct Partition {
    pub bootable: bool,
    pub type_code: u8,
    pub lba_start: u32,
    pub sector_count: u32,
}

/// Lê MBR do primeiro setor (LBA 0) via ATA
pub fn read_mbr(ata: &AtaDriver) -> Vec<Partition> {
    let mut mbr = [0u8; 512];
    if !unsafe { ata.read_sectors(0, &mut mbr, 1) } {
        serial_println!("[MBR] Falha ao ler setor 0");
        return Vec::new();
    }
    if mbr[0x1FE] != 0x55 || mbr[0x1FF] != 0xAA {
        serial_println!("[MBR] Signature 55AA nao encontrada");
        return Vec::new();
    }
    let mut parts = Vec::new();
    for i in 0..4 {
        let off = 0x1BE + i * 16;
        let type_code = mbr[off + 4];
        if type_code == 0 { continue; }
        let lba = u32::from_le_bytes([mbr[off+8], mbr[off+9], mbr[off+10], mbr[off+11]]);
        let count = u32::from_le_bytes([mbr[off+12], mbr[off+13], mbr[off+14], mbr[off+15]]);
        parts.push(Partition { bootable: mbr[off] == 0x80, type_code, lba_start: lba, sector_count: count });
        serial_println!("[MBR] {}: type={:#04x} LBA={} size={}", i+1, type_code, lba, count);
    }
    parts
}

pub struct Fat12Writer<'a> {
    ata: &'a AtaDriver,
    pub lba_start: u32,
    bpb: FatBpb,
}

struct FatBpb {
    reserved: u16, fat_count: u8, root_entries: u16,
    sectors_per_fat: u16, sectors_per_cluster: u8,
}

impl<'a> Fat12Writer<'a> {
    pub unsafe fn new(ata: &'a AtaDriver, part: &Partition) -> Option<Self> {
        if part.type_code != 0x01 { return None; }
        let mut bpb_buf = [0u8; 512];
        if !ata.read_sectors(part.lba_start, &mut bpb_buf, 1) { return None; }
        let bpb = FatBpb {
            reserved: u16::from_le_bytes([bpb_buf[0x0E], bpb_buf[0x0F]]),
            fat_count: bpb_buf[0x10],
            root_entries: u16::from_le_bytes([bpb_buf[0x11], bpb_buf[0x12]]),
            sectors_per_fat: u16::from_le_bytes([bpb_buf[0x16], bpb_buf[0x17]]),
            sectors_per_cluster: bpb_buf[0x0D],
        };
        serial_println!("[FAT12] BPB: spc={} reserved={} fats={} root={} spf={}",
            bpb.sectors_per_cluster, bpb.reserved, bpb.fat_count, bpb.root_entries, bpb.sectors_per_fat);
        Some(Fat12Writer { ata, lba_start: part.lba_start, bpb })
    }

    fn root_lba(&self) -> u32 {
        self.lba_start + self.bpb.reserved as u32
            + self.bpb.fat_count as u32 * self.bpb.sectors_per_fat as u32
    }

    fn data_lba(&self) -> u32 {
        let root_bytes = self.bpb.root_entries as u32 * 32;
        self.root_lba() + (root_bytes + 511) / 512
    }

    pub unsafe fn append_log(&self, data: &[u8]) -> bool {
        if data.len() > 500 { return false; }
        let root_lba = self.root_lba();
        let mut root = [0u8; 512];
        if !self.ata.read_sectors(root_lba, &mut root, 1) { return false; }

        let mut entry = 0usize;
        let mut found = false;
        for i in 0..self.bpb.root_entries as usize {
            if root[i * 32] == 0 { break; }
            if root[i*32..i*32+11] == *b"BOOT    LOG" { entry = i; found = true; break; }
        }
        if !found { return false; }

        let old_size = u32::from_le_bytes(root[entry*32+28..entry*32+32].try_into().unwrap());
        let cluster = u16::from_le_bytes([root[entry*32+26], root[entry*32+27]]) as u32;

        let lba = self.data_lba() + (cluster - 2) * self.bpb.sectors_per_cluster as u32;
        let mut buf = [0u8; 512];
        if old_size > 0 { self.ata.read_sectors(lba, &mut buf, 1); }

        let write_pos = old_size as usize;
        let copy_len = data.len().min(512usize.saturating_sub(write_pos));
        buf[write_pos..write_pos + copy_len].copy_from_slice(&data[..copy_len]);

        if !self.ata.write_sectors(lba, &buf, 1) { return false; }

        let new_size = old_size + data.len() as u32;
        root[entry*32+28..entry*32+32].copy_from_slice(&new_size.to_le_bytes());
        self.ata.write_sectors(root_lba, &root, 1)
    }
}
