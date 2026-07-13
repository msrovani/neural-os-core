use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use super::disk_info::*;

pub trait FilesystemProbe: Send {
    fn fs_type(&self) -> FilesystemType;
    fn priority(&self) -> u8;
    fn probe(&self, read_fn: &dyn Fn(u64, &mut [u8]) -> bool, _max_lba: u64) -> Option<FsInfo>;
}

pub struct FsProbeRegistry {
    probes: Vec<Box<dyn FilesystemProbe>>,
}

impl FsProbeRegistry {
    pub fn new() -> Self {
        let mut reg = FsProbeRegistry { probes: Vec::new() };
        // Ordem por prioridade (maior = tenta primeiro)
        reg.register(Box::new(XfsProbe));       // "XFSB" — very specific
        reg.register(Box::new(NtfsProbe));       // "NTFS    " — specific
        reg.register(Box::new(ExtProbe));        // 0xEF53 — specific
        reg.register(Box::new(BtrfsProbe));      // "_BHRfS_M" — specific
        reg.register(Box::new(Iso9660Probe));    // "CD001" — specific
        reg.register(Box::new(HfsPlusProbe));    // "H+" / "HX"
        reg.register(Box::new(ErOfsProbe));       // EROFS (Android)
        reg.register(Box::new(ReFsProbe));         // ReFS (Windows)
        reg.register(Box::new(Fat32Probe));       // BPB + OEM string
        reg.register(Box::new(ExFatProbe));       // "EXFAT   "
        reg
    }

    pub fn register(&mut self, probe: Box<dyn FilesystemProbe>) {
        self.probes.push(probe);
        self.probes.sort_by(|a, b| b.priority().cmp(&a.priority()));
    }

    pub fn detect(&self, read_fn: &dyn Fn(u64, &mut [u8]) -> bool, max_lba: u64) -> Option<FsInfo> {
        for probe in &self.probes {
            if let Some(info) = probe.probe(read_fn, max_lba) {
                return Some(info);
            }
        }
        None
    }
}

fn read_sector(read_fn: &dyn Fn(u64, &mut [u8]) -> bool, lba: u64, buf: &mut [u8; 512]) -> bool {
    read_fn(lba, buf)
}

// ── FAT32 ──────────────────────────────────────────────────
pub struct Fat32Probe;
impl FilesystemProbe for Fat32Probe {
    fn fs_type(&self) -> FilesystemType { FilesystemType::Fat32 }
    fn priority(&self) -> u8 { 30 }
    fn probe(&self, read_fn: &dyn Fn(u64, &mut [u8]) -> bool, _ml: u64) -> Option<FsInfo> {
        let mut bpb = [0u8; 512];
        if !read_sector(read_fn, 0, &mut bpb) { return None; }
        if bpb[0] != 0xEB && bpb[0] != 0xE9 { return None; }
        if bpb[510] != 0x55 || bpb[511] != 0xAA { return None; }
        let oem = core::str::from_utf8(&bpb[3..11]).ok()?;
        if !oem.starts_with("FAT32") && !oem.starts_with("MSDOS") && !oem.starts_with("MSWIN") {
            let bpb_fat_count = bpb[0x10];
            if bpb_fat_count == 0 { return None; }
        }
        let total_sectors = u32::from_le_bytes([bpb[0x20], bpb[0x21], bpb[0x22], bpb[0x23]]);
        if total_sectors < 65536 { return None; } // FAT32 tem >65525 clusters
        let bps = u16::from_le_bytes([bpb[0x0B], bpb[0x0C]]);
        let spc = bpb[0x0D];
        let label_bytes = &bpb[0x47..0x52];
        let label = core::str::from_utf8(label_bytes).unwrap_or("").trim_end().into();
        let total_bytes = total_sectors as u64 * bps as u64;
        Some(FsInfo { fs_type: FilesystemType::Fat32, label, uuid: String::new(),
            total_bytes, free_bytes: None, block_size: spc as u32 * bps as u32, is_writeable: true })
    }
}

