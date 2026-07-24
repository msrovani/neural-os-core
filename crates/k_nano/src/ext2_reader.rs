//! EXT2/3/4 reader — ADR-0072 Labor 13 (read-only MVP) + Labor 24 write opt-in.
//! list/read com ponteiros clássicos + extents leaf (magic 0xF30A).
//! Write: 1 ficheiro root sem journal (flag opt-in).

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::block_dev::BlockDevice;
use crate::fs_driver::{FilesystemDriver, FsInfo};

const EXT4_EXTENTS: u32 = 0x0008_0000;
const EXT_MAGIC: u16 = 0xF30A;

/// Labor 24 — deny by default (corrupção sem journal).
static EXT_WRITE_OPTIN: AtomicBool = AtomicBool::new(false);

pub fn enable_ext_write(on: bool) {
    EXT_WRITE_OPTIN.store(on, Ordering::Relaxed);
    crate::slog_nano!(
        "EXT4",
        "info",
        "step=write_optin status={} (sem journal)",
        if on { "ON" } else { "OFF" }
    );
}

pub fn ext_write_enabled() -> bool {
    EXT_WRITE_OPTIN.load(Ordering::Relaxed)
}

pub struct Ext2Reader {
    start_lba: u64,
    block_size: u32,
    inodes_per_group: u32,
    blocks_per_group: u32,
    inode_size: u16,
    total_inodes: u32,
    first_data_block: u32,
    label: String,
    feature_incompat: u32,
    /// Cache root: (name, is_dir, inode).
    root_cache: Vec<(String, bool, u32)>,
    mounted: bool,
    /// Cache file bodies for trait read (path -> bytes) — filled on mount smoke helpers.
    file_cache: Vec<(String, Vec<u8>)>,
}

impl Ext2Reader {
    fn block_to_lba(&self, block: u32) -> u64 {
        self.start_lba + block as u64 * (self.block_size as u64 / 512)
    }

    fn read_block(&self, dev: &mut dyn BlockDevice, block: u32, buf: &mut [u8]) -> bool {
        if buf.len() < self.block_size as usize {
            return false;
        }
        let lba = self.block_to_lba(block);
        let sectors = self.block_size as usize / 512;
        for i in 0..sectors {
            if !dev.read_sectors(lba + i as u64, &mut buf[i * 512..(i + 1) * 512]) {
                return false;
            }
        }
        true
    }

    fn gdt_block(&self) -> u32 {
        // Group 0 descriptor starts at first_data_block+1 (block 1 if 1K; block 0+1 if 2K/4K after SB).
        if self.block_size == 1024 {
            2
        } else {
            1
        }
    }

    fn read_inode(&self, dev: &mut dyn BlockDevice, inode: u32) -> Option<Vec<u8>> {
        if inode == 0 || inode > self.total_inodes {
            return None;
        }
        let group = (inode - 1) / self.inodes_per_group;
        let index = (inode - 1) % self.inodes_per_group;
        let mut gdt = vec![0u8; self.block_size as usize];
        if !self.read_block(dev, self.gdt_block() + group, &mut gdt) {
            // Fallback: group 0 only in first GDT block
            if group != 0 || !self.read_block(dev, self.gdt_block(), &mut gdt) {
                return None;
            }
        }
        let ent_off = if group == 0 {
            0
        } else {
            // Re-read block 0 GDT with offset — simplified: only group 0 for MVP smoke
            0
        };
        let _ = ent_off;
        let off = (group % (self.block_size as u32 / 32)) as usize * 32;
        if off + 12 > gdt.len() {
            return None;
        }
        let inode_table_block = u32::from_le_bytes([gdt[off + 8], gdt[off + 9], gdt[off + 10], gdt[off + 11]]);
        if inode_table_block == 0 {
            return None;
        }
        let byte_off = index as u64 * self.inode_size as u64;
        let block = inode_table_block + (byte_off / self.block_size as u64) as u32;
        let within = (byte_off % self.block_size as u64) as usize;
        let mut blk = vec![0u8; self.block_size as usize];
        if !self.read_block(dev, block, &mut blk) {
            return None;
        }
        let mut out = vec![0u8; self.inode_size as usize];
        let n = out.len().min(blk.len().saturating_sub(within));
        out[..n].copy_from_slice(&blk[within..within + n]);
        if n < out.len() {
            let mut blk2 = vec![0u8; self.block_size as usize];
            if self.read_block(dev, block + 1, &mut blk2) {
                let rest = out.len() - n;
                out[n..].copy_from_slice(&blk2[..rest]);
            }
        }
        Some(out)
    }

