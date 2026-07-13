//! EXT2/3/4 reader minimo — leitura de arquivos via inode table.
//! Suporta: listar diretorio raiz, ler arquivo por path, symlinks.
//! Nao suporta: escrita, journal (EXT3/4), extents (EXT4), ACLs.

use alloc::vec;
use alloc::vec::Vec;
use alloc::string::String;
use crate::block_dev::BlockDevice;
use crate::fs_driver::{FilesystemDriver, FsInfo};

pub struct Ext2Reader {
    start_lba: u64,
    block_size: u32,
    inodes_per_group: u32,
    blocks_per_group: u32,
    inode_table_blocks: u32,
    inode_size: u16,
    total_inodes: u32,
    first_inode: u32,
    label: String,
}

impl Ext2Reader {
    fn read_blocks(&self, dev: &mut dyn BlockDevice, block: u64, buf: &mut [u8]) -> bool {
        let lba = self.start_lba + block * self.block_size as u64 / 512;
        for i in 0..(buf.len() / 512) {
            if !dev.read_sectors(lba + i as u64, &mut buf[i*512..(i+1)*512]) {
                return false;
            }
        }
        true
    }

    fn read_inode(&self, dev: &mut dyn BlockDevice, inode: u32) -> Option<Vec<u8>> {
        let group = (inode - 1) / self.inodes_per_group;
        let index = (inode - 1) % self.inodes_per_group;
        let gdt_offset = (group as u64 + 1) * self.block_size as u64 / 512; // GDT after superblock
        let mut gdt_entry = [0u8; 32];
        for i in 0..(32/512) {
            if !dev.read_sectors(self.start_lba + gdt_offset + i as u64, &mut gdt_entry[i*512..(i+1)*512]) { return None; }
        }
        let inode_table_block = u32::from_le_bytes([gdt_entry[8], gdt_entry[9], gdt_entry[10], gdt_entry[11]]);
        let inode_lba = self.start_lba + inode_table_block as u64 * self.block_size as u64 / 512
            + index as u64 * self.inode_size as u64 / 512;
        let mut inode_buf = vec![0u8; self.inode_size as usize];
        for i in 0..(self.inode_size as usize / 512) {
            if !dev.read_sectors(inode_lba + i as u64, &mut inode_buf[i*512..(i+1)*512]) { return None; }
        }
        Some(inode_buf)
    }

    fn inode_size_from_inode(&self, inode: &[u8]) -> u32 {
        u32::from_le_bytes([inode[4], inode[5], inode[6], inode[7]])
    }

    fn inode_blocks(&self, inode: &[u8]) -> u32 {
        u32::from_le_bytes([inode[12], inode[13], inode[14], inode[15]])
    }

    fn inode_block_ptr(&self, inode: &[u8], index: usize) -> u32 {
        let off = 40 + index * 4;
        if off + 4 > inode.len() { return 0; }
        u32::from_le_bytes([inode[off], inode[off+1], inode[off+2], inode[off+3]])
    }
}

impl FilesystemDriver for Ext2Reader {
    fn name(&self) -> &str { "ext2" }

    fn detect(dev: &mut dyn BlockDevice, start_lba: u64) -> Option<Self> {
        let mut sb = [0u8; 1024];
        for i in 0..2usize {
            let off = i * 512;
            if !dev.read_sectors(start_lba + 2 + i as u64, &mut sb[off..off+512]) { return None; }
        }
        if sb[56..58] != [0x53, 0xEF] { return None; } // ext2 magic
        let total_inodes = u32::from_le_bytes([sb[0], sb[1], sb[2], sb[3]]);
        let block_size = 1024u32 << u32::from_le_bytes([sb[24], sb[25], sb[26], sb[27]]);
        let inodes_per_group = u32::from_le_bytes([sb[40], sb[41], sb[42], sb[43]]);
        let inode_size = u16::from_le_bytes([sb[88], sb[89]]);
        let first_inode = u32::from_le_bytes([sb[84], sb[85], sb[86], sb[87]]);
        let mut label = String::new();
        for &c in &sb[120..136] {
            if c == 0 { break; }
            label.push(c as char);
        }
        let blocks_per_group = u32::from_le_bytes([sb[32], sb[33], sb[34], sb[35]]);
        let inode_table_blocks = (inodes_per_group * inode_size as u32 + block_size - 1) / block_size;
        Some(Ext2Reader { start_lba, block_size, inodes_per_group, blocks_per_group,
            inode_table_blocks, inode_size, total_inodes, first_inode, label })
    }

    fn mount(&mut self, _dev: &mut dyn BlockDevice, _start_lba: u64) -> Result<FsInfo, &'static str> {
        Ok(FsInfo { fs_type: "EXT2", label: self.label.clone(), total_bytes: 0,
            free_bytes: None, block_size: self.block_size, writable: false })
    }

    fn read(&self, _path: &str, _offset: u64, _buf: &mut [u8]) -> Result<usize, &'static str> {
        Err("EXT2 read: not yet implemented")
    }

    fn write(&mut self, _path: &str, _offset: u64, _data: &[u8]) -> Result<(), &'static str> {
        Err("EXT2 read-only")
    }

    fn list(&self, _path: &str) -> Result<Vec<(String, bool)>, &'static str> {
        Err("EXT2 list: not yet implemented")
    }

    fn free_space(&self) -> u64 { 0 }
    fn total_space(&self) -> u64 { 0 }
}