// ── NTFS ──────────────────────────────────────────────────
pub struct NtfsProbe;
impl FilesystemProbe for NtfsProbe {
    fn fs_type(&self) -> FilesystemType { FilesystemType::Ntfs }
    fn priority(&self) -> u8 { 80 }
    fn probe(&self, read_fn: &dyn Fn(u64, &mut [u8]) -> bool, _ml: u64) -> Option<FsInfo> {
        let mut vbr = [0u8; 512];
        if !read_sector(read_fn, 0, &mut vbr) { return None; }
        if &vbr[3..11] != b"NTFS    " { return None; }
        if vbr[510] != 0x55 || vbr[511] != 0xAA { return None; }
        let bps = u16::from_le_bytes([vbr[0x0B], vbr[0x0C]]);
        let spc = vbr[0x0D];
        let total_sectors = u64::from_le_bytes([vbr[0x28], vbr[0x29], vbr[0x2A], vbr[0x2B],
            vbr[0x2C], vbr[0x2D], vbr[0x2E], vbr[0x2F]]);
        let total_bytes = total_sectors * bps as u64;
        Some(FsInfo { fs_type: FilesystemType::Ntfs, label: String::new(), uuid: String::new(),
            total_bytes, free_bytes: None, block_size: spc as u32 * bps as u32, is_writeable: true })
    }
}

// ── EXT2/3/4 ──────────────────────────────────────────────
pub struct ExtProbe;
impl FilesystemProbe for ExtProbe {
    fn fs_type(&self) -> FilesystemType { FilesystemType::Ext4 }
    fn priority(&self) -> u8 { 80 }
    fn probe(&self, read_fn: &dyn Fn(u64, &mut [u8]) -> bool, _ml: u64) -> Option<FsInfo> {
        let mut buf = [0u8; 1024];
        if !read_fn(0, &mut buf) { return None; }
        let offset = if u16::from_le_bytes([buf[0x0B], buf[0x0C]]) == 1024 { 1024 } else { 0 };
        if offset + 1024 > buf.len() { return None; }
        let magic = u16::from_le_bytes([buf[offset + 0x38], buf[offset + 0x39]]);
        if magic != 0xEF53 { return None; }
        let block_size = 1024u32 << buf[offset + 0x18];
        let blocks_total = u32::from_le_bytes([buf[offset+0x04], buf[offset+0x05], buf[offset+0x06], buf[offset+0x07]]);
        let total_bytes = blocks_total as u64 * block_size as u64;
        let label = core::str::from_utf8(&buf[offset+0x78..offset+0x88]).unwrap_or("").trim_end().into();
        let feat = u32::from_le_bytes([buf[offset+0x60], buf[offset+0x61], buf[offset+0x62], buf[offset+0x63]]);
        let fs_type = if feat & 0x04 != 0 { FilesystemType::Ext4 }
                      else if feat & 0x02 != 0 { FilesystemType::Ext3 }
                      else { FilesystemType::Ext2 };
        Some(FsInfo { fs_type, label, uuid: String::new(),
            total_bytes, free_bytes: None, block_size, is_writeable: true })
    }
}

// ── XFS ──────────────────────────────────────────────────
pub struct XfsProbe;
impl FilesystemProbe for XfsProbe {
    fn fs_type(&self) -> FilesystemType { FilesystemType::Xfs }
    fn priority(&self) -> u8 { 90 }
    fn probe(&self, read_fn: &dyn Fn(u64, &mut [u8]) -> bool, _ml: u64) -> Option<FsInfo> {
        let mut sb = [0u8; 512];
        if !read_sector(read_fn, 0, &mut sb) { return None; }
        if &sb[0..4] != b"XFSB" { return None; }
        let block_size = u32::from_be_bytes([sb[0x04], sb[0x05], sb[0x06], sb[0x07]]);
        let blocks = u64::from_be_bytes([sb[0x08], sb[0x09], sb[0x0A], sb[0x0B],
            sb[0x0C], sb[0x0D], sb[0x0E], sb[0x0F]]);
        let label_off = if sb[0x60] == 0 { 0x64 } else { 0x60 }; // v5 has label at 0x60
        let label = core::str::from_utf8(&sb[label_off..label_off+12]).unwrap_or("").trim_end().into();
        Some(FsInfo { fs_type: FilesystemType::Xfs, label, uuid: String::new(),
            total_bytes: blocks * block_size as u64, free_bytes: None,
            block_size, is_writeable: true })
    }
}