    fn inode_mode(inode: &[u8]) -> u16 {
        u16::from_le_bytes([inode[0], inode[1]])
    }

    fn inode_size_bytes(inode: &[u8]) -> u64 {
        let lo = u32::from_le_bytes([inode[4], inode[5], inode[6], inode[7]]) as u64;
        let hi = if inode.len() >= 112 {
            u32::from_le_bytes([inode[108], inode[109], inode[110], inode[111]]) as u64
        } else {
            0
        };
        lo | (hi << 32)
    }

    fn inode_flags(inode: &[u8]) -> u32 {
        if inode.len() < 36 {
            return 0;
        }
        u32::from_le_bytes([inode[32], inode[33], inode[34], inode[35]])
    }

    fn collect_blocks_classic(
        &self,
        dev: &mut dyn BlockDevice,
        inode: &[u8],
        out: &mut Vec<u32>,
        max: usize,
    ) {
        for i in 0..12 {
            if out.len() >= max {
                return;
            }
            let b = u32::from_le_bytes([
                inode[40 + i * 4],
                inode[41 + i * 4],
                inode[42 + i * 4],
                inode[43 + i * 4],
            ]);
            if b != 0 {
                out.push(b);
            }
        }
        // Single indirect only for MVP
        let indir = u32::from_le_bytes([inode[88], inode[89], inode[90], inode[91]]);
        if indir != 0 && out.len() < max {
            let mut blk = vec![0u8; self.block_size as usize];
            if self.read_block(dev, indir, &mut blk) {
                for i in 0..(self.block_size as usize / 4) {
                    if out.len() >= max {
                        break;
                    }
                    let b = u32::from_le_bytes([
                        blk[i * 4],
                        blk[i * 4 + 1],
                        blk[i * 4 + 2],
                        blk[i * 4 + 3],
                    ]);
                    if b != 0 {
                        out.push(b);
                    }
                }
            }
        }
    }

    fn collect_blocks_extents(
        &self,
        _dev: &mut dyn BlockDevice,
        inode: &[u8],
        out: &mut Vec<u32>,
        max: usize,
    ) {
        // i_block[0..] as extent header at offset 40
        if inode.len() < 52 {
            return;
        }
        let magic = u16::from_le_bytes([inode[40], inode[41]]);
        if magic != EXT_MAGIC {
            return;
        }
        let entries = u16::from_le_bytes([inode[42], inode[43]]) as usize;
        let depth = u16::from_le_bytes([inode[46], inode[47]]);
        if depth != 0 {
            return; // index nodes residual
        }
        let mut off = 52usize;
        for _ in 0..entries {
            if off + 12 > inode.len() || out.len() >= max {
                break;
            }
            let len = u16::from_le_bytes([inode[off + 4], inode[off + 5]]) as u32;
            let start_hi = u16::from_le_bytes([inode[off + 6], inode[off + 7]]) as u64;
            let start_lo = u32::from_le_bytes([
                inode[off + 8],
                inode[off + 9],
                inode[off + 10],
                inode[off + 11],
            ]) as u64;
            let start = (start_lo | (start_hi << 32)) as u32;
            for i in 0..len {
                if out.len() >= max {
                    break;
                }
                out.push(start.wrapping_add(i));
            }
            off += 12;
        }
    }

    fn inode_data_blocks(
        &self,
        dev: &mut dyn BlockDevice,
        inode: &[u8],
        max: usize,
    ) -> Vec<u32> {
        let mut out = Vec::new();
        if Self::inode_flags(inode) & EXT4_EXTENTS != 0 {
            self.collect_blocks_extents(dev, inode, &mut out, max);
        } else {
            self.collect_blocks_classic(dev, inode, &mut out, max);
        }
        out
    }

