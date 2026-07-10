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
}

impl NeuralVolume {
    pub fn format(dev: &mut dyn BlockDevice, start_lba: u64, total_lba: u64) -> bool {
        let total_blocks = total_lba / 8;
        if total_blocks < 256 { return false; }

        let journal_blocks = (total_blocks / 100).max(256).min(16384);
        let next_cow = 3 + journal_blocks; // reserved(0) + super(1) + backup(2) + journal(N)
        let fe_node_addr = next_cow + 1;
        let root_node_addr = next_cow; // inode tree root

        // Superblock com contagem correta
        let sb = Superblock {
            magic: *b"NEURALFS",
            version: 1,
            total_blocks,
            free_blocks: total_blocks - fe_node_addr - 1,
            allocated_inodes: 1,
            last_tx_id: 0,
            root_inode: 1,
            inode_tree_root: root_node_addr,
            free_extent_root: fe_node_addr,
            checksum_tree_root: 0,
            journal_start: 3,
            journal_blocks,
            uuid: [0x4E555241, 0x4C46435F],
            label: [0; 4],
            next_cow_block: fe_node_addr + 1,
        };
        if !sb.write(dev, start_lba) { return false; }

        // Cria inode raiz em bloco dedicado
        let mut root_data = [0u8; 4096];
        let root_ino = Inode::new_dir();
        root_data[..128].copy_from_slice(&root_ino.to_bytes());
        let root_node = BTreeNode { block_addr: root_node_addr, data: root_data };
        root_node.write(dev, start_lba);

        // Free-extent tree: [fe_node_addr+1, total_blocks)
        let free_start = fe_node_addr + 1;
        let free_count = total_blocks - free_start;
        let fe_value = Extent::new(free_start, free_count);
        let mut fe_node = BTreeNode::new(fe_node_addr);
        fe_node.set_generation(1);
        let fe_key = Extent::make_free_key(free_start);
        let mut fe_item = [0u8; 48]; // leaf item: key(17) + value(16 nao truncado)
        fe_key.to_bytes(&mut fe_item[..17]);
        fe_item[17..33].copy_from_slice(&fe_value.to_bytes());
        fe_node.data[24..72].copy_from_slice(&fe_item);
        fe_node.set_item_count(1);
        fe_node.write_checksum();
        fe_node.write(dev, start_lba);

        true
    }

    pub fn mount(dev: &mut dyn BlockDevice, start_lba: u64) -> Option<Self> {
        let sb = Superblock::read(dev, start_lba)?;
        if &sb.magic != &super::superblock::SUPERBLOCK_MAGIC { return None; }
        // Recovery: se falhar, monta mesmo assim (dados da ultima tx perdidos)
        let _ = Journal::recover(dev, start_lba, &sb);
        Some(NeuralVolume { sb, start_lba, tx_id: 0, journal: Journal::new() })
    }

    pub fn next_block(&mut self) -> u64 {
        if self.sb.free_blocks == 0 { return 0; } // disco cheio
        let b = self.sb.next_cow_block;
        self.sb.next_cow_block += 1;
        self.sb.free_blocks -= 1;
        b
    }

    pub fn begin_tx(&mut self) {
        self.tx_id += 1;
        self.journal.begin_tx(self.tx_id);
    }

    pub fn commit_tx(&mut self, dev: &mut dyn BlockDevice) -> bool {
        self.sb.last_tx_id = self.tx_id;
        if !self.journal.commit(dev, self.start_lba, &self.sb) { return false; }
        if !self.sb.write(dev, self.start_lba) { return false; }
        true
    }
}