// ── ISO9660 ───────────────────────────────────────────────
pub struct Iso9660Probe;
impl FilesystemProbe for Iso9660Probe {
    fn fs_type(&self) -> FilesystemType { FilesystemType::Iso9660 }
    fn priority(&self) -> u8 { 75 }
    fn probe(&self, read_fn: &dyn Fn(u64, &mut [u8]) -> bool, _ml: u64) -> Option<FsInfo> {
        let mut buf = [0u8; 2048];
        if !read_fn(16, &mut buf) { return None; }
        if &buf[1..6] != b"CD001" { return None; }
        let label = core::str::from_utf8(&buf[40..72]).unwrap_or("").trim_end().into();
        let block_size = u16::from_be_bytes([buf[128], buf[129]]) as u32;
        let blocks = u32::from_be_bytes([buf[80], buf[81], buf[82], buf[83]]);
        Some(FsInfo { fs_type: FilesystemType::Iso9660, label, uuid: String::new(),
            total_bytes: blocks as u64 * block_size as u64, free_bytes: None,
            block_size, is_writeable: false })
    }
}

// ── exFAT ─────────────────────────────────────────────────
pub struct ExFatProbe;
impl FilesystemProbe for ExFatProbe {
    fn fs_type(&self) -> FilesystemType { FilesystemType::ExFat }
    fn priority(&self) -> u8 { 70 }
    fn probe(&self, read_fn: &dyn Fn(u64, &mut [u8]) -> bool, _ml: u64) -> Option<FsInfo> {
        let mut vbr = [0u8; 512];
        if !read_sector(read_fn, 0, &mut vbr) { return None; }
        if &vbr[3..11] != b"EXFAT   " { return None; }
        let bps = u16::from_le_bytes([vbr[0x0B], vbr[0x0C]]);
        let spc = vbr[0x0D];
        let total_sectors = u64::from_le_bytes([vbr[0x28], vbr[0x29], vbr[0x2A], vbr[0x2B],
            vbr[0x2C], vbr[0x2D], vbr[0x2E], vbr[0x2F]]);
        let label = core::str::from_utf8(&vbr[0x47..0x55]).unwrap_or("").trim_end().into();
        Some(FsInfo { fs_type: FilesystemType::ExFat, label, uuid: String::new(),
            total_bytes: total_sectors * bps as u64, free_bytes: None,
            block_size: spc as u32 * bps as u32, is_writeable: true })
    }
}

// ── Btrfs ─────────────────────────────────────────────────
pub struct BtrfsProbe;
impl FilesystemProbe for BtrfsProbe {
    fn fs_type(&self) -> FilesystemType { FilesystemType::Btrfs }
    fn priority(&self) -> u8 { 85 }
    fn probe(&self, read_fn: &dyn Fn(u64, &mut [u8]) -> bool, _ml: u64) -> Option<FsInfo> {
        let mut buf = [0u8; 512];
        if !read_fn(128, &mut buf) { return None; } // superblock @ 64KB
        if &buf[0x40..0x48] != b"_BHRfS_M" { return None; }
        let block_size = u32::from_le_bytes([buf[0x20], buf[0x21], buf[0x22], buf[0x23]]);
        let total_bytes = u64::from_le_bytes([buf[0x30], buf[0x31], buf[0x32], buf[0x33],
            buf[0x34], buf[0x35], buf[0x36], buf[0x37]]);
        let label = core::str::from_utf8(&buf[0x60..0x7F]).unwrap_or("").trim_end().into();
        Some(FsInfo { fs_type: FilesystemType::Btrfs, label, uuid: String::new(),
            total_bytes, free_bytes: None, block_size, is_writeable: true })
    }
}

