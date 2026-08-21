//! FAT Filesystem + MBR partition management + free space detection.
//! Monta particoes detectadas no VFS, cria particao de dados em espaco livre.

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use crate::ata::AtaDriver;
use crate::block_dev::BlockDevice;
/// Converte "NAME.EXT" para entrada FAT 8.3 (11 bytes, espaços).
fn encode_83(name: &str) -> [u8; 11] {
    let mut out = [b' '; 11];
    let upper = name.to_ascii_uppercase();
    let (base, ext) = match upper.rsplit_once('.') {
        Some((b, e)) => (b, e),
        None => (upper.as_str(), ""),
    };
    for (i, &c) in base.as_bytes().iter().take(8).enumerate() {
        out[i] = c;
    }
    for (i, &c) in ext.as_bytes().iter().take(3).enumerate() {
        out[8 + i] = c;
    }
    out
}

#[derive(Debug)]
pub struct Partition {
    pub bootable: bool,
    pub type_code: u8,
    pub lba_start: u32,
    pub sector_count: u32,
}

/// GUIDs GPT (bytes little-endian on-disk)
const GPT_ESP: [u8; 16] = [
    0x28, 0x73, 0x2A, 0xC1, 0x1F, 0xF8, 0xD2, 0x11, 0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9, 0x3B,
];
const GPT_BASIC_DATA: [u8; 16] = [
    0xA2, 0xA0, 0xD0, 0xEB, 0xE5, 0xB9, 0x33, 0x44, 0x87, 0xC0, 0x68, 0xB6, 0xB7, 0x26, 0x99, 0xC7,
];
/// NeuralFS GPT type — ver `gpt::GPT_TYPE_NEURALFS`.
const GPT_NEURALFS: [u8; 16] = crate::gpt::GPT_TYPE_NEURALFS;

/// Parseia tabela MBR (4 entradas) a partir do setor 0.
pub fn parse_mbr_sector(mbr: &[u8; 512]) -> Vec<Partition> {
    if mbr[0x1FE] != 0x55 || mbr[0x1FF] != 0xAA {
        return Vec::new();
    }
    let mut parts = Vec::new();
    for i in 0..4 {
        let off = 0x1BE + i * 16;
        let type_code = mbr[off + 4];
        if type_code == 0 {
            continue;
        }
        let lba = u32::from_le_bytes([mbr[off + 8], mbr[off + 9], mbr[off + 10], mbr[off + 11]]);
        let count = u32::from_le_bytes([mbr[off + 12], mbr[off + 13], mbr[off + 14], mbr[off + 15]]);
        parts.push(Partition {
            bootable: mbr[off] == 0x80,
            type_code,
            lba_start: lba,
            sector_count: count,
        });
    }
    parts
}

/// Parseia particoes GPT (ESP→0xEF, Basic Data→0x0C) a partir do header em LBA 1 + entries.
/// `read_sector(lba, buf)` deve preencher 512 bytes.
pub fn parse_gpt_partitions<F>(mut read_sector: F) -> Vec<Partition>
where
    F: FnMut(u64, &mut [u8; 512]) -> bool,
{
    let mut hdr = [0u8; 512];
    if !read_sector(1, &mut hdr) || &hdr[0..8] != b"EFI PART" {
        return Vec::new();
    }
    let entries_lba = u64::from_le_bytes([
        hdr[72], hdr[73], hdr[74], hdr[75], hdr[76], hdr[77], hdr[78], hdr[79],
    ]);
    let entry_count = u32::from_le_bytes([hdr[80], hdr[81], hdr[82], hdr[83]]);
    let entry_size = u32::from_le_bytes([hdr[84], hdr[85], hdr[86], hdr[87]]);
    if entry_count == 0 || entry_count > 128 || entry_size != 128 {
        return Vec::new();
    }
    let per_sec = 512 / entry_size as usize;
    let total_blocks = (entry_count as usize + per_sec - 1) / per_sec;
    let mut parts = Vec::new();
    for blk in 0..total_blocks {
        let mut buf = [0u8; 512];
        if !read_sector(entries_lba + blk as u64, &mut buf) {
            break;
        }
        for ent in 0..per_sec {
            let idx = blk * per_sec + ent;
            if idx >= entry_count as usize {
                break;
            }
            let off = ent * entry_size as usize;
            let type_guid = &buf[off..off + 16];
            if type_guid.iter().all(|&b| b == 0) {
                continue;
            }
            let start = u64::from_le_bytes([
                buf[off + 32],
                buf[off + 33],
                buf[off + 34],
                buf[off + 35],
                buf[off + 36],
                buf[off + 37],
                buf[off + 38],
                buf[off + 39],
            ]);
            let end = u64::from_le_bytes([
                buf[off + 40],
                buf[off + 41],
                buf[off + 42],
                buf[off + 43],
                buf[off + 44],
                buf[off + 45],
                buf[off + 46],
                buf[off + 47],
            ]);
            if start > 0xFFFF_FFFF || end < start || (end - start + 1) > 0xFFFF_FFFF {
                continue;
            }
            let type_code = if type_guid == GPT_ESP {
                0xEFu8
            } else if type_guid == GPT_NEURALFS {
                crate::neural_fs::volume::MBR_TYPE_NEURALFS // 0x7F — NeuralFS nativo
            } else if type_guid == GPT_BASIC_DATA {
                0x0Cu8 // FAT/exFAT dados (USB unificado / Microsoft Basic Data)
            } else {
                0xEEu8
            };
            parts.push(Partition {
                bootable: false,
                type_code,
                lba_start: start as u32,
                sector_count: (end - start + 1) as u32,
            });
        }
    }
    parts
}

fn merge_parts(mbr: Vec<Partition>, gpt: Vec<Partition>) -> Vec<Partition> {
    let mut out = mbr;
    for g in gpt {
        // Pula protective 0xEE e duplicatas por LBA
        if g.type_code == 0xEE {
            continue;
        }
        if out.iter().any(|p| p.lba_start == g.lba_start) {
            continue;
        }
        out.push(g);
    }
    out
}

/// Le MBR (+ GPT se protective 0xEE / header EFI PART) via ATA.
/// USB unificado: MBR hibrido expoe 0x0C; GPT tambem lista Basic Data como 0x0C.
pub fn read_mbr(ata: &AtaDriver) -> Vec<Partition> {
    let mut mbr = [0u8; 512];
    if !unsafe { ata.read_sectors(0, &mut mbr, 1) } {
        crate::slog_nano!("MBR", "info", "Falha ao ler setor 0");
        return Vec::new();
    }
    if mbr[0x1FE] != 0x55 || mbr[0x1FF] != 0xAA {
        crate::slog_nano!("MBR", "info", "Signature 55AA nao encontrada");
        return Vec::new();
    }
    let mut parts = parse_mbr_sector(&mbr);
    for (i, p) in parts.iter().enumerate() {
        crate::slog_nano!("MBR", "info", "{}: type={:#04x} LBA={} size={}MB",
            i + 1,
            p.type_code,
            p.lba_start,
            p.sector_count as u64 * 512 / (1024 * 1024));
    }
    let has_ee = parts.iter().any(|p| p.type_code == 0xEE);
    let has_fat = parts
        .iter()
        .any(|p| p.type_code == 0x0B || p.type_code == 0x0C || p.type_code == 0x1C);
    // Sempre tenta GPT se protective EE, ou se nao ha FAT no MBR (firmware GPT-only)
    if has_ee || !has_fat {
        let gpt = parse_gpt_partitions(|lba, buf| unsafe { ata.read_sectors(lba as u32, buf, 1) });
        if !gpt.is_empty() {
            crate::slog_nano!("GPT", "info", "{} particoes", gpt.len());
            for (i, p) in gpt.iter().enumerate() {
                crate::slog_nano!("GPT", "info", "{}: type={:#04x} LBA={} size={}MB",
                    i + 1,
                    p.type_code,
                    p.lba_start,
                    p.sector_count as u64 * 512 / (1024 * 1024));
            }
            parts = merge_parts(parts, gpt);
        }
    }
    parts
}

/// Le MBR (+ GPT) via qualquer BlockDevice (ATA, USB-MSC, MemoryDisk).
pub fn read_mbr_dev(dev: &mut dyn crate::block_dev::BlockDevice) -> Vec<Partition> {
    let mut mbr = [0u8; 512];
    if !dev.read_sectors(0, &mut mbr) {
        return Vec::new();
    }

    if mbr[0x1FE] != 0x55 || mbr[0x1FF] != 0xAA {
        return Vec::new();
    }
    let mut parts = parse_mbr_sector(&mbr);
    let has_ee = parts.iter().any(|p| p.type_code == 0xEE);
    let has_fat = parts
        .iter()
        .any(|p| p.type_code == 0x0B || p.type_code == 0x0C || p.type_code == 0x1C);
    if has_ee || !has_fat {
        let gpt = parse_gpt_partitions(|lba, buf| dev.read_sectors(lba, buf));
        if !gpt.is_empty() {
            parts = merge_parts(parts, gpt);
        }
    }
    parts
}

/// Disco tem particao FAT32 visivel (MBR 0x0B/0x0C/0x1C ou GPT Basic Data).
pub fn disk_has_fat32(ata: &AtaDriver) -> bool {
    read_mbr(ata)
        .iter()
        .any(|p| p.type_code == 0x0B || p.type_code == 0x0C || p.type_code == 0x1C)
}

// ─── Seam device-abstraído do zero-copy (Pilar 2.2) ───────────────────────────
// Os wrappers da arena (`cortex::gguf::{read_fat_into, fat_lookup_size}`) usam
// o ATA global; este seam roda o MESMO código sobre `&dyn Fat32Io` — host-testável
// com MemoryDisk e target com AtaDriver (coerção automática `&AtaDriver → &dyn`).

