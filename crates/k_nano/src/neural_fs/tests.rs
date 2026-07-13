//! Testes do NeuralFS com MemoryDisk (disco em RAM, sem hardware).

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;
    use crate::block_dev::BlockDevice;
    use crate::neural_fs::checksum;
    use crate::neural_fs::superblock::Superblock;
    use crate::neural_fs::btree::{BTreeNode, Key, ItemType};
    use crate::neural_fs::inode::Inode;
    use crate::neural_fs::dir::DirEntry;
    use crate::neural_fs::extent::Extent;
    use crate::neural_fs::journal::Journal;
    use crate::neural_fs::volume::NeuralVolume;

    struct MemoryDisk {
        data: Vec<u8>,
    }

    impl MemoryDisk {
        fn new(size: usize) -> Self {
            MemoryDisk { data: vec![0u8; size] }
        }
    }

    impl BlockDevice for MemoryDisk {
        fn read_sectors(&mut self, lba: u64, buf: &mut [u8]) -> bool {
            let start = lba as usize * 512;
            if start + buf.len() > self.data.len() { return false; }
            buf.copy_from_slice(&self.data[start..start + buf.len()]);
            true
        }
        fn write_sectors(&mut self, lba: u64, buf: &[u8]) -> bool {
            let start = lba as usize * 512;
            if start + buf.len() > self.data.len() { return false; }
            self.data[start..start + buf.len()].copy_from_slice(buf);
            true
        }
    }

    #[test]
    fn test_checksum_crc32c() {
        let data = b"Hello, NeuralFS!";
        let crc = checksum::crc32c(data);
        assert_ne!(crc, 0);
        assert_eq!(checksum::crc32c(data), crc); // deterministico
    }

    #[test]
    fn test_checksum_block() {
        let mut block = [0u8; 4096];
        block[4..16].copy_from_slice(b"NEURALFS TEST");
        let crc = checksum::crc32c_block(&block);
        let mut block_with_crc = block;
        block_with_crc[0..4].copy_from_slice(&crc.to_le_bytes());
        assert!(checksum::verify_block(&block_with_crc));
    }

    #[test]
    fn test_superblock_format() {
        let sb = Superblock::new(65536);
        assert_eq!(&sb.magic, b"NEURALFS");
        assert!(sb.total_blocks > sb.free_blocks);
        assert_eq!(sb.root_inode, 1);
        assert!(sb.next_cow_block > sb.journal_start);
    }

    #[test]
    fn test_superblock_write_read() {
        let mut disk = MemoryDisk::new(1024 * 1024);
        let sb = Superblock::new(65536);
        assert!(sb.write(&mut disk, 0));
        let sb2 = Superblock::read(&mut disk, 0).unwrap();
        assert_eq!(sb.total_blocks, sb2.total_blocks);
        assert_eq!(sb.free_blocks, sb2.free_blocks);
        assert_eq!(sb.next_cow_block, sb2.next_cow_block);
    }

    #[test]
    fn test_btree_node_basic() {
        let mut node = BTreeNode::new(42);
        assert_eq!(node.block_addr, 42);
        assert_eq!(node.level(), 0);
        assert_eq!(node.item_count(), 0);
        node.set_item_count(5);
        assert_eq!(node.item_count(), 5);
        node.set_generation(100);
        assert_eq!(node.generation(), 100);
    }

    #[test]
    fn test_btree_key() {
        let k1 = Key { object_id: 1, item_type: ItemType::Inode, offset: 0 };
        let k2 = Key { object_id: 2, item_type: ItemType::Inode, offset: 0 };
        let mut bytes = [0u8; 17];
        k1.to_bytes(&mut bytes);
        let k1r = Key::from_bytes(&bytes);
        assert_eq!(k1.object_id, k1r.object_id);
        assert_eq!(k1r.cmp(&k2), core::cmp::Ordering::Less);
    }

    #[test]
    fn test_btree_write_read() {
        let mut disk = MemoryDisk::new(1024 * 1024);
        let mut node = BTreeNode::new(5);
        node.data[100] = 0xAB;
        node.write_checksum();
        assert!(node.write(&mut disk, 0));
        let node2 = BTreeNode::read(&mut disk, 0, 5).unwrap();
        assert_eq!(node2.data[100], 0xAB);
        assert!(checksum::verify_block(&node2.data));
    }

    #[test]
    fn test_inode() {
        let ino = Inode::new_file();
        assert!(ino.is_file());
        assert!(!ino.is_dir());
        let bytes = ino.to_bytes();
        let ino2 = Inode::from_bytes(&bytes).unwrap();
        assert_eq!(ino.mode, ino2.mode);
        assert_eq!(ino.size, ino2.size);

        let dir = Inode::new_dir();
        assert!(dir.is_dir());
    }

    #[test]
    fn test_dir_entry() {
        let de = DirEntry::new("test.txt", 42);
        assert_eq!(de.name, "test.txt");
        assert_eq!(de.inode, 42);
        let bytes = de.to_bytes();
        let de2 = DirEntry::from_bytes(&bytes).unwrap();
        assert_eq!(de.name_hash, de2.name_hash);
        assert_eq!(de.name, de2.name);
    }

    #[test]
    fn test_dir_xxhash() {
        let de1 = DirEntry::new("hello.txt", 1);
        let de2 = DirEntry::new("hello.txt", 2);
        assert_eq!(de1.name_hash, de2.name_hash); // mesmo nome = mesmo hash
        let de3 = DirEntry::new("world.txt", 3);
        assert_ne!(de1.name_hash, de3.name_hash); // nomes diferentes = hashes diferentes
    }

    #[test]
    fn test_extent() {
        let ext = Extent::new(100, 50);
        assert_eq!(ext.start_block, 100);
        assert_eq!(ext.block_count, 50);
        let bytes = ext.to_bytes();
        let ext2 = Extent::from_bytes(&bytes);
        assert_eq!(ext.start_block, ext2.start_block);
        assert_eq!(ext.block_count, ext2.block_count);
    }

    #[test]
    fn test_extent_alloc_from_free() {
        let free_extents = [Extent::new(50, 200), Extent::new(500, 100)];
        let result = Extent::alloc_from_free_tree(&free_extents, 50);
        assert!(result.is_some());
        let (start, allocated, remainder) = result.unwrap();
        assert_eq!(allocated.block_count, 50);
        // last-fit: aloca do final do maior extent
        assert_eq!(start, 200); // 50 + 200 - 50 = 200
        assert!(remainder.is_some());
        assert_eq!(remainder.unwrap().block_count, 150);
    }

    #[test]
    fn test_journal_commit_recover() {
        let mut disk = MemoryDisk::new(4 * 1024 * 1024);
        let sb = Superblock::new(65536);
        assert!(sb.write(&mut disk, 0));

        let mut journal = Journal::new();
        journal.begin_tx(1);
        let mut data = [0u8; 4096];
        data[0..4].copy_from_slice(b"TEST");
        journal.log_block(42, &data);
        assert!(journal.commit(&mut disk, 0, &sb));

        // Recovery
        assert!(Journal::recover(&mut disk, 0, &sb));
    }

    #[test]
    fn test_volume_format() {
        let mut disk = MemoryDisk::new(8 * 1024 * 1024);
        assert!(NeuralVolume::format(&mut disk, 0, 8192));
        let vol = NeuralVolume::mount(&mut disk, 0);
        assert!(vol.is_some());
        let v = vol.unwrap();
        assert_eq!(&v.sb.magic, b"NEURALFS");
        assert_eq!(v.sb.root_inode, 1);
    }

    #[test]
    fn test_btree_node_find() {
        let mut node = BTreeNode::new(1);
        let k1 = Key { object_id: 10, item_type: ItemType::Inode, offset: 0 };
        let k2 = Key { object_id: 20, item_type: ItemType::Inode, offset: 0 };
        let k3 = Key { object_id: 30, item_type: ItemType::Inode, offset: 0 };

        let mut item = [0u8; 32];
        k1.to_bytes(&mut item[..17]);
        node.data[24..56].copy_from_slice(&item);
        k2.to_bytes(&mut item[..17]);
        node.data[56..88].copy_from_slice(&item);
        k3.to_bytes(&mut item[..17]);
        node.data[88..120].copy_from_slice(&item);
        node.set_item_count(3);

        assert_eq!(node.find_key(&k1), Ok(0));
        assert_eq!(node.find_key(&k2), Ok(1));
        assert_eq!(node.find_key(&k3), Ok(2));

        let k_miss = Key { object_id: 15, item_type: ItemType::Inode, offset: 0 };
        assert_eq!(node.find_key(&k_miss), Err(1));
    }
}
