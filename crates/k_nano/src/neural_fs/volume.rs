//! Volume — API de alto nivel para NeuralFS.
use crate::block_dev::BlockDevice;
use super::superblock::Superblock;
use super::btree::BTreeNode;
use super::inode::Inode;
use super::extent::Extent;
use super::journal::Journal;

pub struct NeuralVolume {
    pub sb: Superblock,
    pub start_lba: u64,
    pub tx_id: u64,
    journal: Journal,
    pub dirty: bool,
}

impl NeuralVolume {
    pub fn format(dev: &mut dyn BlockDevice, start_lba: u64, total_lba: u64) -> bool {
        let total_blocks = total_lba / 8;
        if total_blocks < 512 { return false; } // minimo 512 blocos = 4MB

        let journal_blocks = (total_blocks / 100).max(256).min(16384);
        let next_cow = 3 + journal_blocks; // reserved(0) + super(1) + backup(2) + journal(N)
        if next_cow + 2 >= total_blocks { return false; } // precisa de metadata + dados

        let fe_node_addr = next_cow;
        let root_node_addr = next_cow + 1;

        let next_free_block = root_node_addr + 2; // root +1, metadata gap
        let sb = Superblock {
            magic: *b"NEURALFS", version: 1, total_blocks,
            free_blocks: total_blocks - next_free_block,
            allocated_inodes: 1, last_tx_id: 0, root_inode: 1,
            inode_tree_root: root_node_addr, free_extent_root: fe_node_addr,
            checksum_tree_root: 0, journal_start: 3, journal_blocks,
            uuid: [0x4E555241, 0x4C46435F], label: [0; 4],
            next_cow_block: next_free_block,
        };
        if !sb.write(dev, start_lba) { return false; }

        // Inode raiz com CRC
        let mut root_data = [0u8; 4096];
        root_data[..128].copy_from_slice(&Inode::new_dir().to_bytes());
        let mut root_node = BTreeNode { block_addr: root_node_addr, data: root_data };
        if !root_node.write(dev, start_lba) { return false; }

        // Free-extent tree — comeca APOS o bloco root_node_addr + 1
        let free_start = root_node_addr + 2;
        let free_count = total_blocks - free_start;
        let mut fe_node = BTreeNode::new(fe_node_addr);
        fe_node.set_generation(1);
        let fe_key = crate::neural_fs::extent::Extent::make_free_key(free_start);
        let fe_value = Extent::new(free_start, free_count);
        let mut fe_item = [0u8; 48];
        fe_key.to_bytes(&mut fe_item[..17]);
        fe_item[17..33].copy_from_slice(&fe_value.to_bytes());
        fe_node.data[24..72].copy_from_slice(&fe_item);
        fe_node.set_item_count(1);
        if !fe_node.write(dev, start_lba) { return false; }
        true
    }

    pub fn mount(dev: &mut dyn BlockDevice, start_lba: u64) -> Option<Self> {
        let sb = Superblock::read(dev, start_lba)?;
        if &sb.magic != &super::superblock::SUPERBLOCK_MAGIC { return None; }
        let recover_ok = Journal::recover(dev, start_lba, &sb);
        let dirty = !recover_ok; // marca dirty se recovery falhou
        Some(NeuralVolume { sb, start_lba, tx_id: 0, journal: Journal::new(), dirty })
    }

    pub fn next_block(&mut self) -> u64 {
        if self.sb.free_blocks == 0 { return 0; }
        if self.sb.next_cow_block >= self.sb.total_blocks { return 0; }
        let b = self.sb.next_cow_block;
        self.sb.next_cow_block += 1;
        self.sb.free_blocks -= 1;
        b
    }

    pub fn begin_tx(&mut self) { self.tx_id += 1; self.journal.begin_tx(self.tx_id); }

    pub fn commit_tx(&mut self, dev: &mut dyn BlockDevice) -> bool {
        if !self.journal.commit(dev, self.start_lba, &self.sb) { return false; }
        self.sb.last_tx_id = self.tx_id;
        if !self.sb.write(dev, self.start_lba) { return false; }
        true
    }
}
