//! B-tree CoW para NeuralFS — leaf items de 48 bytes + internos multi-nivel.
//! key(17) + value(31). insert/delete/lookup/scan/split (folha + interno).

use k_nano::block_dev::BlockDevice;

pub const LEAF_ITEM_SIZE: usize = 48;
pub const LEAF_HEADER: usize = 24;
pub const MAX_LEAF_ITEMS: usize = (4096 - LEAF_HEADER) / LEAF_ITEM_SIZE; // 84

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum ItemType {
    Inode = 0x01,
    DirEntry = 0x02,
    FileExtent = 0x03,
    FreeExtent = 0x04,
    Checksum = 0x05,
}

pub type ObjectId = u64;

#[derive(Debug, Clone, Copy)]
pub struct Key {
    pub object_id: ObjectId,
    pub item_type: ItemType,
    pub offset: u64,
}

impl Key {
    pub fn from_bytes(b: &[u8]) -> Self {
        Key {
            object_id: u64::from_le_bytes(b[0..8].try_into().unwrap_or([0; 8])),
            item_type: match b.get(8).copied().unwrap_or(0) {
                0x01 => ItemType::Inode,
                0x02 => ItemType::DirEntry,
                0x03 => ItemType::FileExtent,
                0x04 => ItemType::FreeExtent,
                _ => ItemType::Checksum,
            },
            offset: u64::from_le_bytes(b[9..17].try_into().unwrap_or([0; 8])),
        }
    }

    pub fn to_bytes(&self, b: &mut [u8]) {
        if b.len() < 17 {
            return;
        }
        b[0..8].copy_from_slice(&self.object_id.to_le_bytes());
        b[8] = self.item_type as u8;
        b[9..17].copy_from_slice(&self.offset.to_le_bytes());
    }

    pub fn cmp(&self, other: &Key) -> core::cmp::Ordering {
        match self.object_id.cmp(&other.object_id) {
            core::cmp::Ordering::Equal => match (self.item_type as u8).cmp(&(other.item_type as u8))
            {
                core::cmp::Ordering::Equal => self.offset.cmp(&other.offset),
                o => o,
            },
            o => o,
        }
    }
}

/// Payload auxiliar: 31 bytes apos a key.
#[derive(Debug, Clone, Copy)]
pub struct LeafValue {
    pub raw: [u8; 31],
}

impl LeafValue {
    pub fn zero() -> Self {
        LeafValue { raw: [0u8; 31] }
    }

    pub fn from_inode(mode: u16, size: u64, data_block: u64, block_count: u32) -> Self {
        let mut v = Self::zero();
        v.raw[0..2].copy_from_slice(&mode.to_le_bytes());
        v.raw[2..10].copy_from_slice(&size.to_le_bytes());
        v.raw[10..18].copy_from_slice(&data_block.to_le_bytes());
        v.raw[18..22].copy_from_slice(&block_count.to_le_bytes());
        v
    }

    pub fn as_inode(&self) -> (u16, u64, u64, u32) {
        let mode = u16::from_le_bytes([self.raw[0], self.raw[1]]);
        let size = u64::from_le_bytes(self.raw[2..10].try_into().unwrap_or([0; 8]));
        let data_block = u64::from_le_bytes(self.raw[10..18].try_into().unwrap_or([0; 8]));
        let block_count = u32::from_le_bytes(self.raw[18..22].try_into().unwrap_or([0; 4]));
        (mode, size, data_block, block_count)
    }

    pub fn from_dir(child_inode: u64, name: &str) -> Self {
        let mut v = Self::zero();
        v.raw[0..8].copy_from_slice(&child_inode.to_le_bytes());
        let nb = name.as_bytes();
        let n = nb.len().min(22);
        v.raw[8] = n as u8;
        v.raw[9..9 + n].copy_from_slice(&nb[..n]);
        v
    }

    pub fn as_dir(&self) -> (u64, alloc::string::String) {
        let ino = u64::from_le_bytes(self.raw[0..8].try_into().unwrap_or([0; 8]));
        let n = (self.raw[8] as usize).min(22);
        let name = core::str::from_utf8(&self.raw[9..9 + n])
            .map(|s| alloc::string::String::from(s))
            .unwrap_or_default();
        (ino, name)
    }

    pub fn from_extent(start: u64, count: u64) -> Self {
        let mut v = Self::zero();
        v.raw[0..8].copy_from_slice(&start.to_le_bytes());
        v.raw[8..16].copy_from_slice(&count.to_le_bytes());
        v
    }

