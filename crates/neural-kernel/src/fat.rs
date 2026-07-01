//! FAT Filesystem + MBR partition management + free space detection.
//! Monta particoes detectadas no VFS, cria particao de dados em espaco livre.

use alloc::boxed::Box;
use alloc::vec;
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

/// Le MBR do primeiro setor (LBA 0) via ATA
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
        serial_println!("[MBR] {}: type={:#04x} LBA={} size={}MB", i+1, type_code, lba, count as u64 * 512 / (1024*1024));
    }
    parts
}

/// Encontra o maior espaco livre nao particionado
pub fn find_free_space(parts: &[Partition], total_sectors: u64) -> (u32, u32) {
    let mut occupied: Vec<(u32, u32)> = parts.iter()
        .map(|p| (p.lba_start, p.lba_start + p.sector_count))
        .collect();
    occupied.sort_by_key(|&(start, _)| start);

    let mut current = 64u32; // primeiros setores reservados (MBR + boot)
    let mut best_start = 0u32;
    let mut best_size = 0u32;

    for &(start, end) in &occupied {
        if start > current {
            let gap = start - current;
            if gap > best_size { best_size = gap; best_start = current; }
        }
        current = core::cmp::max(current, end);
    }

    let final_gap = (total_sectors as u32).saturating_sub(current);
    if final_gap > best_size { best_size = final_gap; best_start = current; }

    if best_size < 2048 { (0, 0) } else { (best_start, best_size) }
}

/// Detecta se e um pendrive bootavel (poucas particoes conhecidas)
pub fn is_bootable_usb(parts: &[Partition]) -> bool {
    let kernel = parts.iter().filter(|p| p.type_code == 0x0C || p.type_code == 0x20).count();
    kernel >= 1 && parts.len() <= 3
}

/// Monta particoes + cria dados em espaco livre
pub unsafe fn mount_partitions(ata: &AtaDriver) {
    let parts = read_mbr(ata);
    if parts.is_empty() { return; }
    let total = ata.total_sectors().unwrap_or(0);
    serial_println!("[DISK] Total: {} setores ({} MB), {} particoes", total, total as u64 * 512 / (1024*1024), parts.len());

    for (i, part) in parts.iter().enumerate() {
        let fs_name = match part.type_code {
            0x01 | 0x06 | 0x0B | 0x0C => "vfat",
            0x07 => "ntfs", 0x83 => "ext3", 0x20 => "oem", _ => "unknown",
        };
        // Tenta abrir como FAT32 (type 0x0B ou 0x0C)
        if part.type_code == 0x0B || part.type_code == 0x0C {
            if let Some(fat32) = Fat32Reader::new(ata, part) {
                let root_list = unsafe { fat32.list_root() };
                serial_println!("[FAT32] Root contents:\n{}", root_list);
            }
        }
        let mount_point = alloc::format!("/mnt/sdhc/p{}", i+1);
        if let Some(ref mut vfs) = *crate::vfs::VFS.lock() {
            vfs.mount(Box::leak(mount_point.clone().into_boxed_str()), fs_name);
        }
        crate::mhi::MHI_REGISTRY.lock().register(
            x86_64::PhysAddr::new(part.lba_start as u64 * 512),
            part.sector_count as usize * 512, crate::mhi::AllocTier::Hdd, &mount_point);
        serial_println!("[DISK] Montado {} type={:#04x} {}MB", mount_point, part.type_code, part.sector_count as u64 * 512 / (1024*1024));
    }

    if total > 0 {
        let (free_start, free_size) = find_free_space(&parts, total);
        let free_mb = free_size as u64 * 512 / (1024*1024);
        let is_usb = is_bootable_usb(&parts);
        serial_println!("[DISK] Livre: LBA {} ({} MB) usb={}", free_start, free_mb, is_usb);
        if free_size > 2048 && is_usb {
            let addr = free_start as u64 * 512;
            crate::mhi::MHI_REGISTRY.lock().register(
                x86_64::PhysAddr::new(addr), free_size as usize * 512, crate::mhi::AllocTier::Hdd, "/mnt/sdhc/data");
            if let Some(ref mut vfs) = *crate::vfs::VFS.lock() { vfs.mount("/mnt/sdhc/data", "ata"); }
            serial_println!("[DISK] + {} MB para dados MHI!", free_mb);
        } else if free_size > 2048 && !is_usb {
            serial_println!("[DISK] HD com {} MB livres. Ignorado (requer confirmacao).", free_mb);
        }
    }
}

// ── FAT32 Reader ──────────────────────────────────────────────
// FAT32 usa 28-bit clusters, root dir como cluster chain, BPB extendido.

pub struct Fat32Reader<'a> {
    ata: &'a AtaDriver,
    pub lba_start: u32,
    pub sectors_per_cluster: u8,
    pub bytes_per_sector: u16,
    reserved_sectors: u16,
    fat_count: u8,
    sectors_per_fat32: u32,
    root_cluster: u32,
    fat_lba: u32,
    data_lba: u32,
}