/// Partição que o loader de modelos considera FAT32 (MBR/GPT mapeado):
/// 0x0B/0x0C/0x1C clássicos + 0xEF (GPT ESP é FAT32 real — AGENTS.md) e 0x73
/// (exFAT em alguns sticks; `Fat32Reader::new` rejeita se o BPB não casar).
pub fn is_model_fat_partition(t: u8) -> bool {
    matches!(t, 0x0B | 0x0C | 0x1C | 0x73 | 0xEF)
}

/// Le MBR sobre `&dyn Fat32Io` (sem GPT — suficiente p/ raiz de modelos;
/// espelha `read_mbr_dev` no caminho MBR).
pub fn read_mbr_io(io: &dyn Fat32Io) -> Vec<Partition> {
    let mut mbr = [0u8; 512];
    if !io.io_read_sectors(0, &mut mbr, 1) {
        return Vec::new();
    }
    if mbr[0x1FE] != 0x55 || mbr[0x1FF] != 0xAA {
        return Vec::new();
    }
    parse_mbr_sector(&mbr)
}

/// Primeira partição FAT32 (pela ordem do MBR) montada como `Fat32Reader` —
/// o parser abre via `new()` que valida o BPB (bps/spc reais; rejeita não-FAT).
pub unsafe fn fat32_reader_first(io: &dyn Fat32Io) -> Option<Fat32Reader<'_>> {
    for part in read_mbr_io(io) {
        if is_model_fat_partition(part.type_code) {
            if let Some(fs) = Fat32Reader::new(io, &part) {
                return Some(fs);
            }
        }
    }
    None
}

/// Zero-copy genérico: lê o arquivo inteiro direto para `dst` (sem `Vec`
/// intermediário — setores alinhados vão do controlador direto ao destino,
/// que no target é uma página de frame via HHDM). Cap em `dst.len()`.
pub unsafe fn read_fat32_into_io(io: &dyn Fat32Io, name: &str, dst: &mut [u8]) -> Option<usize> {
    let fs = fat32_reader_first(io)?;
    fs.read_file_into(name, dst)
}

/// Tamanho do arquivo na raiz FAT32 (sem ler conteúdo) — dimensiona a
/// alocação na arena antes do `read_fat32_into_io`.
pub unsafe fn fat32_lookup_size_io(io: &dyn Fat32Io, name: &str) -> Option<usize> {
    let fs = fat32_reader_first(io)?;
    fs.lookup_file_size(name)
}

/// Volume de dados QEMU/HW: MBR 0x07 + assinatura `EXFAT   ` no VBR (mkexfat.py).
pub fn disk_has_exfat(ata: &AtaDriver) -> bool {
    for p in read_mbr(ata) {
        if p.type_code != 0x07 && p.type_code != 0xEE {
            // 0x07 clássico; alguns layouts usam GPT — ainda checamos VBR abaixo
        }
        let mut vbr = [0u8; 512];
        if !unsafe { ata.read_sectors(p.lba_start, &mut vbr, 1) } {
            continue;
        }
        if &vbr[3..11] == b"EXFAT   " {
            return true;
        }
    }
    false
}

/// Disco de dados preferível a ESP boot (FAT pequenino / só UEFI).
pub fn disk_has_data_fs(ata: &AtaDriver) -> bool {
    disk_has_fat32(ata) || disk_has_exfat(ata)
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
    let kernel = parts.iter().filter(|p| p.type_code == 0x0C || p.type_code == 0x1C || p.type_code == 0x20).count();
    kernel >= 1 && parts.len() <= 3
}

/// Monta particoes + cria dados em espaco livre
pub unsafe fn mount_partitions(ata: &AtaDriver) {
    let parts = read_mbr(ata);
    if parts.is_empty() { return; }
    let total = ata.total_sectors().unwrap_or(0);
    crate::slog_nano!("DISK", "info", "Total: {} setores ({} MB), {} particoes", total, total as u64 * 512 / (1024*1024), parts.len());

    for (i, part) in parts.iter().enumerate() {
        let fs_name = match part.type_code {
            0x0B | 0x0C | 0x1C | 0x73 => "vfat",
            0x07 => "ntfs", 0x83 => "ext3", 0x20 => "oem", _ => "unknown",
        };
        // Tenta abrir como FAT32 (type 0x0B ou 0x0C)
        if part.type_code == 0x0B || part.type_code == 0x0C || part.type_code == 0x1C || part.type_code == 0x73 {
            if let Some(fat32) = Fat32Reader::new(ata, part) {
                let root_list = unsafe { fat32.list_root() };
                crate::slog_nano!("FAT32", "info", "Root contents:\n{}", root_list);
            }
        }
        let mount_point = alloc::format!("/mnt/sdhc/p{}", i+1);
        if let Some(ref mut vfs) = *crate::vfs::VFS.lock() {
            vfs.mount(Box::leak(mount_point.clone().into_boxed_str()), fs_name);
        }
        crate::mhi::MHI_REGISTRY.lock().register(
            x86_64::PhysAddr::new(part.lba_start as u64 * 512),
            part.sector_count as usize * 512, crate::mhi::AllocTier::Hdd, &mount_point);
        crate::slog_nano!("DISK", "info", "Montado {} type={:#04x} {}MB",
            mount_point, part.type_code, part.sector_count as u64 * 512 / (1024*1024));
    }

    if total > 0 {
        let (free_start, free_size) = find_free_space(&parts, total);
        let free_mb = free_size as u64 * 512 / (1024*1024);
        let is_usb = is_bootable_usb(&parts);
        crate::slog_nano!("DISK", "info", "Livre: LBA {} ({} MB) usb={}", free_start, free_mb, is_usb);
        if free_size > 2048 && is_usb {
            let addr = free_start as u64 * 512;
            crate::mhi::MHI_REGISTRY.lock().register(
                x86_64::PhysAddr::new(addr), free_size as usize * 512, crate::mhi::AllocTier::Hdd, "/mnt/sdhc/data");
            if let Some(ref mut vfs) = *crate::vfs::VFS.lock() { vfs.mount("/mnt/sdhc/data", "ata"); }
            crate::slog_nano!("DISK", "info", "+ {} MB para dados MHI!", free_mb);
        } else if free_size > 2048 && !is_usb {
            crate::slog_nano!("DISK", "info", "HD com {} MB livres. Ignorado (requer confirmacao).", free_mb);
        }
    }
}

// ── FAT32 Reader ──────────────────────────────────────────────
// FAT32 usa 28-bit clusters, root dir como cluster chain, BPB extendido.

/// I/O mínimo exigido pelo parser FAT32.
/// `dyn` permite host-testar o parser com `MemoryDisk` (RAM) e usar o ATA
/// real em bare-metal — zero-copy: `io_read_sectors` grava direto no buffer
/// do chamador (ex: páginas da Cortex Arena via HHDM).
pub trait Fat32Io {
    fn io_read_sectors(&self, lba: u32, buf: &mut [u8], count: u8) -> bool;
    fn io_write_sectors(&self, lba: u32, buf: &[u8], count: u8) -> bool;
}

impl Fat32Io for AtaDriver {
    fn io_read_sectors(&self, lba: u32, buf: &mut [u8], count: u8) -> bool {
        unsafe { AtaDriver::read_sectors(self, lba, buf, count) }
    }
    fn io_write_sectors(&self, lba: u32, buf: &[u8], count: u8) -> bool {
        unsafe { AtaDriver::write_sectors(self, lba, buf, count) }
    }
}

impl Fat32Io for crate::neural_fs::volume::MemoryDisk {
    fn io_read_sectors(&self, lba: u32, buf: &mut [u8], count: u8) -> bool {
        let start = lba as usize * 512;
        let len = count as usize * 512;
        if start + len > self.data.len() || buf.len() < len {
            return false;
        }
        buf[..len].copy_from_slice(&self.data[start..start + len]);
        true
    }
    fn io_write_sectors(&self, lba: u32, buf: &[u8], count: u8) -> bool {
        let start = lba as usize * 512;
        let len = count as usize * 512;
        if start + len > self.data.len() || buf.len() < len {
            return false;
        }
        let dst = unsafe { self.data.as_ptr().add(start) } as *mut u8;
        unsafe { core::ptr::copy_nonoverlapping(buf.as_ptr(), dst, len); }
        true
    }
}

pub struct Fat32Reader<'a> {
    pub io: &'a dyn Fat32Io,
    pub lba_start: u32,
    pub sectors_per_cluster: u8,
    pub bytes_per_sector: u16,
    reserved_sectors: u16,
    fat_count: u8,
    sectors_per_fat32: u32,
    root_cluster: u32,
    fat_lba: u64,
    data_lba: u64,
}