// ── HFS+ ─────────────────────────────────────────────────
pub struct HfsPlusProbe;
impl FilesystemProbe for HfsPlusProbe {
    fn fs_type(&self) -> FilesystemType { FilesystemType::HfsPlus }
    fn priority(&self) -> u8 { 65 }
    fn probe(&self, read_fn: &dyn Fn(u64, &mut [u8]) -> bool, _ml: u64) -> Option<FsInfo> {
        let mut buf = [0u8; 512];
        if !read_fn(2, &mut buf) { return None; }
        let sig = u16::from_be_bytes([buf[0x00], buf[0x01]]);
        if sig != 0x482B && sig != 0x4858 { return None; } // "H+" or "HX"
        let block_size = u32::from_be_bytes([buf[0x08], buf[0x09], buf[0x0A], buf[0x0B]]);
        let blocks = u32::from_be_bytes([buf[0x04], buf[0x05], buf[0x06], buf[0x07]]);
        Some(FsInfo { fs_type: FilesystemType::HfsPlus, label: String::new(), uuid: String::new(),
            total_bytes: blocks as u64 * block_size as u64, free_bytes: None,
            block_size, is_writeable: true })
    }
}

// ── EROFS (Android) ─────────────────────────────────────────
pub struct ErOfsProbe;
impl FilesystemProbe for ErOfsProbe {
    fn fs_type(&self) -> FilesystemType { FilesystemType::ErOfs }
    fn priority(&self) -> u8 { 70 }
    fn probe(&self, read_fn: &dyn Fn(u64, &mut [u8]) -> bool, _ml: u64) -> Option<FsInfo> {
        let mut sb = [0u8; 512];
        if !read_fn(0, &mut sb) { return None; }
        if &sb[0..5] != b"EROFS" { return None; }
        let version = sb[5];
        if version < 1 { return None; }
        let block_size = 1024u32; // EROFS uses 1K blocks typically
        let blocks = u64::from_le_bytes([sb[0x1C], sb[0x1D], sb[0x1E], sb[0x1F],
            sb[0x20], sb[0x21], sb[0x22], sb[0x23]]);
        let label = if version >= 1 {
            core::str::from_utf8(&sb[0x38..0x48]).unwrap_or("").trim_end().into()
        } else { String::new() };
        Some(FsInfo { fs_type: FilesystemType::ErOfs, label, uuid: String::new(),
            total_bytes: blocks as u64 * block_size as u64, free_bytes: None,
            block_size, is_writeable: false })
    }
}

// ── ReFS (Windows Server) ──────────────────────────────────
pub struct ReFsProbe;
impl FilesystemProbe for ReFsProbe {
    fn fs_type(&self) -> FilesystemType { FilesystemType::ReFs }
    fn priority(&self) -> u8 { 65 }
    fn probe(&self, read_fn: &dyn Fn(u64, &mut [u8]) -> bool, _ml: u64) -> Option<FsInfo> {
        let mut vbr = [0u8; 512];
        if !read_fn(0, &mut vbr) { return None; }
        if &vbr[3..11] == b"NTFS    " { return None; } // not ReFS, it's NTFS
        // ReFS has no NTFS-like OEM string. Check for partition type 0x07 elimination.
        // ReFS superblock signature: "ReFS" at various offsets depending on version
        if &vbr[0..4] != b"ReFS" && &vbr[0x10..0x14] != b"ReFS" {
            // Also check for "FSRS" (ReFS v3+)
            if &vbr[0..8] != b"FSRS\0\0\0\0" && &vbr[0x10..0x18] != b"FSRS\0\0\0\0" {
                return None;
            }
        }
        let total_bytes = u64::from_le_bytes([vbr[0x20], vbr[0x21], vbr[0x22], vbr[0x23],
            vbr[0x24], vbr[0x25], vbr[0x26], vbr[0x27]]);
        Some(FsInfo { fs_type: FilesystemType::ReFs, label: String::new(), uuid: String::new(),
            total_bytes, free_bytes: None, block_size: 4096, is_writeable: false })
    }
}