impl<'a> Fat32Reader<'a> {
    /// Tenta abrir particao FAT32 (type 0x0B ou 0x0C)
    pub unsafe fn new(ata: &'a AtaDriver, part: &Partition) -> Option<Self> {
        if part.type_code != 0x0B && part.type_code != 0x0C { return None; }
        let mut bpb = [0u8; 512];
        if !ata.read_sectors(part.lba_start, &mut bpb, 1) { return None; }

        let bytes_per_sector = u16::from_le_bytes([bpb[0x0B], bpb[0x0C]]);
        let sectors_per_cluster = bpb[0x0D];
        let reserved_sectors = u16::from_le_bytes([bpb[0x0E], bpb[0x0F]]);
        let fat_count = bpb[0x10];
        let sectors_per_fat32 = u32::from_le_bytes([bpb[0x24], bpb[0x25], bpb[0x26], bpb[0x27]]);
        let root_cluster = u32::from_le_bytes([bpb[0x2C], bpb[0x2D], bpb[0x2E], bpb[0x2F]]);

        if bytes_per_sector == 0 || sectors_per_cluster == 0 { return None; }

        let fat_lba = part.lba_start + reserved_sectors as u32;
        let data_lba = fat_lba + fat_count as u32 * sectors_per_fat32;

        serial_println!("[FAT32] BPB: bps={} spc={} fats={} spf={} root_cluster={}",
            bytes_per_sector, sectors_per_cluster, fat_count, sectors_per_fat32, root_cluster);
        serial_println!("[FAT32] fat_lba={} data_lba={}", fat_lba, data_lba);

        Some(Fat32Reader { ata, lba_start: part.lba_start, sectors_per_cluster, bytes_per_sector,
            reserved_sectors, fat_count, sectors_per_fat32, root_cluster, fat_lba, data_lba })
    }

    /// Le o valor da FAT para um cluster (cada entrada tem 28 bits)
    unsafe fn read_fat_entry(&self, cluster: u32) -> u32 {
        let fat_offset = cluster * 4; // cada entrada = 4 bytes
        let fat_sector = self.fat_lba + fat_offset / self.bytes_per_sector as u32;
        let mut sector = [0u8; 512];
        if !self.ata.read_sectors(fat_sector, &mut sector, 1) { return 0xFFF_FFFF; }
        let byte_off = (fat_offset % self.bytes_per_sector as u32) as usize;
        let val = u32::from_le_bytes([
            sector[byte_off], sector[byte_off+1],
            sector[byte_off+2], sector[byte_off+3],
        ]);
        val & 0x0FFF_FFFF // 28-bit cluster value
    }

    /// LBA do primeiro setor de um cluster
    fn cluster_lba(&self, cluster: u32) -> u32 {
        self.data_lba + (cluster - 2) as u32 * self.sectors_per_cluster as u32
    }

    /// Le o diretorio root FAT32 (cluster chain) e lista arquivos
    pub unsafe fn list_root(&self) -> alloc::string::String {
        let mut out = alloc::string::String::from("FAT32 Root:\n");
        let mut cluster = self.root_cluster;

        while cluster < 0x0FFF_FFF8 && cluster >= 2 {
            let lba = self.cluster_lba(cluster);
            let mut buf = vec![0u8; self.sectors_per_cluster as usize * self.bytes_per_sector as usize];
            for i in 0..self.sectors_per_cluster as u32 {
                self.ata.read_sectors(lba + i, &mut buf[i as usize * 512..(i+1) as usize * 512], 1);
            }

            for i in 0..buf.len() / 32 {
                let off = i * 32;
                let first_byte = buf[off];
                if first_byte == 0 { break; } // fim
                if first_byte == 0xE5 { continue; } // deletado
                if buf[off + 11] & 0x08 != 0 { continue; } // volume label / long name

                let name = core::str::from_utf8(&buf[off..off+11]).unwrap_or("???????????");
                let size = u32::from_le_bytes([buf[off+28], buf[off+29], buf[off+30], buf[off+31]]);
                let attr = buf[off+11];
                let dir_flag = if attr & 0x10 != 0 { 'd' } else { '-' };
                out.push_str(&alloc::format!("  {} {:11} {} bytes\n", dir_flag, name, size));
            }

            cluster = self.read_fat_entry(cluster);
        }
        out
    }