    /// Lista diretório (inode) → (name, is_dir, inode).
    pub fn list_dir(
        &self,
        dev: &mut dyn BlockDevice,
        dir_inode: u32,
    ) -> Vec<(String, bool, u32)> {
        let Some(inode) = self.read_inode(dev, dir_inode) else {
            return Vec::new();
        };
        if (Self::inode_mode(&inode) & 0xF000) != 0x4000 {
            return Vec::new();
        }
        let size = Self::inode_size_bytes(&inode) as usize;
        let blocks = self.inode_data_blocks(dev, &inode, 64);
        let mut entries = Vec::new();
        let mut blk = vec![0u8; self.block_size as usize];
        let mut read = 0usize;
        for b in blocks {
            if read >= size {
                break;
            }
            if !self.read_block(dev, b, &mut blk) {
                break;
            }
            let mut off = 0usize;
            while off + 8 <= self.block_size as usize && read + off < size {
                let ino = u32::from_le_bytes([blk[off], blk[off + 1], blk[off + 2], blk[off + 3]]);
                let rec_len = u16::from_le_bytes([blk[off + 4], blk[off + 5]]) as usize;
                if rec_len < 8 || off + rec_len > self.block_size as usize {
                    break;
                }
                let name_len = blk[off + 6] as usize;
                let file_type = blk[off + 7];
                if ino != 0 && name_len > 0 && off + 8 + name_len <= blk.len() {
                    let name = String::from(
                        core::str::from_utf8(&blk[off + 8..off + 8 + name_len]).unwrap_or(""),
                    );
                    if name != "." && name != ".." {
                        let is_dir = file_type == 2
                            || (file_type == 0 && {
                                self.read_inode(dev, ino)
                                    .map(|i| (Self::inode_mode(&i) & 0xF000) == 0x4000)
                                    .unwrap_or(false)
                            });
                        entries.push((name, is_dir, ino));
                    }
                }
                off += rec_len;
            }
            read += self.block_size as usize;
        }
        entries
    }

    pub fn read_file_bytes(
        &self,
        dev: &mut dyn BlockDevice,
        path: &str,
        max: usize,
    ) -> Option<Vec<u8>> {
        let name = path.trim_matches('/');
        if name.is_empty() || name.contains('/') {
            // MVP: só root-level
            if name.contains('/') {
                return None;
            }
        }
        let inode_num = if name.is_empty() {
            2
        } else {
            self.list_dir(dev, 2)
                .into_iter()
                .find(|(n, _, _)| n == name)
                .map(|(_, _, i)| i)?
        };
        let inode = self.read_inode(dev, inode_num)?;
        if Self::inode_mode(&inode) & 0xF000 == 0x4000 {
            return None;
        }
        let size = Self::inode_size_bytes(&inode).min(max as u64) as usize;
        let blocks = self.inode_data_blocks(dev, &inode, (size / self.block_size as usize) + 2);
        let mut data = Vec::with_capacity(size);
        let mut blk = vec![0u8; self.block_size as usize];
        for b in blocks {
            if data.len() >= size {
                break;
            }
            if !self.read_block(dev, b, &mut blk) {
                break;
            }
            let take = (size - data.len()).min(self.block_size as usize);
            data.extend_from_slice(&blk[..take]);
        }
        Some(data)
    }

    fn write_block(&self, dev: &mut dyn BlockDevice, block: u32, buf: &[u8]) -> bool {
        if buf.len() < self.block_size as usize {
            return false;
        }
        let lba = self.block_to_lba(block);
        let sectors = self.block_size as usize / 512;
        for i in 0..sectors {
            if !dev.write_sectors(lba + i as u64, &buf[i * 512..(i + 1) * 512]) {
                return false;
            }
        }
        true
    }