    pub fn as_extent(&self) -> (u64, u64) {
        let start = u64::from_le_bytes(self.raw[0..8].try_into().unwrap_or([0; 8]));
        let count = u64::from_le_bytes(self.raw[8..16].try_into().unwrap_or([0; 8]));
        (start, count)
    }
}

pub struct BTreeNode {
    pub block_addr: u64,
    pub data: [u8; 4096],
}

impl BTreeNode {
    pub fn new(block_addr: u64) -> Self {
        let mut node = BTreeNode {
            block_addr,
            data: [0u8; 4096],
        };
        node.data[4] = 0; // leaf
        node.data[6..8].copy_from_slice(&0u16.to_le_bytes());
        node.data[8..16].copy_from_slice(&block_addr.to_le_bytes());
        node
    }

    pub fn level(&self) -> u8 {
        self.data[4]
    }
    pub fn item_count(&self) -> u16 {
        u16::from_le_bytes([self.data[6], self.data[7]])
    }
    pub fn set_item_count(&mut self, n: u16) {
        self.data[6..8].copy_from_slice(&n.to_le_bytes());
    }
    pub fn generation(&self) -> u64 {
        u64::from_le_bytes(self.data[16..24].try_into().unwrap_or([0; 8]))
    }
    pub fn set_generation(&mut self, gen: u64) {
        self.data[16..24].copy_from_slice(&gen.to_le_bytes());
    }

    pub fn compute_checksum(&self) -> u32 {
        crate::neural_fs::checksum::crc32c(&self.data[4..4096])
    }

    pub fn write_checksum(&mut self) {
        let crc = self.compute_checksum();
        self.data[0..4].copy_from_slice(&crc.to_le_bytes());
    }

    pub fn read(dev: &mut dyn BlockDevice, start_lba: u64, block_addr: u64) -> Option<Self> {
        let block_lba = block_addr.checked_mul(8)?;
        let mut node = BTreeNode {
            block_addr,
            data: [0u8; 4096],
        };
        for i in 0..8usize {
            let lba = start_lba.checked_add(block_lba.checked_add(i as u64)?)?;
            let off = i * 512;
            if !dev.read_sectors(lba, &mut node.data[off..off + 512]) {
                return None;
            }
        }
        if !crate::neural_fs::checksum::verify_block(&node.data) {
            return None;
        }
        Some(node)
    }

    pub fn write(&mut self, dev: &mut dyn BlockDevice, start_lba: u64) -> bool {
        self.data[8..16].copy_from_slice(&self.block_addr.to_le_bytes());
        self.write_checksum();
        for i in 0..8usize {
            let lba = start_lba + self.block_addr * 8 + i as u64;
            let off = i * 512;
            if !dev.write_sectors(lba, &self.data[off..off + 512]) {
                return false;
            }
        }
        true
    }

    fn item_off(idx: usize) -> Option<usize> {
        LEAF_HEADER.checked_add(idx.checked_mul(LEAF_ITEM_SIZE)?)
    }

    pub fn get_item(&self, idx: usize) -> Option<&[u8]> {
        let count = self.item_count() as usize;
        if idx >= count {
            return None;
        }
        let off = Self::item_off(idx)?;
        if off + LEAF_ITEM_SIZE > 4096 {
            return None;
        }
        Some(&self.data[off..off + LEAF_ITEM_SIZE])
    }

    pub fn get_key_value(&self, idx: usize) -> Option<(Key, LeafValue)> {
        let item = self.get_item(idx)?;
        let key = Key::from_bytes(item);
        let mut raw = [0u8; 31];
        raw.copy_from_slice(&item[17..48]);
        Some((key, LeafValue { raw }))
    }

    pub fn find_key(&self, key: &Key) -> Result<usize, usize> {
        let count = self.item_count() as usize;
        for i in 0..count {
            let item = match self.get_item(i) {
                Some(x) => x,
                None => return Err(i),
            };
            let k = Key::from_bytes(item);
            match k.cmp(key) {
                core::cmp::Ordering::Equal => return Ok(i),
                core::cmp::Ordering::Greater => return Err(i),
                _ => {}
            }
        }
        Err(count)
    }