    /// Le o conteudo de um arquivo pelo nome na raiz (cluster chain)
    pub unsafe fn read_file(&self, name: &str) -> Option<Vec<u8>> {
        let mut cluster = self.root_cluster;
        let name_upper = name.to_ascii_uppercase();

        while cluster < 0x0FFF_FFF8 && cluster >= 2 {
            let lba = self.cluster_lba(cluster);
            let mut buf = vec![0u8; self.sectors_per_cluster as usize * self.bytes_per_sector as usize];
            for i in 0..self.sectors_per_cluster as u32 {
                self.ata.read_sectors(lba + i, &mut buf[i as usize * 512..(i+1) as usize * 512], 1);
            }

            for entry_off in (0..buf.len()).step_by(32) {
                let first = buf[entry_off];
                if first == 0 { break; }
                if first == 0xE5 { continue; }
                if buf[entry_off + 11] & 0x08 != 0 { continue; }

                let entry_name = core::str::from_utf8(&buf[entry_off..entry_off+11]).unwrap_or("");
                let trimmed = entry_name.trim_end();
                if trimmed != name_upper { continue; }

                let file_size = u32::from_le_bytes([
                    buf[entry_off+28], buf[entry_off+29],
                    buf[entry_off+30], buf[entry_off+31],
                ]) as usize;
                let start_cluster_lo = u16::from_le_bytes([buf[entry_off+26], buf[entry_off+27]]);
                let start_cluster_hi = u16::from_le_bytes([buf[entry_off+20], buf[entry_off+21]]);
                let start_cluster = ((start_cluster_hi as u32) << 16) | start_cluster_lo as u32;

                let mut data = Vec::with_capacity(file_size);
                let mut fc = start_cluster;
                while fc < 0x0FFF_FFF8 && fc >= 2 {
                    let clba = self.cluster_lba(fc);
                    let mut chunk = [0u8; 512];
                    for i in 0..self.sectors_per_cluster as u32 {
                        if data.len() >= file_size { break; }
                        self.ata.read_sectors(clba + i, &mut chunk, 1);
                        let remaining = file_size - data.len();
                        let copy_end = remaining.min(512);
                        data.extend_from_slice(&chunk[..copy_end]);
                    }
                    fc = self.read_fat_entry(fc);
                }
                return Some(data);
            }
            cluster = self.read_fat_entry(cluster);
        }
        None
    }
}

pub struct Fat12Writer<'a> {
    ata: &'a AtaDriver, pub lba_start: u32, bpb: FatBpb,
}

struct FatBpb { reserved: u16, fat_count: u8, root_entries: u16, sectors_per_fat: u16, sectors_per_cluster: u8, }

impl<'a> Fat12Writer<'a> {
    pub unsafe fn new(ata: &'a AtaDriver, part: &Partition) -> Option<Self> {
        if part.type_code != 0x01 { return None; }
        let mut bpb_buf = [0u8; 512];
        if !ata.read_sectors(part.lba_start, &mut bpb_buf, 1) { return None; }
        Some(Fat12Writer { ata, lba_start: part.lba_start,
            bpb: FatBpb {
                reserved: u16::from_le_bytes([bpb_buf[0x0E], bpb_buf[0x0F]]),
                fat_count: bpb_buf[0x10],
                root_entries: u16::from_le_bytes([bpb_buf[0x11], bpb_buf[0x12]]),
                sectors_per_fat: u16::from_le_bytes([bpb_buf[0x16], bpb_buf[0x17]]),
                sectors_per_cluster: bpb_buf[0x0D],
            },
        })
    }

    fn root_lba(&self) -> u32 { self.lba_start + self.bpb.reserved as u32 + self.bpb.fat_count as u32 * self.bpb.sectors_per_fat as u32 }
    fn data_lba(&self) -> u32 { self.root_lba() + (self.bpb.root_entries as u32 * 32 + 511) / 512 }

    pub unsafe fn append_log(&self, data: &[u8]) -> bool {
        if data.len() > 500 { return false; }
        let root_lba = self.root_lba();
        let mut root = [0u8; 512];
        if !self.ata.read_sectors(root_lba, &mut root, 1) { return false; }
        let mut entry = 0usize;
        for i in 0..self.bpb.root_entries as usize {
            if root[i * 32] == 0 { break; }
            if root[i*32..i*32+11] == *b"BOOT    LOG" { entry = i; break; }
        }
        let old_size = u32::from_le_bytes(root[entry*32+28..entry*32+32].try_into().unwrap());
        let cluster = u16::from_le_bytes([root[entry*32+26], root[entry*32+27]]) as u32;
        let lba = self.data_lba() + (cluster - 2) * self.bpb.sectors_per_cluster as u32;
        let mut buf = [0u8; 512];
        if old_size > 0 { self.ata.read_sectors(lba, &mut buf, 1); }
        let write_pos = old_size as usize;
        let copy_len = data.len().min(512usize.saturating_sub(write_pos));
        buf[write_pos..write_pos + copy_len].copy_from_slice(&data[..copy_len]);
        self.ata.write_sectors(lba, &buf, 1);
        root[entry*32+28..entry*32+32].copy_from_slice(&(old_size + data.len() as u32).to_le_bytes());
        self.ata.write_sectors(root_lba, &root, 1)
    }
}