    /// Labor 24: write MVP opt-in — create/overwrite root file (sem journal).
    /// Só se `enable_ext_write(true)`. Aloca 1 bloco + inode via bitmap grupo 0.
    pub fn write_file_root_optin(
        &mut self,
        dev: &mut dyn BlockDevice,
        name: &str,
        data: &[u8],
    ) -> Result<(), &'static str> {
        if !EXT_WRITE_OPTIN.load(core::sync::atomic::Ordering::Relaxed) {
            return Err("ext_write_optin_off");
        }
        let name = name.trim_matches('/');
        if name.is_empty() || name.len() > 255 || name.contains('/') {
            return Err("bad_name");
        }
        if data.len() > self.block_size as usize {
            return Err("data_gt_block");
        }
        // Block bitmap @ gdt+4 (bg_block_bitmap)
        let mut gdt = alloc::vec![0u8; self.block_size as usize];
        if !self.read_block(dev, self.gdt_block(), &mut gdt) {
            return Err("gdt_read");
        }
        let bmap_blk = u32::from_le_bytes([gdt[0], gdt[1], gdt[2], gdt[3]]);
        let imap_blk = u32::from_le_bytes([gdt[4], gdt[5], gdt[6], gdt[7]]);
        let itable = u32::from_le_bytes([gdt[8], gdt[9], gdt[10], gdt[11]]);
        if bmap_blk == 0 || imap_blk == 0 || itable == 0 {
            return Err("gdt_zero");
        }
        let mut bmap = alloc::vec![0u8; self.block_size as usize];
        if !self.read_block(dev, bmap_blk, &mut bmap) {
            return Err("bmap_read");
        }
        let mut free_block = 0u32;
        for bit in 0..(self.blocks_per_group as usize).min(bmap.len() * 8) {
            let byte = bit / 8;
            let mask = 1u8 << (bit % 8);
            if bmap[byte] & mask == 0 {
                bmap[byte] |= mask;
                free_block = self.first_data_block + bit as u32;
                break;
            }
        }
        if free_block == 0 {
            return Err("no_free_block");
        }
        let mut imap = alloc::vec![0u8; self.block_size as usize];
        if !self.read_block(dev, imap_blk, &mut imap) {
            return Err("imap_read");
        }
        // Inode 1-based; skip 1..10 reserved; find free >= 11
        let mut free_ino = 0u32;
        for ino in 11..=self.inodes_per_group.min(self.total_inodes) {
            let bit = (ino - 1) as usize;
            let byte = bit / 8;
            let mask = 1u8 << (bit % 8);
            if byte < imap.len() && imap[byte] & mask == 0 {
                imap[byte] |= mask;
                free_ino = ino;
                break;
            }
        }
        if free_ino == 0 {
            return Err("no_free_inode");
        }
        let mut blk = alloc::vec![0u8; self.block_size as usize];
        blk[..data.len()].copy_from_slice(data);
        if !self.write_block(dev, free_block, &blk) {
            return Err("data_write");
        }
        // Build inode (classic, no extents)
        let mut inode = alloc::vec![0u8; self.inode_size as usize];
        inode[0] = 0x81;
        inode[1] = 0x81; // mode regular 0644-ish
        let sz = data.len() as u32;
        inode[4..8].copy_from_slice(&sz.to_le_bytes());
        inode[28] = 1; // i_blocks (512-byte units) approx
        inode[40..44].copy_from_slice(&free_block.to_le_bytes());
        let byte_off = ((free_ino - 1) % self.inodes_per_group) as u64 * self.inode_size as u64;
        let iblock = itable + (byte_off / self.block_size as u64) as u32;
        let within = (byte_off % self.block_size as u64) as usize;
        let mut itbl = alloc::vec![0u8; self.block_size as usize];
        if !self.read_block(dev, iblock, &mut itbl) {
            return Err("itable_read");
        }
        let n = inode.len().min(itbl.len().saturating_sub(within));
        itbl[within..within + n].copy_from_slice(&inode[..n]);
        if !self.write_block(dev, iblock, &itbl) {
            return Err("itable_write");
        }
        if !self.write_block(dev, bmap_blk, &bmap) || !self.write_block(dev, imap_blk, &imap) {
            return Err("bitmap_write");
        }
        // Append dirent to root (inode 2) first data block
        let root_ino = self.read_inode(dev, 2).ok_or("root_inode")?;
        let root_blocks = self.inode_data_blocks(dev, &root_ino, 1);
        let rb = *root_blocks.first().ok_or("root_empty")?;
        let mut dir = alloc::vec![0u8; self.block_size as usize];
        if !self.read_block(dev, rb, &mut dir) {
            return Err("dir_read");
        }
        // Find end of last entry / free space
        let mut off = 0usize;
        let mut insert_at = None;
        while off + 8 <= dir.len() {
            let inode_n = u32::from_le_bytes([dir[off], dir[off + 1], dir[off + 2], dir[off + 3]]);
            let rec = u16::from_le_bytes([dir[off + 4], dir[off + 5]]) as usize;
            if rec < 8 {
                break;
            }
            if inode_n == 0 && rec >= 8 + name.len() {
                insert_at = Some(off);
                break;
            }
            let name_len = dir[off + 6] as usize;
            let used = (8 + name_len + 3) & !3;
            if inode_n != 0 && rec > used + 8 + name.len() {
                // Split entry
                let new_rec = used;
                let rest = rec - new_rec;
                dir[off + 4] = (new_rec & 0xff) as u8;
                dir[off + 5] = (new_rec >> 8) as u8;
                insert_at = Some(off + new_rec);
                dir[off + new_rec] = 0;
                dir[off + new_rec + 1] = 0;
                dir[off + new_rec + 2] = 0;
                dir[off + new_rec + 3] = 0;
                dir[off + new_rec + 4] = (rest & 0xff) as u8;
                dir[off + new_rec + 5] = (rest >> 8) as u8;
                break;
            }
            off += rec;
            if off >= dir.len() {
                break;
            }
        }
        let at = insert_at.ok_or("dir_full")?;
        let rec_len = ((8 + name.len() + 3) & !3).max(8 + name.len());
        if at + rec_len > dir.len() {
            return Err("dir_overflow");
        }
        dir[at..at + 4].copy_from_slice(&free_ino.to_le_bytes());
        // keep existing rec len if splitting left it
        if u16::from_le_bytes([dir[at + 4], dir[at + 5]]) == 0 {
            dir[at + 4] = (rec_len & 0xff) as u8;
            dir[at + 5] = (rec_len >> 8) as u8;
        }
        dir[at + 6] = name.len() as u8;
        dir[at + 7] = 1; // file type
        dir[at + 8..at + 8 + name.len()].copy_from_slice(name.as_bytes());
        if !self.write_block(dev, rb, &dir) {
            return Err("dir_write");
        }
        self.file_cache.push((String::from(name), data.to_vec()));
        self.root_cache
            .push((String::from(name), false, free_ino));
        crate::slog_nano!(
            "EXT4",
            "info",
            "step=write status=OK name={} ino={} blk={} len={} VERDICT=PASS reason=optin_mvp",
            name,
            free_ino,
            free_block,
            data.len()
        );
        Ok(())
    }

    /// Mount + cache root; optionally prefetch first small file for trait read.
    pub fn mount_and_cache(&mut self, dev: &mut dyn BlockDevice) -> Result<usize, &'static str> {
        self.root_cache = self.list_dir(dev, 2);
        self.file_cache.clear();
        // Prefetch até 2 arquivos pequenos (<8KiB) para trait::read
        let names: Vec<(String, u32)> = self
            .root_cache
            .iter()
            .filter(|(_, is_dir, _)| !*is_dir)
            .take(2)
            .map(|(n, _, i)| (n.clone(), *i))
            .collect();
        for (name, _ino) in names {
            if let Some(bytes) = self.read_file_bytes(dev, &name, 8192) {
                self.file_cache.push((name, bytes));
            }
        }
        self.mounted = true;
        Ok(self.root_cache.len())
    }
}