    /// Insere (key,value) ordenado. Retorna false se cheio ou key duplicada.
    pub fn insert(&mut self, key: &Key, value: &LeafValue) -> bool {
        let count = self.item_count() as usize;
        if count >= MAX_LEAF_ITEMS {
            return false;
        }
        let pos = match self.find_key(key) {
            Ok(_) => return false, // duplicate
            Err(i) => i,
        };
        // shift right
        if pos < count {
            let src = match Self::item_off(pos) {
                Some(s) => s,
                None => { k_nano::slog_bin!("BTREE", "error", "item_off overflow in insert shift"); return false; }
            };
            let dst = match Self::item_off(pos + 1) {
                Some(d) => d,
                None => { k_nano::slog_bin!("BTREE", "error", "item_off overflow in insert shift+1"); return false; }
            };
            let bytes = (count - pos) * LEAF_ITEM_SIZE;
            self.data.copy_within(src..src + bytes, dst);
        }
        let off = match Self::item_off(pos) {
            Some(o) => o,
            None => { k_nano::slog_bin!("BTREE", "error", "item_off overflow in insert off"); return false; }
        };
        key.to_bytes(&mut self.data[off..off + 17]);
        self.data[off + 17..off + 48].copy_from_slice(&value.raw);
        self.set_item_count((count + 1) as u16);
        true
    }

    pub fn delete_at(&mut self, idx: usize) -> bool {
        let count = self.item_count() as usize;
        if idx >= count {
            return false;
        }
        if idx + 1 < count {
            let src = match Self::item_off(idx + 1) {
                Some(s) => s,
                None => { k_nano::slog_bin!("BTREE", "error", "item_off overflow in delete_at shift"); return false; }
            };
            let dst = match Self::item_off(idx) {
                Some(d) => d,
                None => { k_nano::slog_bin!("BTREE", "error", "item_off overflow in delete_at shift dst"); return false; }
            };
            let bytes = (count - idx - 1) * LEAF_ITEM_SIZE;
            self.data.copy_within(src..src + bytes, dst);
        }
        // clear last slot
        if let Some(last) = Self::item_off(count - 1) {
            self.data[last..last + LEAF_ITEM_SIZE].fill(0);
        }
        self.set_item_count((count - 1) as u16);
        true
    }

    pub fn delete_key(&mut self, key: &Key) -> bool {
        match self.find_key(key) {
            Ok(i) => self.delete_at(i),
            Err(_) => false,
        }
    }

    pub fn update_at(&mut self, idx: usize, value: &LeafValue) -> bool {
        let count = self.item_count() as usize;
        if idx >= count {
            return false;
        }
        let Some(off) = Self::item_off(idx) else {
            return false;
        };
        self.data[off + 17..off + 48].copy_from_slice(&value.raw);
        true
    }

    pub fn set_level(&mut self, level: u8) {
        self.data[4] = level;
    }

    pub fn leftmost_child(&self) -> u64 {
        u64::from_le_bytes(self.data[4088..4096].try_into().unwrap_or([0; 8]))
    }

    pub fn set_leftmost_child(&mut self, block: u64) {
        self.data[4088..4096].copy_from_slice(&block.to_le_bytes());
    }

    /// Filho para keys < keys[idx]; apos ultimo item = rightmost via leftmost+items.
    pub fn child_for_key(&self, key: &Key) -> u64 {
        if self.level() == 0 {
            return self.block_addr;
        }
        let count = self.item_count() as usize;
        for i in 0..count {
            if let Some((k, v)) = self.get_key_value(i) {
                if key.cmp(&k) == core::cmp::Ordering::Less {
                    return if i == 0 {
                        self.leftmost_child()
                    } else {
                        // child is in previous item value
                        self.get_key_value(i - 1)
                            .map(|(_, pv)| u64::from_le_bytes(pv.raw[0..8].try_into().unwrap_or([0; 8])))
                            .unwrap_or(0)
                    };
                }
                let _ = v;
            }
        }
        // >= all keys → rightmost child (last item's child ptr)
        if count == 0 {
            return self.leftmost_child();
        }
        self.get_key_value(count - 1)
            .map(|(_, v)| u64::from_le_bytes(v.raw[0..8].try_into().unwrap_or([0; 8])))
            .unwrap_or(0)
    }

    pub fn from_child_ptr(child: u64) -> LeafValue {
        let mut v = LeafValue::zero();
        v.raw[0..8].copy_from_slice(&child.to_le_bytes());
        v
    }

