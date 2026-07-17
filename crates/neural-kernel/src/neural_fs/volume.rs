//! Volume NeuralFS — format/mount + file API + reclaim + split 2-niveis.

use alloc::string::String;
use alloc::vec::Vec;
use crate::block_dev::BlockDevice;
use super::superblock::Superblock;
use super::btree::{
    btree_find_leaf, btree_lookup, btree_scan_leaves, BTreeNode, ItemType, Key, LeafValue,
    MAX_LEAF_ITEMS,
};
use super::dir::DirEntry;
use super::inode::Inode;
use super::journal::Journal;

/// Tipo MBR reservado para NeuralFS (nao conflita com FAT 0x0B/0C/1C).
pub const MBR_TYPE_NEURALFS: u8 = 0x7F;
const FREE_LIST_MAGIC: &[u8; 8] = b"NRFSFREE";
const FREE_LIST_MAX: usize = (4096 - 16) / 8; // 510 entries

pub struct MemoryDisk {
    pub data: Vec<u8>,
}

impl MemoryDisk {
    pub fn new(size_bytes: usize) -> Self {
        MemoryDisk {
            data: alloc::vec![0u8; size_bytes],
        }
    }

    pub fn sector_count(&self) -> u64 {
        (self.data.len() / 512) as u64
    }
}

impl BlockDevice for MemoryDisk {
    fn read_sectors(&mut self, lba: u64, buf: &mut [u8]) -> bool {
        let start = lba as usize * 512;
        if start + buf.len() > self.data.len() || buf.len() % 512 != 0 {
            return false;
        }
        buf.copy_from_slice(&self.data[start..start + buf.len()]);
        true
    }
    fn write_sectors(&mut self, lba: u64, buf: &[u8]) -> bool {
        let start = lba as usize * 512;
        if start + buf.len() > self.data.len() || buf.len() % 512 != 0 {
            return false;
        }
        self.data[start..start + buf.len()].copy_from_slice(buf);
        true
    }
    fn total_sectors(&self) -> u64 {
        self.sector_count()
    }
}

pub struct NeuralVolume {
    pub sb: Superblock,
    pub start_lba: u64,
    pub tx_id: u64,
    journal: Journal,
    pub dirty: bool,
    /// Blocos livres reclaimados (LIFO), persistidos em free_extent_root.
    free_stack: Vec<u64>,
}

impl NeuralVolume {
    pub fn format(dev: &mut dyn BlockDevice, start_lba: u64, total_lba: u64) -> bool {
        let total_blocks = total_lba / 8;
        if total_blocks < 64 {
            return false;
        }
        let journal_blocks = (total_blocks / 100).max(8).min(256);
        // layout: 0 unused, 1 sb, 2 sb-backup, 3.. journal, free_list, leaf root
        let free_list = 3 + journal_blocks;
        let leaf_addr = free_list + 1;
        if leaf_addr + 8 >= total_blocks {
            return false;
        }

        let sb = Superblock {
            magic: *b"NEURALFS",
            version: 1,
            total_blocks,
            free_blocks: total_blocks - leaf_addr - 1,
            allocated_inodes: 1,
            last_tx_id: 0,
            root_inode: 1,
            inode_tree_root: leaf_addr,
            free_extent_root: free_list,
            checksum_tree_root: 0,
            journal_start: 3,
            journal_blocks,
            uuid: [0x4E555241, 0x4C46435F],
            label: [0; 4],
            next_cow_block: leaf_addr + 1,
        };
        if !sb.write(dev, start_lba) {
            return false;
        }
        // free list vazia
        let mut fl = [0u8; 4096];
        fl[0..8].copy_from_slice(FREE_LIST_MAGIC);
        if !Self::write_block_static(dev, start_lba, free_list, &fl) {
            return false;
        }
        let mut leaf = BTreeNode::new(leaf_addr);
        leaf.set_generation(1);
        let root_val = LeafValue::from_inode(Inode::S_IFDIR | 0o755, 0, 0, 0);
        if !leaf.insert(&Inode::make_key(1), &root_val) {
            return false;
        }
        leaf.write(dev, start_lba)
    }

    fn write_block_static(
        dev: &mut dyn BlockDevice,
        start_lba: u64,
        block: u64,
        data: &[u8; 4096],
    ) -> bool {
        for i in 0..8usize {
            let lba = start_lba + block * 8 + i as u64;
            if !dev.write_sectors(lba, &data[i * 512..(i + 1) * 512]) {
                return false;
            }
        }
        true
    }