impl<'a> Fat32Reader<'a> {
    /// Tenta abrir particao FAT32 (type 0x0B, 0x0C, 0x1C, 0x73 ou ESP 0xEF)
    pub unsafe fn new(io: &'a dyn Fat32Io, part: &Partition) -> Option<Self> {
        if part.type_code != 0x0B && part.type_code != 0x0C && part.type_code != 0x1C && part.type_code != 0x73 && part.type_code != 0xEF {
            crate::slog_nano!("FAT32", "info", "new: type {:#04x} nao compatível (req 0B/0C/1C/73)", part.type_code);
            return None;
        }
        let mut bpb = [0u8; 512];
        if !io.io_read_sectors(part.lba_start, &mut bpb, 1) {
            crate::slog_nano!("FAT32", "info", "new: falha leitura BPB em LBA {}", part.lba_start);
            return None;
        }

        let bytes_per_sector = u16::from_le_bytes([bpb[0x0B], bpb[0x0C]]);
        let sectors_per_cluster = bpb[0x0D];
        let reserved_sectors = u16::from_le_bytes([bpb[0x0E], bpb[0x0F]]);
        let fat_count = bpb[0x10];
        let root_entry_count = u16::from_le_bytes([bpb[0x11], bpb[0x12]]);

        // Rejeita FAT12/FAT16 — so FAT32 (root_entry_count == 0)
        if root_entry_count > 0 {
            crate::slog_nano!("FAT32", "info", "new: root_entry_count={} nao e FAT32 (FAT12/16 ou BPB inválido)", root_entry_count);
            return None;
        }

        let sectors_per_fat32 = u32::from_le_bytes([bpb[0x24], bpb[0x25], bpb[0x26], bpb[0x27]]);
        let root_cluster = u32::from_le_bytes([bpb[0x2C], bpb[0x2D], bpb[0x2E], bpb[0x2F]]);

        // bps fora de 512..=4096 ou nao-multiplo de 32 -> scans de diretorio
        // (step_by(32) com buf[entry+31]) leem OOB. FAT spec: 512/1024/2048/4096.
        if bytes_per_sector < 512 || bytes_per_sector > 4096 || bytes_per_sector % 32 != 0 || sectors_per_cluster == 0 {
            crate::slog_nano!("FAT32", "info", "new: bps={} spc={} invalido (spec)", bytes_per_sector, sectors_per_cluster);
            return None;
        }

        let fat_lba = part.lba_start as u64 + reserved_sectors as u64;
        let data_lba = fat_lba + fat_count as u64 * sectors_per_fat32 as u64;

        crate::slog_nano!("FAT32", "info", "BPB: bps={} spc={} fats={} spf={} root_cluster={}",
            bytes_per_sector, sectors_per_cluster, fat_count, sectors_per_fat32, root_cluster);
        crate::slog_nano!("FAT32", "info", "fat_lba={} data_lba={}", fat_lba, data_lba);

        Some(Fat32Reader { io, lba_start: part.lba_start, sectors_per_cluster, bytes_per_sector,
            reserved_sectors, fat_count, sectors_per_fat32, root_cluster, fat_lba, data_lba })
    }

    /// Retorna o cluster raiz do diretorio FAT32 (pub para boot_log_agent)
    pub fn get_root_cluster(&self) -> u32 { self.root_cluster }

    /// Le o valor da FAT para um cluster (cada entrada tem 28 bits)
    pub unsafe fn read_fat_entry(&self, cluster: u32) -> u32 {
        let fat_offset = cluster as u64 * 4;
        let fat_sector_u64 = self.fat_lba + fat_offset / self.bytes_per_sector as u64;
        let bps = self.bytes_per_sector as usize;
        let mut sector = [0u8; 4096];
        if !self.io.io_read_sectors(fat_sector_u64 as u32, &mut sector[..bps], (bps / 512) as u8) { return 0xFFF_FFFF; }
        let byte_off = (fat_offset % self.bytes_per_sector as u64) as usize;
        let val = u32::from_le_bytes([
            sector[byte_off], sector[byte_off+1],
            sector[byte_off+2], sector[byte_off+3],
        ]);
        val & 0x0FFF_FFFF // 28-bit cluster value
    }

    /// LBA do primeiro setor de um cluster
    pub fn cluster_lba(&self, cluster: u32) -> u32 {
        // u64 internamente para evitar overflow u32. Cast PRÉ-operação.
        (self.data_lba + (cluster as u64).saturating_sub(2) * self.sectors_per_cluster as u64) as u32
    }