    /// Divide folha cheia ao meio. `right` vazio; move metade superior (incl. sep) para `right`.
    /// Retorna a key separadora (primeira de `right`) — copia sobe ao pai.
    pub fn split_into(&mut self, right: &mut BTreeNode) -> Option<Key> {
        let count = self.item_count() as usize;
        if count < 2 {
            return None;
        }
        let mid = count / 2;
        let sep = self.get_key_value(mid)?.0;
        right.set_level(0);
        for i in mid..count {
            let (k, v) = self.get_key_value(i)?;
            right.insert(&k, &v);
        }
        for i in (mid..count).rev() {
            self.delete_at(i);
        }
        Some(sep)
    }

    /// Divide no interno: promove a key do meio (remove dos dois lados).
    /// `right.leftmost_child` = child ptr da key promovida.
    pub fn split_internal_into(&mut self, right: &mut BTreeNode) -> Option<Key> {
        let count = self.item_count() as usize;
        if count < 2 {
            return None;
        }
        let mid = count / 2;
        let (sep_key, sep_val) = self.get_key_value(mid)?;
        let sep_child = u64::from_le_bytes(sep_val.raw[0..8].try_into().unwrap_or([0; 8]));
        right.set_level(self.level());
        right.set_leftmost_child(sep_child);
        for i in (mid + 1)..count {
            let (k, v) = self.get_key_value(i)?;
            right.insert(&k, &v);
        }
        for i in (mid..count).rev() {
            self.delete_at(i);
        }
        Some(sep_key)
    }

    /// Troca ponteiro de filho `old` → `new` (leftmost ou item).
    pub fn replace_child_ptr(&mut self, old: u64, new: u64) -> bool {
        let mut found = false;
        if self.leftmost_child() == old {
            self.set_leftmost_child(new);
            found = true;
        }
        for i in 0..self.item_count() as usize {
            if let Some((_, v)) = self.get_key_value(i) {
                let child = u64::from_le_bytes(v.raw[0..8].try_into().unwrap_or([0; 8]));
                if child == old {
                    self.update_at(i, &Self::from_child_ptr(new));
                    found = true;
                }
            }
        }
        found
    }
}

/// Lookup valor por key — caminha nos internos (level>0) ate a folha.
pub fn btree_lookup(
    dev: &mut dyn BlockDevice,
    start_lba: u64,
    root: u64,
    key: &Key,
) -> Option<LeafValue> {
    let mut addr = root;
    for _ in 0..8 {
        let node = BTreeNode::read(dev, start_lba, addr)?;
        if node.level() == 0 {
            return match node.find_key(key) {
                Ok(i) => node.get_key_value(i).map(|(_, v)| v),
                Err(_) => None,
            };
        }
        let next = node.child_for_key(key);
        if next == 0 {
            return None;
        }
        addr = next;
    }
    None
}

/// Carrega a folha que contem `key` (para mutacao).
pub fn btree_find_leaf(
    dev: &mut dyn BlockDevice,
    start_lba: u64,
    root: u64,
    key: &Key,
) -> Option<BTreeNode> {
    let mut addr = root;
    for _ in 0..8 {
        let node = BTreeNode::read(dev, start_lba, addr)?;
        if node.level() == 0 {
            return Some(node);
        }
        let next = node.child_for_key(key);
        if next == 0 {
            return None;
        }
        addr = next;
    }
    None
}

/// Lista todos os items de todas as folhas (DFS).
pub fn btree_scan_leaves<F>(
    dev: &mut dyn BlockDevice,
    start_lba: u64,
    root: u64,
    mut f: F,
) -> bool
where
    F: FnMut(&Key, &LeafValue),
{
    fn walk<F>(
        dev: &mut dyn BlockDevice,
        start_lba: u64,
        addr: u64,
        depth: u8,
        f: &mut F,
    ) -> bool
    where
        F: FnMut(&Key, &LeafValue),
    {
        if depth > 8 {
            return false;
        }
        let Some(node) = BTreeNode::read(dev, start_lba, addr) else {
            return false;
        };
        if node.level() == 0 {
            for i in 0..node.item_count() as usize {
                if let Some((k, v)) = node.get_key_value(i) {
                    f(&k, &v);
                }
            }
            return true;
        }
        let left = node.leftmost_child();
        if left != 0 && !walk(dev, start_lba, left, depth + 1, f) {
            return false;
        }
        for i in 0..node.item_count() as usize {
            if let Some((_, v)) = node.get_key_value(i) {
                let child = u64::from_le_bytes(v.raw[0..8].try_into().unwrap_or([0; 8]));
                if child != 0 && !walk(dev, start_lba, child, depth + 1, f) {
                    return false;
                }
            }
        }
        true
    }
    walk(dev, start_lba, root, 0, &mut f)
}