    fn read_block_static(
        dev: &mut dyn BlockDevice,
        start_lba: u64,
        block: u64,
        data: &mut [u8; 4096],
    ) -> bool {
        for i in 0..8usize {
            let lba = start_lba + block * 8 + i as u64;
            if !dev.read_sectors(lba, &mut data[i * 512..(i + 1) * 512]) {
                return false;
            }
        }
        true
    }

    pub fn mount(dev: &mut dyn BlockDevice, start_lba: u64) -> Option<Self> {
        let sb = Superblock::read(dev, start_lba)?;
        if &sb.magic != b"NEURALFS" {
            return None;
        }
        let recover_ok = Journal::recover(dev, start_lba, &sb);
        let mut vol = NeuralVolume {
            sb,
            start_lba,
            tx_id: 0,
            journal: Journal::new(),
            dirty: !recover_ok,
            free_stack: Vec::new(),
        };
        vol.load_free_list(dev);
        Some(vol)
    }

    fn load_free_list(&mut self, dev: &mut dyn BlockDevice) {
        self.free_stack.clear();
        let addr = self.sb.free_extent_root;
        if addr == 0 {
            return;
        }
        let mut page = [0u8; 4096];
        if !Self::read_block_static(dev, self.start_lba, addr, &mut page) {
            return;
        }
        if &page[0..8] != FREE_LIST_MAGIC {
            return;
        }
        let count = u32::from_le_bytes(page[8..12].try_into().unwrap_or([0; 4])) as usize;
        let count = count.min(FREE_LIST_MAX);
        for i in 0..count {
            let off = 16 + i * 8;
            let b = u64::from_le_bytes(page[off..off + 8].try_into().unwrap_or([0; 8]));
            if b > 0 && b < self.sb.total_blocks {
                self.free_stack.push(b);
            }
        }
    }

    fn persist_free_list(&self, dev: &mut dyn BlockDevice) -> bool {
        let addr = self.sb.free_extent_root;
        if addr == 0 {
            return true;
        }
        let mut page = [0u8; 4096];
        page[0..8].copy_from_slice(FREE_LIST_MAGIC);
        let n = self.free_stack.len().min(FREE_LIST_MAX);
        page[8..12].copy_from_slice(&(n as u32).to_le_bytes());
        // load empurra 0..n; pop = ultimo → top LIFO fica no fim do array
        for (i, &b) in self.free_stack.iter().rev().take(n).rev().enumerate() {
            let off = 16 + i * 8;
            page[off..off + 8].copy_from_slice(&b.to_le_bytes());
        }
        Self::write_block_static(dev, self.start_lba, addr, &page)
    }

    pub fn next_block(&mut self) -> Option<u64> {
        if let Some(b) = self.free_stack.pop() {
            return Some(b);
        }
        if self.sb.free_blocks == 0 || self.sb.next_cow_block >= self.sb.total_blocks {
            return None;
        }
        let b = self.sb.next_cow_block;
        self.sb.next_cow_block += 1;
        self.sb.free_blocks = self.sb.free_blocks.saturating_sub(1);
        Some(b)
    }

    fn reclaim_block(&mut self, block: u64) {
        if block == 0 || block >= self.sb.total_blocks {
            return;
        }
        if self.free_stack.len() >= FREE_LIST_MAX {
            return; // deixa vazar ate o bump (melhor que corromper)
        }
        self.free_stack.push(block);
        self.sb.free_blocks = self.sb.free_blocks.saturating_add(1);
    }

    fn begin_tx(&mut self) {
        self.tx_id += 1;
        self.journal.begin_tx(self.tx_id);
    }

    fn commit_tx(&mut self, dev: &mut dyn BlockDevice) -> bool {
        if !self.persist_free_list(dev) {
            return false;
        }
        if !self.journal.commit(dev, self.start_lba, &self.sb) {
            return false;
        }
        self.sb.last_tx_id = self.tx_id;
        self.sb.write(dev, self.start_lba)
    }

    fn write_block_raw(&self, dev: &mut dyn BlockDevice, block: u64, data: &[u8; 4096]) -> bool {
        Self::write_block_static(dev, self.start_lba, block, data)
    }

    fn read_block_raw(
        &self,
        dev: &mut dyn BlockDevice,
        block: u64,
        data: &mut [u8; 4096],
    ) -> bool {
        Self::read_block_static(dev, self.start_lba, block, data)
    }