    /// Le o diretorio root FAT32 (cluster chain) e lista arquivos
    pub unsafe fn list_root(&self) -> alloc::string::String {
        let mut out = alloc::string::String::from("FAT32 Root:\n");
        let mut cluster = self.root_cluster;

        while cluster < 0x0FFF_FFF8 && cluster >= 2 {
            let lba = self.cluster_lba(cluster);
            let mut buf = vec![0u8; self.sectors_per_cluster as usize * self.bytes_per_sector as usize];
            for i in 0..self.sectors_per_cluster as u32 {
                if !self.io.io_read_sectors(lba + i, &mut buf[i as usize * self.bytes_per_sector as usize..(i + 1) as usize * self.bytes_per_sector as usize], (self.bytes_per_sector / 512) as u8) {
                    return alloc::string::String::from("FAT32 Root:\n  (read error)\n");
                }
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

    /// Teto de clusters ao varrer root — chain ciclica/corrupta nao pode hangar o scheduler.
    const MAX_ROOT_DIR_CLUSTERS: u32 = 256;

    /// Le um range de bytes de um arquivo pelo nome (streaming).
    /// Retorna bytes de `offset` ate `offset + size` do arquivo.
    pub unsafe fn read_file_range(&self, name: &str, offset: usize, size: usize) -> Option<Vec<u8>> {
        let want = encode_83(name);
        let mut cluster = self.root_cluster;
        let mut walked = 0u32;
        let mut prev = 0u32;

        while cluster < 0x0FFF_FFF8 && cluster >= 2 && walked < Self::MAX_ROOT_DIR_CLUSTERS {
            if cluster == prev {
                break;
            }
            prev = cluster;
            walked += 1;
            let lba = self.cluster_lba(cluster);
            let mut buf = vec![0u8; self.sectors_per_cluster as usize * self.bytes_per_sector as usize];
            for i in 0..self.sectors_per_cluster as u32 {
                if !self.io.io_read_sectors(lba + i, &mut buf[i as usize * self.bytes_per_sector as usize..(i + 1) as usize * self.bytes_per_sector as usize], (self.bytes_per_sector / 512) as u8) {
                    return None;
                }
            }
            for entry_off in (0..buf.len()).step_by(32) {
                let first = buf[entry_off];
                // first==0 = fim do diretorio inteiro (spec FAT) — nao seguir chain
                if first == 0 {
                    return None;
                }
                if first == 0xE5 { continue; }
                if buf[entry_off + 11] & 0x08 != 0 { continue; }
                if buf[entry_off + 11] & 0x0F == 0x0F { continue; }
                if buf[entry_off..entry_off+11] != want { continue; }

                let file_size = u32::from_le_bytes([
                    buf[entry_off+28], buf[entry_off+29],
                    buf[entry_off+30], buf[entry_off+31],
                ]) as usize;
                let start_cluster_lo = u16::from_le_bytes([buf[entry_off+26], buf[entry_off+27]]);
                let start_cluster_hi = u16::from_le_bytes([buf[entry_off+20], buf[entry_off+21]]);
                let start_cluster = ((start_cluster_hi as u32) << 16) | start_cluster_lo as u32;

                let end = (offset + size).min(file_size);
                if offset >= file_size { return None; }
                let actual_size = end - offset;

                let mut data = Vec::with_capacity(actual_size);
                let mut fc = start_cluster;
                let mut pos = 0usize;
                let max_clusters = (file_size / self.bytes_per_sector as usize).max(1) * 2; // safety bound
                let mut cluster_iter = 0usize;
                while fc < 0x0FFF_FFF8 && fc >= 2 && data.len() < actual_size && cluster_iter < max_clusters {
                    let clba = self.cluster_lba(fc);
                    let cluster_bytes = self.sectors_per_cluster as usize * self.bytes_per_sector as usize;
                    for si in 0..self.sectors_per_cluster as u32 {
                        if data.len() >= actual_size { break; }
                        let bps = self.bytes_per_sector as usize;
                        let sector_start = pos + si as usize * bps;
                        if sector_start + bps <= offset {
                            continue; // skip sectors before offset
                        }
                        let mut chunk = [0u8; 4096];
                        if !self.io.io_read_sectors(clba + si, &mut chunk[..bps], (bps / 512) as u8) {
                            return None;
                        }
                        let copy_start = if sector_start < offset { offset - sector_start } else { 0 };
                        let copy_end = bps.min(actual_size - data.len() + copy_start);
                        data.extend_from_slice(&chunk[copy_start..copy_end]);
                    }
                    pos += cluster_bytes;
                    fc = self.read_fat_entry(fc);
                    cluster_iter += 1;
                }
                if data.len() < actual_size {
                    return None; // FAT cluster chain corrupted or truncated
                }
                return Some(data);
            }
            cluster = self.read_fat_entry(cluster);
        }
        None
    }

    /// Localiza entrada de diretório raiz pelo nome 8.3 → (tamanho, cluster inicial).
    /// Caminho compartilhado por leituras (evita duplicar a varredura de dir).
    unsafe fn find_root_entry(&self, name: &str) -> Option<(usize, u32)> {
        let want = encode_83(name);
        let mut cluster = self.root_cluster;
        let mut walked = 0u32;
        let mut prev = 0u32;
        while cluster < 0x0FFF_FFF8 && cluster >= 2 && walked < Self::MAX_ROOT_DIR_CLUSTERS {
            if cluster == prev {
                break;
            }
            prev = cluster;
            walked += 1;
            let lba = self.cluster_lba(cluster);
            let mut buf = vec![0u8; self.sectors_per_cluster as usize * self.bytes_per_sector as usize];
            for i in 0..self.sectors_per_cluster as u32 {
                if !self.io.io_read_sectors(lba + i, &mut buf[i as usize * self.bytes_per_sector as usize..(i + 1) as usize * self.bytes_per_sector as usize], (self.bytes_per_sector / 512) as u8) {
                    return None;
                }
            }
            for entry_off in (0..buf.len()).step_by(32) {
                let first = buf[entry_off];
                if first == 0 {
                    return None;
                }
                if first == 0xE5 { continue; }
                if buf[entry_off + 11] & 0x08 != 0 { continue; }
                if buf[entry_off + 11] & 0x0F == 0x0F { continue; }
                if buf[entry_off..entry_off+11] != want { continue; }
                let file_size = u32::from_le_bytes([
                    buf[entry_off+28], buf[entry_off+29],
                    buf[entry_off+30], buf[entry_off+31],
                ]) as usize;
                let start_cluster_lo = u16::from_le_bytes([buf[entry_off+26], buf[entry_off+27]]);
                let start_cluster_hi = u16::from_le_bytes([buf[entry_off+20], buf[entry_off+21]]);
                let start_cluster = ((start_cluster_hi as u32) << 16) | start_cluster_lo as u32;
                return Some((file_size, start_cluster));
            }
            cluster = self.read_fat_entry(cluster);
        }
        None
    }

    /// Zero-copy: le `size` bytes de `offset` do arquivo DIRETAMENTE para `dst`
    /// (sem `Vec` intermediário). Setores alinhados vão direto do controlador para
    /// o destino — usado para ler modelos direto nas páginas da Cortex Arena.
    /// Retorna `Some(n)` com `n = min(size, file_size - offset)` capado em `dst.len()`.
    pub unsafe fn read_file_range_into(
        &self,
        name: &str,
        offset: usize,
        size: usize,
        dst: &mut [u8],
    ) -> Option<usize> {
        let (file_size, start_cluster) = self.find_root_entry(name)?;
        if offset >= file_size {
            return None;
        }
        let end = offset.saturating_add(size).min(file_size);
        let want = (end - offset).min(dst.len());
        if want == 0 {
            return Some(0);
        }
        let mut written = 0usize;
        let mut fc = start_cluster;
        let max_clusters = (file_size / self.bytes_per_sector as usize).max(1) * 2;
        let mut cluster_iter = 0usize;
        let mut pos = 0usize; // posição absoluta dentro do arquivo
        while fc < 0x0FFF_FFF8 && fc >= 2 && written < want && cluster_iter < max_clusters {
            let clba = self.cluster_lba(fc);
            let spc = self.sectors_per_cluster as u32;
            for si in 0..spc {
                if written >= want { break; }
                let bps = self.bytes_per_sector as usize;
                let sector_start = pos + si as usize * bps;
                if sector_start + bps <= offset { continue; } // setor antes do offset
                // janela útil deste setor (lo..hi relativos ao setor)
                let lo = if sector_start < offset { offset - sector_start } else { 0 };
                let hi = bps.min(want - written + lo);
                if hi <= lo { continue; }
                let dst_off = written;
                // caminho alinhado: setor inteiro lido DIRETO em dst — zero-copy real
                if lo == 0 && hi == bps && dst_off + bps <= dst.len() {
                    if !self.io.io_read_sectors(clba + si, &mut dst[dst_off..dst_off + bps], (bps / 512) as u8) {
                        return None;
                    }
                    written += bps;
                } else {
                    let mut chunk = [0u8; 4096];
                    if !self.io.io_read_sectors(clba + si, &mut chunk[..bps], (bps / 512) as u8) {
                        return None;
                    }
                    let n = hi - lo;
                    if dst_off + n > dst.len() {
                        return None;
                    }
                    dst[dst_off..dst_off + n].copy_from_slice(&chunk[lo..hi]);
                    written += n;
                }
            }
            pos += self.sectors_per_cluster as usize * self.bytes_per_sector as usize;
            fc = self.read_fat_entry(fc);
            cluster_iter += 1;
        }
        if written < want {
            return None; // FAT cluster chain corrompida/truncada
        }
        Some(written)
    }

    /// Zero-copy: le o arquivo inteiro (ou o que couber em `dst`) direto para `dst`.
    pub unsafe fn read_file_into(&self, name: &str, dst: &mut [u8]) -> Option<usize> {
        self.read_file_range_into(name, 0, dst.len(), dst)
    }

    /// Retorna tamanho do arquivo na raiz (8.3), sem ler o conteúdo.
    pub unsafe fn lookup_file_size(&self, name: &str) -> Option<usize> {
        let want = encode_83(name);
        let mut cluster = self.root_cluster;
        let mut walked = 0u32;
        let mut prev = 0u32;
        while cluster < 0x0FFF_FFF8 && cluster >= 2 && walked < Self::MAX_ROOT_DIR_CLUSTERS {
            if cluster == prev {
                break;
            }
            prev = cluster;
            walked += 1;
            let lba = self.cluster_lba(cluster);
            let mut buf = vec![0u8; self.sectors_per_cluster as usize * self.bytes_per_sector as usize];
            for i in 0..self.sectors_per_cluster as u32 {
                self.io.io_read_sectors(lba + i, &mut buf[i as usize * self.bytes_per_sector as usize..(i + 1) as usize * self.bytes_per_sector as usize], (self.bytes_per_sector / 512) as u8);
            }
            for entry_off in (0..buf.len()).step_by(32) {
                let first = buf[entry_off];
                if first == 0 {
                    return None;
                }
                if first == 0xE5 { continue; }
                if buf[entry_off + 11] & 0x08 != 0 { continue; }
                if buf[entry_off + 11] & 0x0F == 0x0F { continue; }
                if buf[entry_off..entry_off+11] != want { continue; }
                let file_size = u32::from_le_bytes([
                    buf[entry_off+28], buf[entry_off+29],
                    buf[entry_off+30], buf[entry_off+31],
                ]) as usize;
                return Some(file_size);
            }
            cluster = self.read_fat_entry(cluster);
        }
        None
    }

    /// Le o conteudo de um arquivo pelo nome na raiz (cluster chain)
    pub unsafe fn read_file(&self, name: &str) -> Option<Vec<u8>> {
        let mut cluster = self.root_cluster;
        let want = encode_83(name);
        let mut walked = 0u32;
        let mut prev = 0u32;

        while cluster < 0x0FFF_FFF8 && cluster >= 2 && walked < Self::MAX_ROOT_DIR_CLUSTERS {
            if cluster == prev {
                break;
            }
            prev = cluster;
            walked += 1;
            let lba = self.cluster_lba(cluster);
            let mut buf = vec![0u8; self.sectors_per_cluster as usize * self.bytes_per_sector as usize];
            for i in 0..self.sectors_per_cluster as u32 {
                self.io.io_read_sectors(lba + i, &mut buf[i as usize * self.bytes_per_sector as usize..(i + 1) as usize * self.bytes_per_sector as usize], (self.bytes_per_sector / 512) as u8);
            }

            for entry_off in (0..buf.len()).step_by(32) {
                let first = buf[entry_off];
                if first == 0 {
                    return None;
                }
                if first == 0xE5 { continue; }
                if buf[entry_off + 11] & 0x08 != 0 { continue; }
                if buf[entry_off + 11] & 0x0F == 0x0F { continue; }
                if buf[entry_off..entry_off+11] != want { continue; }

                let file_size = u32::from_le_bytes([
                    buf[entry_off+28], buf[entry_off+29],
                    buf[entry_off+30], buf[entry_off+31],
                ]) as usize;
                // Cap defensivo: arquivos >64MB via read_file completo nao sao esperados no boot path
                let file_size = file_size.min(64 * 1024 * 1024);
                let start_cluster_lo = u16::from_le_bytes([buf[entry_off+26], buf[entry_off+27]]);
                let start_cluster_hi = u16::from_le_bytes([buf[entry_off+20], buf[entry_off+21]]);
                let start_cluster = ((start_cluster_hi as u32) << 16) | start_cluster_lo as u32;

                let mut data = Vec::with_capacity(file_size);
                let mut fc = start_cluster;
                let max_clusters = (file_size / self.bytes_per_sector as usize).max(1) * 2;
                let mut cluster_iter = 0usize;
                while fc < 0x0FFF_FFF8 && fc >= 2 && data.len() < file_size && cluster_iter < max_clusters {
                    let clba = self.cluster_lba(fc);
                    let bps = self.bytes_per_sector as usize;
                    let mut chunk = [0u8; 4096];
                    for i in 0..self.sectors_per_cluster as u32 {
                        if data.len() >= file_size { break; }
                        if !self.io.io_read_sectors(clba + i, &mut chunk[..bps], (bps / 512) as u8) {
                            return None;
                        }
                        let remaining = file_size - data.len();
                        let copy_end = remaining.min(bps);
                        data.extend_from_slice(&chunk[..copy_end]);
                    }
                    fc = self.read_fat_entry(fc);
                    cluster_iter += 1;
                }
                return Some(data);
            }
            cluster = self.read_fat_entry(cluster);
        }
        None
    }
}

// ── FAT32 Writer ──────────────────────────────────────────────
// Suporta escrita de arquivos no root, alocacao de clusters, atualizacao FAT.
// Usado pelo boot_logger para persistir log de boot em disco.

pub struct Fat32Writer<'a> {
    pub reader: Fat32Reader<'a>,
}

impl<'a> Fat32Writer<'a> {
    pub unsafe fn new(io: &'a dyn Fat32Io, part: &Partition) -> Option<Self> {
        Fat32Reader::new(io, part).map(|reader| Fat32Writer { reader })
    }

    /// Le entrada de diretorio pelo nome (formato 8.3 uppercase via encode_83)
    unsafe fn find_entry(&self, name: &str) -> Option<(u32, u32, u32)> {
        let name_bytes = encode_83(name);

        let mut cluster = self.reader.root_cluster;
        while cluster < 0x0FFF_FFF8 && cluster >= 2 {
            let lba = self.reader.cluster_lba(cluster);
            let cluster_bytes = self.reader.sectors_per_cluster as usize * self.reader.bytes_per_sector as usize;
            let mut buf = vec![0u8; cluster_bytes];
            for i in 0..self.reader.sectors_per_cluster as u32 {
                self.reader.io.io_read_sectors(lba + i, &mut buf[i as usize * self.reader.bytes_per_sector as usize..(i + 1) as usize * self.reader.bytes_per_sector as usize], (self.reader.bytes_per_sector / 512) as u8);
            }
            for entry_off in (0..buf.len()).step_by(32) {
                let first = buf[entry_off];
                if first == 0 || first == 0xE5 { continue; }
                if buf[entry_off + 11] & 0x08 != 0 { continue; }
                if &buf[entry_off..entry_off+11] == &name_bytes {
                    let _size = u32::from_le_bytes([buf[entry_off+28], buf[entry_off+29], buf[entry_off+30], buf[entry_off+31]]);
                    let cluster_lo = u16::from_le_bytes([buf[entry_off+26], buf[entry_off+27]]);
                    let cluster_hi = u16::from_le_bytes([buf[entry_off+20], buf[entry_off+21]]);
                    let start_cluster = ((cluster_hi as u32) << 16) | cluster_lo as u32;
                    return Some((entry_off as u32, lba, start_cluster));
                }
            }
            cluster = self.reader.read_fat_entry(cluster);
        }
        None
    }

    /// Escreve entrada FAT para um cluster
    unsafe fn write_fat_entry(&self, cluster: u32, value: u32) -> bool {
        let fat_offset = cluster as u64 * 4;
        let fat_sector_u64 = self.reader.fat_lba + fat_offset / self.reader.bytes_per_sector as u64;
        let byte_off = (fat_offset % self.reader.bytes_per_sector as u64) as usize;
        let bps = self.reader.bytes_per_sector as usize;
        let mut sector = [0u8; 4096];
        if !self.reader.io.io_read_sectors(fat_sector_u64 as u32, &mut sector[..bps], (bps / 512) as u8) { return false; }
        let val = value & 0x0FFF_FFFF;
        sector[byte_off..byte_off+4].copy_from_slice(&val.to_le_bytes());
        self.reader.io.io_write_sectors(fat_sector_u64 as u32, &sector[..bps], (bps / 512) as u8)
    }

    /// Varre a FAT por N clusters livres (le FAT por setor — nao 1 I/O por entrada).
    /// Budget: no maximo MAX_FAT_SCAN_SECTORS para nao travar o boot em PIO (spf pode ser 16K+).
    unsafe fn find_free_clusters(&self, count: u32) -> Option<Vec<u32>> {
        const MAX_FAT_SCAN_SECTORS: u32 = 65535;
        let bps = self.reader.bytes_per_sector as u32;
        let fat_sectors = self.reader.sectors_per_fat32.min(MAX_FAT_SCAN_SECTORS);
        let entries_per_sector = bps / 4;
        let mut result = Vec::with_capacity(count as usize);
        let mut sector_buf = [0u8; 4096];
        for sec in 0..fat_sectors {
            if result.len() >= count as usize { break; }
            let lba_u64 = self.reader.fat_lba + sec as u64;
            if !self.reader.io.io_read_sectors(lba_u64 as u32, &mut sector_buf[..bps as usize], (bps / 512) as u8) {
                continue;
            }
            let base = sec * entries_per_sector;
            let start = if sec == 0 { 2u32 } else { 0u32 }; // skip FAT[0], FAT[1]
            for i in start..entries_per_sector {
                if result.len() >= count as usize { break; }
                let off = (i * 4) as usize;
                let val = u32::from_le_bytes([
                    sector_buf[off], sector_buf[off + 1],
                    sector_buf[off + 2], sector_buf[off + 3],
                ]) & 0x0FFF_FFFF;
                if val == 0 {
                    result.push(base + i);
                }
            }
        }
        if result.len() >= count as usize {
            Some(result)
        } else {
            crate::slog_nano!("FAT32", "info", "find_free_clusters budget/miss need={} got={} scanned<={}", count, result.len(), fat_sectors);
            None
        }
    }

    /// Cria entrada de diretorio 8.3 no root
    unsafe fn create_entry(&self, name: &str, first_cluster: u32, file_size: u32) -> bool {
        let name_bytes = encode_83(name);

        let mut cluster = self.reader.root_cluster;
        while cluster < 0x0FFF_FFF8 && cluster >= 2 {
            let lba = self.reader.cluster_lba(cluster);
            let cluster_bytes = self.reader.sectors_per_cluster as usize * self.reader.bytes_per_sector as usize;
            let mut buf = vec![0u8; cluster_bytes];
            for i in 0..self.reader.sectors_per_cluster as u32 {
                self.reader.io.io_read_sectors(lba + i, &mut buf[i as usize * self.reader.bytes_per_sector as usize..(i + 1) as usize * self.reader.bytes_per_sector as usize], (self.reader.bytes_per_sector / 512) as u8);
            }
            for entry_off in (0..buf.len()).step_by(32) {
                let first = buf[entry_off];
                if first == 0 || first == 0xE5 {
                    // Slot livre! Preencher entrada
                    buf[entry_off..entry_off+11].copy_from_slice(&name_bytes);
                    buf[entry_off+11] = 0x20; // attr: archive
                    let now = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u16;
                    let time = ((now / 3600) as u16) << 11 | (((now % 3600) / 60) as u16) << 5 | (now as u16 % 60) / 2;
                    let date = ((2026 - 1980) as u16) << 9 | (7 << 5) | 2; // 2026-07-02 fixo
                    buf[entry_off+14..entry_off+16].copy_from_slice(&time.to_le_bytes());
                    buf[entry_off+16..entry_off+18].copy_from_slice(&date.to_le_bytes());
                    let cluster_lo = first_cluster as u16;
                    let cluster_hi = (first_cluster >> 16) as u16;
                    buf[entry_off+20..entry_off+22].copy_from_slice(&cluster_hi.to_le_bytes());
                    buf[entry_off+26..entry_off+28].copy_from_slice(&cluster_lo.to_le_bytes());
                    buf[entry_off+28..entry_off+32].copy_from_slice(&file_size.to_le_bytes());
                    // Escrever cluster de diretorio de volta
                    let bps = self.reader.bytes_per_sector as usize;
                    for i in 0..self.reader.sectors_per_cluster as u32 {
                        let off = i as usize * bps;
                        self.reader.io.io_write_sectors(lba + i, &buf[off..off+bps], (bps / 512) as u8);
                    }
                    return true;
                }
            }
            cluster = self.reader.read_fat_entry(cluster);
        }
        false
    }

    /// Escreve dados em um cluster chain, alocando FAT entries
    unsafe fn write_cluster_chain(&self, _start_cluster: u32, data: &[u8]) -> bool {
        let spc = self.reader.sectors_per_cluster as u32;
        let bps = self.reader.bytes_per_sector as usize;
        let cluster_size = spc as usize * bps;
        let num_clusters = (data.len() + cluster_size - 1) / cluster_size;

        let clusters = match self.find_free_clusters(num_clusters as u32) {
            Some(c) => c,
            None => { crate::slog_nano!("FAT32", "info", "Sem clusters livres!"); return false; }
        };
        let mut written = 0usize;
        for (i, &c) in clusters.iter().enumerate() {
            let lba = self.reader.cluster_lba(c);
            let chunk = &data[written..written + cluster_size.min(data.len() - written)];
            for s in 0..spc {
                let off = s as usize * bps;
                let end = off + bps;
                // SECTOR SHORT-WRITE BUG: se o arquivo < 512B, o setor s=1 fazia
                // &chunk[512..] -> PANIC (range start out of range). Fix: setor
                // sem dados = zeros (cluster FAT32 é maior que o arquivo; o size
                // no dirent limita a leitura). Bug real de HW: WIFI.CFG/BOOT.LOG/
                // TLSPINS.BIN pequenos gravados na ESP panicavam.
                let sector_data = if off >= chunk.len() {
                    &[][..]
                } else if end <= chunk.len() {
                    &chunk[off..end]
                } else {
                    &chunk[off..]
                };
                let bps = self.reader.bytes_per_sector as usize;
                let mut sector = [0u8; 4096];
                sector[..sector_data.len()].copy_from_slice(sector_data);
                if !self.reader.io.io_write_sectors(lba + s, &sector[..bps], (bps / 512) as u8) {
                    return false;
                }
            }
            written += cluster_size;
            // FAT entry: aponta para proximo cluster ou EOC
            let next = if i + 1 < clusters.len() { clusters[i+1] } else { 0x0FFF_FFF8 };
            if !self.write_fat_entry(c, next) { return false; }
        }
        true
    }

    /// Escreve arquivo completo no root (cria ou substitui)
    pub unsafe fn write_file(&self, name: &str, data: &[u8]) -> bool {
        let cluster_size = self.reader.sectors_per_cluster as usize * self.reader.bytes_per_sector as usize;
        let num_clusters = (data.len() + cluster_size - 1) / cluster_size;

        // Se arquivo ja existe, reusa primeiro cluster
        if let Some((_off, _lba, first_cluster)) = self.find_entry(name) {
            // Sobrescrever: reutilizar cluster inicial, alocar mais se necessario
            let existing_clusters = (0u32..).scan(first_cluster, |c, _| {
                if *c < 2 || *c >= 0x0FFF_FFF8 { return None; }
                let cur = *c;
                *c = unsafe { self.reader.read_fat_entry(cur) };
                Some(cur)
            }).count();

            if existing_clusters >= num_clusters {
                // Clusters existentes sao suficientes — soh escrever dados
                return self.write_cluster_chain(first_cluster, data);
            }
            // Precisamos liberar clusters antigos e alocar novos
            // Simplificacao: usar write_cluster_chain com novos clusters
            // (deixamos clusters antigos orfaos — GC em proximo boot)
            // Marcar antigo cluster como free
            let mut c = first_cluster;
            while c >= 2 && c < 0x0FFF_FFF8 {
                let next = unsafe { self.reader.read_fat_entry(c) };
                if !unsafe { self.write_fat_entry(c, 0) } {
                    return false;
                }
                c = next;
            }
            // Alocar novos e escrever
            if !self.write_cluster_chain(first_cluster, data) { return false; }
            // Atualizar tamanho na entrada
            self.update_file_size(name, core::cmp::min(data.len(), u32::MAX as usize) as u32)
        } else {
            // Arquivo novo: alocar clusters e criar entrada
            let clusters = match self.find_free_clusters(num_clusters as u32) {
                Some(c) => c,
                None => { crate::slog_nano!("FAT32", "info", "Sem clusters livres!"); return false; }
            };
            if !self.write_cluster_chain(clusters[0], data) { return false; }
            self.create_entry(name, clusters[0], core::cmp::min(data.len(), u32::MAX as usize) as u32)
        }
    }

    /// Tamanho atual do arquivo no root (None se inexistente).
    unsafe fn file_size(&self, name: &str) -> Option<u32> {
        let name_bytes = encode_83(name);
        let mut cluster = self.reader.root_cluster;
        while cluster < 0x0FFF_FFF8 && cluster >= 2 {
            let lba = self.reader.cluster_lba(cluster);
            let cluster_bytes =
                self.reader.sectors_per_cluster as usize * self.reader.bytes_per_sector as usize;
            let mut buf = vec![0u8; cluster_bytes];
            for i in 0..self.reader.sectors_per_cluster as u32 {
                self.reader.io.io_read_sectors(
                    lba + i,
                    &mut buf[i as usize * self.reader.bytes_per_sector as usize
                        ..(i + 1) as usize * self.reader.bytes_per_sector as usize],
                    (self.reader.bytes_per_sector / 512) as u8,
                );
            }
            for entry_off in (0..buf.len()).step_by(32) {
                let first = buf[entry_off];
                if first == 0 || first == 0xE5 {
                    continue;
                }
                if buf[entry_off + 11] & 0x08 != 0 {
                    continue;
                }
                if &buf[entry_off..entry_off + 11] == &name_bytes {
                    return Some(u32::from_le_bytes([
                        buf[entry_off + 28],
                        buf[entry_off + 29],
                        buf[entry_off + 30],
                        buf[entry_off + 31],
                    ]));
                }
            }
            cluster = self.reader.read_fat_entry(cluster);
        }
        None
    }

    /// Escreve `data` nos clusters já alocados (encadeia FAT).
    unsafe fn write_data_to_clusters(&self, clusters: &[u32], data: &[u8]) -> bool {
        if clusters.is_empty() {
            return data.is_empty();
        }
        let spc = self.reader.sectors_per_cluster as u32;
        let bps = self.reader.bytes_per_sector as usize;
        let cluster_size = spc as usize * bps;
        let mut written = 0usize;
        for (i, &c) in clusters.iter().enumerate() {
            let lba = self.reader.cluster_lba(c);
            let remain = data.len().saturating_sub(written);
            let chunk_len = remain.min(cluster_size);
            let chunk = &data[written..written + chunk_len];
            for s in 0..spc {
                let off = s as usize * bps;
                let mut sector = [0u8; 512];
                if off < chunk.len() {
                    let end = (off + bps).min(chunk.len());
                    sector[..end - off].copy_from_slice(&chunk[off..end]);
                }
                if !self.reader.io.io_write_sectors(lba + s, &sector, 1) {
                    return false;
                }
            }
            written += chunk_len;
            let next = if i + 1 < clusters.len() {
                clusters[i + 1]
            } else {
                0x0FFF_FFF8
            };
            if !self.write_fat_entry(c, next) {
                return false;
            }
        }
        true
    }

    /// Append chunks (stream-to-disk). Cria o arquivo se não existir.
    pub unsafe fn append_file(&self, name: &str, data: &[u8]) -> bool {
        if data.is_empty() {
            return true;
        }
        let cluster_size =
            self.reader.sectors_per_cluster as usize * self.reader.bytes_per_sector as usize;

        if self.find_entry(name).is_none() {
            return self.write_file(name, data);
        }

        let mut size = self.file_size(name).unwrap_or(0) as usize;
        let (_off, _lba, first_cluster) = match self.find_entry(name) {
            Some(e) => e,
            None => return false,
        };

        // Último cluster da chain
        let mut last = first_cluster;
        let mut c = first_cluster;
        while c >= 2 && c < 0x0FFF_FFF8 {
            last = c;
            c = self.reader.read_fat_entry(c);
        }

        let mut src = 0usize;
        // Completa espaço livre no último cluster (parcial)
        let pad = size % cluster_size;
        if pad != 0 && last >= 2 {
            let space = cluster_size - pad;
            let take = space.min(data.len());
            let lba = self.reader.cluster_lba(last);
            let mut cluster_buf = vec![0u8; cluster_size];
            let bps = self.reader.bytes_per_sector as usize;
            for s in 0..self.reader.sectors_per_cluster as u32 {
                let off = s as usize * bps;
                self.reader.io.io_read_sectors(
                    lba + s,
                    &mut cluster_buf[off..off + bps],
                    (bps / 512) as u8,
                );
            }
            cluster_buf[pad..pad + take].copy_from_slice(&data[..take]);
            for s in 0..self.reader.sectors_per_cluster as u32 {
                let off = s as usize * bps;
                if !self
                    .reader
                    .io
                    .io_write_sectors(lba + s, &cluster_buf[off..off + bps], (bps / 512) as u8)
                {
                    return false;
                }
            }
            src = take;
            size += take;
        }

        let remaining = &data[src..];
        if !remaining.is_empty() {
            let num = (remaining.len() + cluster_size - 1) / cluster_size;
            let clusters = match self.find_free_clusters(num as u32) {
                Some(c) => c,
                None => {
                    crate::slog_nano!("FAT32", "info", "append: sem clusters livres need={}", num);
                    return false;
                }
            };
            if last >= 2 && last < 0x0FFF_FFF8 {
                if !self.write_fat_entry(last, clusters[0]) {
                    return false;
                }
            }
            if !self.write_data_to_clusters(&clusters, remaining) {
                return false;
            }
            size += remaining.len();
        }

        self.update_file_size(name, core::cmp::min(size, u32::MAX as usize) as u32)
    }

    /// Atualiza tamanho do arquivo na entrada de diretorio
    unsafe fn update_file_size(&self, name: &str, size: u32) -> bool {
        let name_bytes = encode_83(name);

        let mut cluster = self.reader.root_cluster;
        while cluster < 0x0FFF_FFF8 && cluster >= 2 {
            let lba = self.reader.cluster_lba(cluster);
            let cluster_bytes = self.reader.sectors_per_cluster as usize * self.reader.bytes_per_sector as usize;
            let mut buf = vec![0u8; cluster_bytes];
            for i in 0..self.reader.sectors_per_cluster as u32 {
                self.reader.io.io_read_sectors(lba + i, &mut buf[i as usize * self.reader.bytes_per_sector as usize..(i + 1) as usize * self.reader.bytes_per_sector as usize], (self.reader.bytes_per_sector / 512) as u8);
            }
            for entry_off in (0..buf.len()).step_by(32) {
                let first = buf[entry_off];
                if first == 0 || first == 0xE5 { continue; }
                if buf[entry_off + 11] & 0x08 != 0 { continue; }
                if &buf[entry_off..entry_off+11] == &name_bytes {
                    buf[entry_off+28..entry_off+32].copy_from_slice(&size.to_le_bytes());
                    let bps = self.reader.bytes_per_sector as usize;
                    for i in 0..self.reader.sectors_per_cluster as u32 {
                        let off = i as usize * bps;
                        self.reader.io.io_write_sectors(lba + i, &buf[off..off+bps], (bps / 512) as u8);
                    }
                    return true;
                }
            }
            cluster = self.reader.read_fat_entry(cluster);
        }
        false
    }
}

// ── FAT32 Format ───────────────────────────────────────────────
/// Formata uma partição como FAT32 (ESP). Cria BPB, FSInfo, duas FATs,
/// diretório raiz vazio. A partição deve ter >= 65525 clusters (>= ~32MB).
/// `dev` — BlockDevice (já posicionado na partição via start_lba).
/// `start_lba` — LBA absoluto da partição.
/// `part_sectors` — tamanho da partição em setores.
/// Formata uma partição como FAT32 (ESP) — 512B/setor, spc=1 (compat com o
/// pipeline Limine/instalador). Delega ao generalizado `format_fat32_bps`.
pub fn format_fat32_esp(
    dev: &mut dyn BlockDevice,
    start_lba: u64,
    part_sectors: u64,
) -> bool {
    // 4Kn: usa bps real do dispositivo (512, 1024, 2048, 4096).
    // ESP do Limine exige bps=512; discos 4Kn reais usam o bps detectado
    // para format_fat32_bps (partition有自己的FAT32 com bps nativo).
    let bps = dev.sector_size();
    format_fat32_bps(dev, start_lba, part_sectors, bps, 1)
}

/// Formata partição FAT32 com `bps` ∈ {512,1024,2048,4096} (Fase 1 v2.0 — 4Kn).
/// `start_lba`/`part_sectors` em unidades de BLOCO do dispositivo (setor BPB);
/// internamente traduz para LBAs 512 do `BlockDevice` via `mult = bps/512`.
/// A partição precisa de >= 65525 clusters reais (FAT32 spec).
pub fn format_fat32_bps(
    dev: &mut dyn BlockDevice,
    start_lba: u64,
    part_sectors: u64,
    bps: u16,
    spc: u32,
) -> bool {
    if !(512..=4096).contains(&bps) || bps % 32 != 0 || spc == 0 {
        return false;
    }
    let mult = (bps / 512) as u64; // bloco bps → LBA 512 do BlockDevice
    // Parâmetros FAT32 (reuso do Python mk_esp_fat.py)
    let reserved: u64 = 32;
    let fats: u64 = 2;
    let part = part_sectors as u64;

    // Calcula FAT sectors (em unidades de setor BPB)
    let data_sectors = part.saturating_sub(reserved + fats);
    let clusters = data_sectors / spc as u64;
    if clusters < 65525 {
        return false; // não é FAT32 real
    }
    let fat_sectors = ((clusters + 2) * 4 + bps as u64 - 1) / bps as u64;
    let data_start = reserved + fats * fat_sectors;
    if data_start >= part {
        return false;
    }

    // ── BPB (bloco start_lba + 0) ──
    // BPB structure is always 512 bytes; for 4Kn (bps=4096), pad to bps.
    let mut bpb_raw = [0u8; 512];
    bpb_raw[0..3].copy_from_slice(b"\xeb\x58\x90");
    bpb_raw[3..11].copy_from_slice(b"MSWIN4.1");
    bpb_raw[0x0B..0x0D].copy_from_slice(&bps.to_le_bytes());       // bytes per sector
    bpb_raw[0x0D] = spc as u8;                                     // sectors per cluster
    bpb_raw[0x0E..0x10].copy_from_slice(&(reserved as u16).to_le_bytes()); // reserved sectors
    bpb_raw[0x10] = fats as u8;                                     // FAT count
    bpb_raw[0x11..0x13].copy_from_slice(&0u16.to_le_bytes());      // root entry count = 0
    bpb_raw[0x13..0x15].copy_from_slice(&0u16.to_le_bytes());      // total sectors 16 = 0
    bpb_raw[0x15] = 0xF8;                                           // media descriptor
    bpb_raw[0x16..0x18].copy_from_slice(&0u16.to_le_bytes());      // FAT sz 16 = 0
    bpb_raw[0x18..0x1A].copy_from_slice(&63u16.to_le_bytes());     // sectors per track
    bpb_raw[0x1A..0x1C].copy_from_slice(&255u16.to_le_bytes());    // heads
    bpb_raw[0x1C..0x20].copy_from_slice(&(start_lba as u32).to_le_bytes()); // hidden sectors
    bpb_raw[0x20..0x24].copy_from_slice(&(part as u32).to_le_bytes());      // total sectors 32
    bpb_raw[0x24..0x28].copy_from_slice(&(fat_sectors as u32).to_le_bytes()); // FAT size 32
    bpb_raw[0x28..0x2A].copy_from_slice(&0u16.to_le_bytes());      // extended flags
    bpb_raw[0x2A..0x2C].copy_from_slice(&0u16.to_le_bytes());      // FS version
    bpb_raw[0x2C..0x30].copy_from_slice(&2u32.to_le_bytes());      // root cluster = 2
    bpb_raw[0x30..0x32].copy_from_slice(&1u16.to_le_bytes());      // FSInfo sector = 1
    bpb_raw[0x32..0x34].copy_from_slice(&6u16.to_le_bytes());      // backup boot sector = 6
    bpb_raw[0x40] = 0x80;                                           // drive number
    bpb_raw[0x41] = 0x00;                                           // reserved
    bpb_raw[0x42] = 0x29;                                           // boot signature
    bpb_raw[0x43..0x47].copy_from_slice(&0x4E45524Fu32.to_le_bytes()); // volume ID
    bpb_raw[0x47..0x52].copy_from_slice(b"NEURAL-ESP ");           // volume label
    bpb_raw[0x52..0x5A].copy_from_slice(b"FAT32   ");              // FAT type label
    bpb_raw[0x1FE..0x200].copy_from_slice(b"\x55\xAA");            // boot signature
    // Para 4Kn: copiar BPB 512B para buffer bps-size (resto = zero)
    let mut bpb_sec = alloc::vec![0u8; bps as usize];
    bpb_sec[..512].copy_from_slice(&bpb_raw);
    if !dev.write_sectors(start_lba * mult, &bpb_sec) {
        return false;
    }

    // ── FSInfo (bloco start_lba + 1) ──
    let mut fsi_raw = [0u8; 512];
    fsi_raw[0..4].copy_from_slice(&0x41615252u32.to_le_bytes());   // lead sig
    fsi_raw[0x1E4..0x1E8].copy_from_slice(&0x61417272u32.to_le_bytes()); // struct sig
    fsi_raw[0x1E8..0x1EC].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // free cluster count (unknown)
    fsi_raw[0x1EC..0x1F0].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // next free cluster
    fsi_raw[0x1FE..0x200].copy_from_slice(b"\x55\xAA");
    let mut fsi_sec = alloc::vec![0u8; bps as usize];
    fsi_sec[..512].copy_from_slice(&fsi_raw);
    if !dev.write_sectors((start_lba + 1) * mult, &fsi_sec) {
        return false;
    }

    // ── Backup BPB (bloco start_lba + 6) ──
    if !dev.write_sectors((start_lba + 6) * mult, &bpb_sec) {
        return false;
    }

    // ── Backup FSInfo (bloco start_lba + 7) ──
    if !dev.write_sectors((start_lba + 7) * mult, &fsi_sec) {
        return false;
    }

    // ── FATs (bloco start_lba + reserved) ──
    let fat_bytes = (fat_sectors * bps as u64) as usize;
    let mut fat = alloc::vec![0u8; fat_bytes];
    // Cluster 0: media descriptor
    fat[0..4].copy_from_slice(&0x0FFFFFF8u32.to_le_bytes());
    // Cluster 1: EOC (dirty)
    fat[4..8].copy_from_slice(&0x0FFFFFFFu32.to_le_bytes());
    // Cluster 2: EOC (root directory, 1 cluster)
    fat[8..12].copy_from_slice(&0x0FFFFFFFu32.to_le_bytes());
    // FAT1
    let fat1_lba = start_lba + reserved;
    let total_fat_sectors = fat_bytes / bps as usize;
    for i in 0..total_fat_sectors {
        let off = i * bps as usize;
        let end = (off + bps as usize).min(fat_bytes);
        let mut sector = alloc::vec![0u8; bps as usize];
        sector[..end - off].copy_from_slice(&fat[off..end]);
        if !dev.write_sectors((fat1_lba + i as u64) * mult, &sector) {
            return false;
        }
    }
    // FAT2 (cópia)
    for i in 0..total_fat_sectors {
        let off = i * bps as usize;
        let end = (off + bps as usize).min(fat_bytes);
        let mut sector = alloc::vec![0u8; bps as usize];
        sector[..end - off].copy_from_slice(&fat[off..end]);
        if !dev.write_sectors((fat1_lba + fat_sectors + i as u64) * mult, &sector) {
            return false;
        }
    }

    // ── Diretório raiz (cluster 2, bloco data_start) ──
    // Raiz vazio — só precisa existir. Um cluster zerado é suficiente.
    let root_lba = start_lba + data_start;
    let root_sector = alloc::vec![0u8; spc as usize * bps as usize];
    if !dev.write_sectors(root_lba * mult, &root_sector) {
        return false;
    }

    true
}
/// Escreve mensagem no arquivo de log do boot atual (FAT32, B<TICK>.LOG).
pub unsafe fn write_boot_log(ata: &AtaDriver, part: &Partition, msg: &str) -> bool {
    let boot_tick = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
    let tick_sec = boot_tick * 55 / 1000;
    let log_line = alloc::format!("[T+{}.{:03}] {}\n", tick_sec / 1000, tick_sec % 1000, msg);

    let writer = match Fat32Writer::new(ata, part) { Some(w) => w, None => return false };
    let mut name = alloc::format!("B{:07X}.LOG", boot_tick);
    name.truncate(11);

    if writer.find_entry(&name).is_some() {
        if let Some(existing) = writer.reader.read_file(&name) {
            let mut new_data = existing;
            new_data.extend_from_slice(log_line.as_bytes());
            writer.write_file(&name, &new_data)
        } else {
            writer.write_file(&name, log_line.as_bytes())
        }
    } else {
        writer.write_file(&name, log_line.as_bytes())
    }
}

/// Lê um range de bytes de um arquivo FAT32 pelo nome, via ATA driver.
/// Helper standalone para streaming de tensores GGUF (AirLLM).
/// Retorna bytes de `offset` até `offset + size` do arquivo.
pub unsafe fn read_file_range_by_name(
    path: &str,
    offset: usize,
    size: usize,
) -> Option<Vec<u8>> {
    let name = path.trim().to_uppercase();
    let ata = crate::ATA_DRIVER.lock();
    let ata = ata.as_ref()?;
    let parts = read_mbr(ata);
    for part in &parts {
        if part.type_code == 0x0B || part.type_code == 0x0C || part.type_code == 0x1C || part.type_code == 0x73 {
            let fs = Fat32Reader::new(ata, part)?;
            return fs.read_file_range(&name, offset, size);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::neural_fs::volume::MemoryDisk;

    /// Round-trip FAT32 completo em host (sem hardware): formata um volume real
    /// via `format_fat32_esp` sobre MemoryDisk, escreve arquivo multi-cluster via
    /// Fat32Writer e valida as leituras zero-copy (`read_file_into` /
    /// `read_file_range_into`) contra o payload original.
    /// Também prova que bps/spc são lidos do BPB (não zerados) e que o parser
    /// rejeita BPB inválido.
    #[test]
    fn fat32_roundtrip_zero_copy() {
        let part_sectors = 128 * 1024u32; // 64MB >= 65525 clusters exigidos
        let mut disk = MemoryDisk::new(part_sectors as usize * 512);

        // Formata o volume (BlockDevice dyn — caminho real de ESP/instalador)
        assert!(format_fat32_esp(&mut disk, 0, part_sectors as u64));

        let part = Partition {
            bootable: false,
            type_code: 0x0C, // FAT32 LBA
            lba_start: 0,
            sector_count: part_sectors,
        };

        // Parser abre o volume formatado: bps=512 e spc=1 vêm do BPB real
        let fs = unsafe { Fat32Reader::new(&disk, &part) }.expect("FAT32 mount");
        assert_eq!(fs.bytes_per_sector, 512);
        assert_eq!(fs.sectors_per_cluster, 1);

        // Arquivo multi-cluster (3000 bytes = 6 clusters com spc=1)
        let payload: Vec<u8> = (0..3000u32).map(|i| (i % 251) as u8).collect();
        let writer = unsafe { Fat32Writer::new(&disk, &part) }.expect("FAT32 writer");
        assert!(unsafe { writer.write_file("TEST.BIN", &payload) });

        // 1. Leitura completa zero-copy
        let mut buf = [0u8; 4096];
        let n = unsafe { fs.read_file_into("TEST.BIN", &mut buf) }.expect("read_into");
        assert_eq!(n, payload.len());
        assert_eq!(&buf[..n], &payload[..]);

        // 2. Paridade com o caminho legado (Vec)
        let legacy = unsafe { fs.read_file("TEST.BIN") }.expect("read_file");
        assert_eq!(legacy, payload);

        // 3. Range alinhado a setor (offset 1024, 512 bytes) — zero-copy puro
        let mut rbuf = [0u8; 512];
        let n = unsafe { fs.read_file_range_into("TEST.BIN", 1024, 512, &mut rbuf) }.expect("range");
        assert_eq!(n, 512);
        assert_eq!(&rbuf[..], &payload[1024..1536]);

        // 4. Range parcial no 1º setor (offset 100, 100 bytes) — caminho chunk
        let mut pb = [0u8; 100];
        let n = unsafe { fs.read_file_range_into("TEST.BIN", 100, 100, &mut pb) }.expect("partial");
        assert_eq!(n, 100);
        assert_eq!(&pb[..], &payload[100..200]);

        // 5. Destino menor que o arquivo — capa em dst.len()
        let mut tiny = [0u8; 10];
        let n = unsafe { fs.read_file_into("TEST.BIN", &mut tiny) }.expect("tiny");
        assert_eq!(n, 10);
        assert_eq!(&tiny[..], &payload[..10]);

        // 6. Offset além do fim -> None; nome inexistente -> None
        assert!(unsafe { fs.read_file_range_into("TEST.BIN", 3000, 1, &mut [0u8; 1]) }.is_none());
        assert!(unsafe { fs.read_file_into("NAOEXIST.BIN", &mut buf) }.is_none());
    }

    /// Simula dispositivo 4Kn (LBA endereça blocos de 4096B). O parser FAT32
    /// deve funcionar com bps=4096 sem nenhum 512 hardcoded no caminho de
    /// dados — a generalização Fase 1 (bps-driven).
    struct FourKnIo<'a> {
        disk: &'a MemoryDisk,
    }

    impl<'a> Fat32Io for FourKnIo<'a> {
        fn io_read_sectors(&self, lba: u32, buf: &mut [u8], count: u8) -> bool {
            let start = lba as usize * 4096;
            let len = count as usize * 512;
            if start + len > self.disk.data.len() || buf.len() < len {
                return false;
            }
            buf[..len].copy_from_slice(&self.disk.data[start..start + len]);
            true
        }
        fn io_write_sectors(&self, lba: u32, buf: &[u8], count: u8) -> bool {
            let start = lba as usize * 4096;
            let len = count as usize * 512;
            if start + len > self.disk.data.len() || buf.len() < len {
                return false;
            }
            let dst = unsafe { self.disk.data.as_ptr().add(start) } as *mut u8;
            unsafe { core::ptr::copy_nonoverlapping(buf.as_ptr(), dst, len); }
            true
        }
    }

    /// Round-trip FAT32 com bps=4096 (4Kn): formata, escreve arquivo multi-
    /// cluster e lê de volta via zero-copy — prova que o parser é bps-driven
    /// (entradas FAT, diretórios e dados em setores de 4096B).
    #[test]
    fn fat32_4kn_roundtrip() {
        // 512MB = 131072 blocos de 4096 — FAT32 4Kn exige >= 65525 clusters
        let part_blocks = 128 * 1024u32;
        let disk = MemoryDisk::new(part_blocks as usize * 4096);
        let mut dev = disk; // precisa &mut para format via BlockDevice
        assert!(format_fat32_bps(&mut dev, 0, part_blocks as u64, 4096, 1));

        let part = Partition {
            bootable: false,
            type_code: 0x0C,
            lba_start: 0,
            sector_count: part_blocks,
        };
        let io = FourKnIo { disk: &dev };
        let fs = unsafe { Fat32Reader::new(&io, &part) }.expect("mount 4Kn");
        assert_eq!(fs.bytes_per_sector, 4096);
        assert_eq!(fs.sectors_per_cluster, 1);

        // Arquivo de 2+ clusters 4Kn (8KB + 300B)
        let payload: Vec<u8> = (0..(8192u32 + 300)).map(|i| (i % 249) as u8).collect();
        let writer = unsafe { Fat32Writer::new(&io, &part) }.expect("writer 4Kn");
        assert!(unsafe { writer.write_file("TEST.BIN", &payload) });

        // Leitura completa zero-copy (bps=4096)
        let mut buf = [0u8; 9000];
        let n = unsafe { fs.read_file_into("TEST.BIN", &mut buf) }.expect("read 4Kn");
        assert_eq!(n, payload.len());
        assert_eq!(&buf[..n], &payload[..]);

        // Range não-alinhado cruzando setores 4Kn (offset 4000, 700 bytes)
        let mut rb = [0u8; 700];
        let n = unsafe { fs.read_file_range_into("TEST.BIN", 4000, 700, &mut rb) }.expect("range 4Kn");
        assert_eq!(n, 700);
        assert_eq!(&rb[..], &payload[4000..4700]);

        // Entrada FAT (read_fat_entry) com bps=4096: root cluster 2 = EOC
        // (format grava 0x0FFFFFFF — range EOC válido 0x0FFF_FFF8..=0x0FFF_FFFF)
        let eoc = unsafe { fs.read_fat_entry(2) } & 0x0FFF_FFFF;
        assert!(eoc >= 0x0FFF_FFF8, "EOC esperado, got {:#x}", eoc);
    }

    /// BPB inválido (bps zerado) deve ser rejeitado — a classe de bug
    /// "bps/spc lendo 0" não pode montar um volume corrompido.
    #[test]
    fn fat32_rejects_invalid_bpb() {
        let mut disk = MemoryDisk::new(512 * 1024);
        // BPB lixo: bps=0, spc=0, root_entry_count=0 (parece FAT32 de longe)
        disk.data[0x0B] = 0;
        disk.data[0x0C] = 0;
        disk.data[0x0D] = 0;
        let part = Partition {
            bootable: false,
            type_code: 0x0C,
            lba_start: 0,
            sector_count: 1024,
        };
        assert!(unsafe { Fat32Reader::new(&disk, &part) }.is_none());
    }

    /// Seam device-abstraído (Pilar 2.2): `read_fat32_into_io`/`fat32_lookup_size_io`
    /// rodam sobre `&dyn Fat32Io` — o MESMO caminho que o wrapper da arena
    /// (`cortex::gguf::read_fat_into`) usa com o ATA global no target. Prova o
    /// fluxo "MBR scan → 1ª partição FAT32 → leitura direta no destino" em host.
    #[test]
    fn fat32_io_seam_zero_copy_into() {
        // Layout realista (QEMU/HW): MBR em setor 0 + partição FAT32 em LBA 2048.
        let start_lba = 2048u32;
        let part_sectors = 128 * 1024u32; // 64MB >= 65525 clusters
        let mut disk = MemoryDisk::new((start_lba as usize + part_sectors as usize) * 512);
        // MBR: assinatura 55AA + entrada 0x0C apontando para LBA 2048
        disk.data[0x1FE] = 0x55;
        disk.data[0x1FF] = 0xAA;
        let off = 0x1BE;
        disk.data[off] = 0x00; // status (não bootável)
        disk.data[off + 4] = 0x0C; // FAT32 LBA
        disk.data[off + 8..off + 12].copy_from_slice(&start_lba.to_le_bytes());
        disk.data[off + 12..off + 16].copy_from_slice(&part_sectors.to_le_bytes());
        assert!(format_fat32_esp(&mut disk, start_lba as u64, part_sectors as u64));

        let part = Partition {
            bootable: false,
            type_code: 0x0C,
            lba_start: start_lba,
            sector_count: part_sectors,
        };
        // Arquivo "modelo" multi-cluster (3000B = 6 clusters com spc=1)
        let payload: Vec<u8> = (0..3000u32).map(|i| (i % 251) as u8).collect();
        let writer = unsafe { Fat32Writer::new(&disk, &part) }.expect("writer");
        assert!(unsafe { writer.write_file("MODEL.BIN", &payload) });

        // lookup sem ler conteúdo (dimensiona a alocação da arena)
        assert_eq!(unsafe { fat32_lookup_size_io(&disk, "MODEL.BIN") }, Some(payload.len()));

        // zero-copy direto no destino (a "página da arena" em host = buffer)
        let mut dst = [0u8; 4096];
        let n = unsafe { read_fat32_into_io(&disk, "MODEL.BIN", &mut dst) }.expect("into");
        assert_eq!(n, payload.len());
        assert_eq!(&dst[..n], &payload[..]);

        // cap em dst.len() (destino menor que o arquivo)
        let mut tiny = [0u8; 10];
        let n = unsafe { read_fat32_into_io(&disk, "MODEL.BIN", &mut tiny) }.expect("tiny");
        assert_eq!(n, 10);
        assert_eq!(&tiny[..], &payload[..10]);

        // nome inexistente → None gracioso (o loader da arena continua no stub)
        assert!(unsafe { read_fat32_into_io(&disk, "NAOEXIST.BIN", &mut dst) }.is_none());
        assert!(unsafe { fat32_lookup_size_io(&disk, "NAOEXIST.BIN") }.is_none());
    }
}