impl FilesystemDriver for Ext2Reader {
    fn name(&self) -> &str {
        if self.feature_incompat & 0x40 != 0 || self.feature_incompat & 0x4 != 0 {
            "ext4"
        } else {
            "ext2"
        }
    }

    fn detect(dev: &mut dyn BlockDevice, start_lba: u64) -> Option<Self> {
        let mut sb = [0u8; 1024];
        // Superblock at offset 1024 from partition start
        if !dev.read_sectors(start_lba + 2, &mut sb[0..512]) {
            return None;
        }
        if !dev.read_sectors(start_lba + 3, &mut sb[512..1024]) {
            return None;
        }
        if sb[56] != 0x53 || sb[57] != 0xEF {
            return None;
        }
        let total_inodes = u32::from_le_bytes([sb[0], sb[1], sb[2], sb[3]]);
        let log_bs = u32::from_le_bytes([sb[24], sb[25], sb[26], sb[27]]);
        let block_size = 1024u32 << log_bs;
        let inodes_per_group = u32::from_le_bytes([sb[40], sb[41], sb[42], sb[43]]);
        let blocks_per_group = u32::from_le_bytes([sb[32], sb[33], sb[34], sb[35]]);
        let first_data_block = u32::from_le_bytes([sb[20], sb[21], sb[22], sb[23]]);
        let inode_size = {
            let v = u16::from_le_bytes([sb[88], sb[89]]);
            if v == 0 {
                128
            } else {
                v
            }
        };
        let feature_incompat = u32::from_le_bytes([sb[96], sb[97], sb[98], sb[99]]);
        let mut label = String::new();
        for &c in &sb[120..136] {
            if c == 0 {
                break;
            }
            if c.is_ascii() {
                label.push(c as char);
            }
        }
        Some(Ext2Reader {
            start_lba,
            block_size,
            inodes_per_group,
            blocks_per_group,
            inode_size,
            total_inodes,
            first_data_block,
            label,
            feature_incompat,
            root_cache: Vec::new(),
            mounted: false,
            file_cache: Vec::new(),
        })
    }