    /// CoW da folha que contem `hint_key`; reclaim do bloco antigo.
    fn cow_leaf_for_key(
        &mut self,
        dev: &mut dyn BlockDevice,
        hint_key: &Key,
    ) -> Option<BTreeNode> {
        let old = btree_find_leaf(dev, self.start_lba, self.sb.inode_tree_root, hint_key)?;
        let old_addr = old.block_addr;
        let new_addr = self.next_block()?;
        let mut node = old;
        node.block_addr = new_addr;
        node.set_generation(self.tx_id.max(1));

        let root = BTreeNode::read(dev, self.start_lba, self.sb.inode_tree_root)?;
        if root.level() == 0 {
            self.sb.inode_tree_root = new_addr;
        } else {
            let mut parent = root;
            let parent_old = parent.block_addr;
            let parent_new = self.next_block()?;
            parent.block_addr = parent_new;
            if parent.leftmost_child() == old_addr {
                parent.set_leftmost_child(new_addr);
            }
            for i in 0..parent.item_count() as usize {
                if let Some((_, v)) = parent.get_key_value(i) {
                    let child = u64::from_le_bytes(v.raw[0..8].try_into().unwrap_or([0; 8]));
                    if child == old_addr {
                        parent.update_at(i, &BTreeNode::from_child_ptr(new_addr));
                    }
                }
            }
            parent.set_generation(self.tx_id.max(1));
            self.journal.log_block(parent_new, &parent.data);
            if !parent.write(dev, self.start_lba) {
                return None;
            }
            self.sb.inode_tree_root = parent_new;
            self.reclaim_block(parent_old);
        }
        self.reclaim_block(old_addr);
        Some(node)
    }

