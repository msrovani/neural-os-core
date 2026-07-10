//! Volume — API de alto nivel para NeuralFS.
//! Formatacao, montagem, desmontagem, criacao/leitura/escrita de arquivos e diretorios.

use crate::block_dev::BlockDevice;
use super::superblock::Superblock;
use super::btree::{BTreeNode, Key, ItemType};
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
    /// Formata um disco como NeuralFS
    pub fn format(dev: &mut dyn BlockDevice, start_lba: u64, total_lba: u64) -> bool {
        let total_blocks = total_lba / 8;
        if total_blocks < 256 { return false; }
        let sb = Superblock::new(total_blocks);
        if !sb.write(dev, start_lba) { return false; }

        // Cria inode raiz na inode tree
        let mut root_node = BTreeNode::new(sb.inode_tree_root);
        root_node.set_generation(1);
        let root_ino = Inode::new_dir();
        let key = Key { object_id: 1, item_type: ItemType::Inode, offset: 0 };
        let mut item_data = [0u8; 32];
        key.to_bytes(&mut item_data[..17]);
        let ino_bytes = root_ino.to_bytes();
        item_data[17..32].copy_from_slice(&ino_bytes[..15]);
        root_node.data[24..56].copy_from_slice(&item_data);
        root_node.set_item_count(1);
        root_node.write_checksum();
        root_node.write(dev, start_lba);

        // Marca blocos usados na free-extent tree
        let used_blocks = sb.next_cow_block;
        let fe_key = Extent::make_free_key(used_blocks);
        let fe_value = Extent::new(used_blocks, total_blocks - used_blocks);
        let fe_node_addr = sb.next_cow_block + 1;
        let mut fe_node = BTreeNode::new(fe_node_addr);
        fe_node.set_generation(1);
        let mut fe_item = [0u8; 32];
        fe_key.to_bytes(&mut fe_item[..17]);
        let ext_bytes = fe_value.to_bytes();
        fe_item[17..32].copy_from_slice(&ext_bytes[..15]);
        fe_node.data[24..56].copy_from_slice(&fe_item);
        fe_node.set_item_count(1);
        fe_node.write_checksum();
        fe_node.write(dev, start_lba);

        true
    }

    /// Monta um volume NeuralFS
    pub fn mount(dev: &mut dyn BlockDevice, start_lba: u64) -> Option<Self> {
        let sb = Superblock::read(dev, start_lba)?;
        if &sb.magic != &super::superblock::SUPERBLOCK_MAGIC { return None; }
        Journal::recover(dev, start_lba, &sb);
        Some(NeuralVolume { sb, start_lba, tx_id: 0, journal: Journal::new() })
    }

    pub fn next_block(&mut self) -> u64 {
        let b = self.sb.next_cow_block;
        self.sb.next_cow_block += 1;
        self.sb.free_blocks = self.sb.free_blocks.saturating_sub(1);
        b
    }

    pub fn begin_tx(&mut self) {
        self.tx_id += 1;
        self.journal.begin_tx(self.tx_id);
    }

    pub fn commit_tx(&mut self, dev: &mut dyn BlockDevice) -> bool {
        self.sb.last_tx_id = self.tx_id;
        // Escreve blocos sujos no journal
        if !self.journal.commit(dev, self.start_lba, &self.sb) { return false; }
        // Atualiza superbloco com nova raiz e tx_id
        if !self.sb.write(dev, self.start_lba) { return false; }
        true
    }
}