    fn mount(&mut self, dev: &mut dyn BlockDevice, start_lba: u64) -> Result<FsInfo, &'static str> {
        if start_lba != self.start_lba {
            *self = Self::detect(dev, start_lba).ok_or("not ext")?;
        }
        let n = self.mount_and_cache(dev)?;
        let _ = n;
        let fs_type: &'static str = if self.feature_incompat & 0x40 != 0 || self.feature_incompat & 0x4 != 0
        {
            "ext4"
        } else {
            "ext2"
        };
        Ok(FsInfo {
            fs_type,
            label: self.label.clone(),
            total_bytes: 0,
            free_bytes: None,
            block_size: self.block_size,
            writable: false,
        })
    }

    fn read(&self, path: &str, offset: u64, buf: &mut [u8]) -> Result<usize, &'static str> {
        if !self.mounted {
            return Err("ext not mounted");
        }
        let name = path.trim_matches('/');
        if let Some((_, data)) = self.file_cache.iter().find(|(n, _)| n == name) {
            if offset as usize >= data.len() {
                return Ok(0);
            }
            let start = offset as usize;
            let n = (data.len() - start).min(buf.len());
            buf[..n].copy_from_slice(&data[start..start + n]);
            return Ok(n);
        }
        Err("ext read: path not in cache — use read_file_bytes(dev)")
    }

    fn write(&mut self, path: &str, _offset: u64, data: &[u8]) -> Result<(), &'static str> {
        // Trait write sem BlockDevice — só cache se opt-in já gravou.
        if !ext_write_enabled() {
            return Err("ext read-only (enable_ext_write)");
        }
        let name = path.trim_matches('/');
        if let Some((_, cached)) = self.file_cache.iter_mut().find(|(n, _)| n == name) {
            *cached = data.to_vec();
            return Ok(());
        }
        Err("ext write: use write_file_root_optin(dev) — sem BlockDevice no trait")
    }

    fn list(&self, path: &str) -> Result<Vec<(String, bool)>, &'static str> {
        if !self.mounted {
            return Err("ext not mounted");
        }
        let p = path.trim_matches('/');
        if !p.is_empty() && p != "." {
            return Err("ext list: only root cached");
        }
        Ok(self
            .root_cache
            .iter()
            .map(|(n, d, _)| (n.clone(), *d))
            .collect())
    }

    fn free_space(&self) -> u64 {
        0
    }
    fn total_space(&self) -> u64 {
        0
    }
}