    fn leaf_insert(
        &mut self,
        dev: &mut dyn BlockDevice,
        key: &Key,
        val: &LeafValue,
    ) -> Result<(), &'static str> {
        let mut leaf = self.cow_leaf_for_key(dev, key).ok_or("cow leaf")?;
        if leaf.insert(key, val) {
            self.journal.log_block(leaf.block_addr, &leaf.data);
            if !leaf.write(dev, self.start_lba) {
                return Err("leaf write");
            }
            if BTreeNode::read(dev, self.start_lba, self.sb.inode_tree_root)
                .map(|r| r.level() == 0)
                .unwrap_or(false)
            {
                self.sb.inode_tree_root = leaf.block_addr;
            }
            return Ok(());
        }
        if leaf.item_count() as usize >= MAX_LEAF_ITEMS {
            self.split_and_insert(dev, leaf, key, val)
        } else {
            Err("insert failed")
        }
    }

    fn split_and_insert(
        &mut self,
        dev: &mut dyn BlockDevice,
        mut left: BTreeNode,
        key: &Key,
        val: &LeafValue,
    ) -> Result<(), &'static str> {
        let right_addr = self.next_block().ok_or("no space")?;
        let mut right = BTreeNode::new(right_addr);
        let sep = left.split_into(&mut right).ok_or("split")?;
        if key.cmp(&sep) == core::cmp::Ordering::Less {
            if !left.insert(key, val) {
                return Err("insert left");
            }
        } else if !right.insert(key, val) {
            return Err("insert right");
        }
        left.set_generation(self.tx_id.max(1));
        right.set_generation(self.tx_id.max(1));
        self.journal.log_block(left.block_addr, &left.data);
        self.journal.log_block(right.block_addr, &right.data);
        if !left.write(dev, self.start_lba) || !right.write(dev, self.start_lba) {
            return Err("split write");
        }

        let root = BTreeNode::read(dev, self.start_lba, self.sb.inode_tree_root)
            .ok_or("root read")?;
        if root.level() == 0 {
            let root_addr = self.next_block().ok_or("no space root")?;
            let mut inode = BTreeNode::new(root_addr);
            inode.set_level(1);
            inode.set_leftmost_child(left.block_addr);
            let sep_val = BTreeNode::from_child_ptr(right.block_addr);
            if !inode.insert(&sep, &sep_val) {
                return Err("root insert");
            }
            inode.set_generation(self.tx_id.max(1));
            self.journal.log_block(root_addr, &inode.data);
            if !inode.write(dev, self.start_lba) {
                return Err("root write");
            }
            if root.block_addr != left.block_addr {
                self.reclaim_block(root.block_addr);
            }
            self.sb.inode_tree_root = root_addr;
        } else {
            let mut parent = BTreeNode::read(dev, self.start_lba, self.sb.inode_tree_root)
                .ok_or("parent")?;
            let old_p = parent.block_addr;
            let new_p = self.next_block().ok_or("no space")?;
            parent.block_addr = new_p;
            let sep_val = BTreeNode::from_child_ptr(right.block_addr);
            if !parent.insert(&sep, &sep_val) {
                return Err("parent full");
            }
            parent.set_generation(self.tx_id.max(1));
            self.journal.log_block(new_p, &parent.data);
            if !parent.write(dev, self.start_lba) {
                return Err("parent write");
            }
            self.sb.inode_tree_root = new_p;
            self.reclaim_block(old_p);
        }
        Ok(())
    }

    pub fn lookup_inode(
        &self,
        dev: &mut dyn BlockDevice,
        ino: u64,
    ) -> Option<(u16, u64, u64, u32)> {
        let v = btree_lookup(
            dev,
            self.start_lba,
            self.sb.inode_tree_root,
            &Inode::make_key(ino),
        )?;
        Some(v.as_inode())
    }

    pub fn lookup_dir_entry(
        &self,
        dev: &mut dyn BlockDevice,
        parent: u64,
        name: &str,
    ) -> Option<u64> {
        let key = DirEntry::make_key(parent, name);
        let v = btree_lookup(dev, self.start_lba, self.sb.inode_tree_root, &key)?;
        Some(v.as_dir().0)
    }

    pub fn resolve_path(&self, dev: &mut dyn BlockDevice, path: &str) -> Option<u64> {
        let path = path.trim_matches('/');
        if path.is_empty() {
            return Some(self.sb.root_inode);
        }
        let mut cur = self.sb.root_inode;
        for part in path.split('/') {
            if part.is_empty() {
                continue;
            }
            cur = self.lookup_dir_entry(dev, cur, part)?;
        }
        Some(cur)
    }

    pub fn list_dir(
        &self,
        dev: &mut dyn BlockDevice,
        dir_ino: u64,
    ) -> Result<Vec<String>, &'static str> {
        let mut out = Vec::new();
        let root = self.sb.inode_tree_root;
        let start = self.start_lba;
        let ok = btree_scan_leaves(dev, start, root, |k, v| {
            if k.object_id == dir_ino && k.item_type == ItemType::DirEntry {
                let (_ino, name) = v.as_dir();
                if !name.is_empty() {
                    out.push(name);
                }
            }
        });
        if !ok {
            return Err("scan failed");
        }
        Ok(out)
    }

    pub fn create_file(
        &mut self,
        dev: &mut dyn BlockDevice,
        parent: u64,
        name: &str,
    ) -> Result<u64, &'static str> {
        if name.is_empty() || name.len() > 22 {
            return Err("bad name");
        }
        if self.lookup_dir_entry(dev, parent, name).is_some() {
            return Err("exists");
        }
        let (mode, _, _, _) = self.lookup_inode(dev, parent).ok_or("parent missing")?;
        if mode & Inode::S_IFDIR == 0 {
            return Err("not a dir");
        }
        self.begin_tx();
        let ino = self.sb.allocated_inodes + 1;
        self.leaf_insert(
            dev,
            &Inode::make_key(ino),
            &LeafValue::from_inode(Inode::S_IFREG | 0o644, 0, 0, 0),
        )?;
        self.leaf_insert(
            dev,
            &DirEntry::make_key(parent, name),
            &LeafValue::from_dir(ino, name),
        )?;
        self.sb.allocated_inodes = ino;
        if !self.commit_tx(dev) {
            return Err("commit");
        }
        Ok(ino)
    }

    pub fn create_dir(
        &mut self,
        dev: &mut dyn BlockDevice,
        parent: u64,
        name: &str,
    ) -> Result<u64, &'static str> {
        if name.is_empty() || name.len() > 22 {
            return Err("bad name");
        }
        if self.lookup_dir_entry(dev, parent, name).is_some() {
            return Err("exists");
        }
        self.begin_tx();
        let ino = self.sb.allocated_inodes + 1;
        self.leaf_insert(
            dev,
            &Inode::make_key(ino),
            &LeafValue::from_inode(Inode::S_IFDIR | 0o755, 0, 0, 0),
        )?;
        self.leaf_insert(
            dev,
            &DirEntry::make_key(parent, name),
            &LeafValue::from_dir(ino, name),
        )?;
        self.sb.allocated_inodes = ino;
        if !self.commit_tx(dev) {
            return Err("commit");
        }
        Ok(ino)
    }

    pub fn write_file(
        &mut self,
        dev: &mut dyn BlockDevice,
        ino: u64,
        data: &[u8],
    ) -> Result<(), &'static str> {
        let (mode, _, old_block, old_count) =
            self.lookup_inode(dev, ino).ok_or("no inode")?;
        if mode & Inode::S_IFREG == 0 {
            return Err("not a file");
        }
        self.begin_tx();
        if old_count > 0 && old_block > 0 {
            for i in 0..old_count as u64 {
                self.reclaim_block(old_block + i);
            }
        }

        let blocks_needed = (data.len() + 4095) / 4096;
        let mut first_block = 0u64;
        let mut block_count = 0u32;
        for bi in 0..blocks_needed {
            let b = self.next_block().ok_or("no space")?;
            if bi == 0 {
                first_block = b;
            }
            let mut page = [0u8; 4096];
            let start = bi * 4096;
            let end = (start + 4096).min(data.len());
            page[..end - start].copy_from_slice(&data[start..end]);
            if !self.write_block_raw(dev, b, &page) {
                return Err("data write");
            }
            block_count += 1;
        }

        let inode_key = Inode::make_key(ino);
        let extent_key = Key {
            object_id: ino,
            item_type: ItemType::FileExtent,
            offset: 0,
        };
        let mut leaf = self.cow_leaf_for_key(dev, &inode_key).ok_or("cow")?;
        leaf.delete_key(&inode_key);
        leaf.delete_key(&extent_key);
        let new_val = LeafValue::from_inode(mode, data.len() as u64, first_block, block_count);
        if !leaf.insert(&inode_key, &new_val) {
            self.journal.log_block(leaf.block_addr, &leaf.data);
            let _ = leaf.write(dev, self.start_lba);
            self.leaf_insert(dev, &inode_key, &new_val)?;
            if blocks_needed > 0 {
                self.leaf_insert(
                    dev,
                    &extent_key,
                    &LeafValue::from_extent(first_block, block_count as u64),
                )?;
            }
        } else {
            if blocks_needed > 0 {
                let _ = leaf.insert(
                    &extent_key,
                    &LeafValue::from_extent(first_block, block_count as u64),
                );
            }
            self.journal.log_block(leaf.block_addr, &leaf.data);
            if !leaf.write(dev, self.start_lba) {
                return Err("leaf write");
            }
            if BTreeNode::read(dev, self.start_lba, self.sb.inode_tree_root)
                .map(|r| r.level() == 0)
                .unwrap_or(false)
            {
                self.sb.inode_tree_root = leaf.block_addr;
            }
        }
        if !self.commit_tx(dev) {
            return Err("commit");
        }
        Ok(())
    }

    pub fn read_file(
        &self,
        dev: &mut dyn BlockDevice,
        ino: u64,
    ) -> Result<Vec<u8>, &'static str> {
        let (mode, size, data_block, block_count) =
            self.lookup_inode(dev, ino).ok_or("no inode")?;
        if mode & Inode::S_IFREG == 0 {
            return Err("not a file");
        }
        if size == 0 || block_count == 0 {
            return Ok(Vec::new());
        }
        let (start, count) = if let Some(v) = btree_lookup(
            dev,
            self.start_lba,
            self.sb.inode_tree_root,
            &Key {
                object_id: ino,
                item_type: ItemType::FileExtent,
                offset: 0,
            },
        ) {
            v.as_extent()
        } else {
            (data_block, block_count as u64)
        };
        let mut out = Vec::with_capacity(size as usize);
        for bi in 0..count {
            let mut page = [0u8; 4096];
            if !self.read_block_raw(dev, start + bi, &mut page) {
                return Err("data read");
            }
            let remain = size as usize - out.len();
            let n = remain.min(4096);
            out.extend_from_slice(&page[..n]);
            if out.len() >= size as usize {
                break;
            }
        }
        out.truncate(size as usize);
        Ok(out)
    }

    /// Detecta magic NEURALFS no superbloco primario (bloco 1).
    pub fn probe_magic(dev: &mut dyn BlockDevice, start_lba: u64) -> bool {
        let mut sector = [0u8; 512];
        if !dev.read_sectors(start_lba + 8, &mut sector) {
            return false;
        }
        &sector[0..8] == b"NEURALFS"
    }
}
