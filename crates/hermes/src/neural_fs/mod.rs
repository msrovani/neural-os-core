//! NeuralFS — sistema de arquivos nativo CoW para neural-os-core.
//! Copy-on-Write, CRC32C checksums, B-tree indexes, journal recovery.

pub mod checksum;
pub mod superblock;
pub mod btree;
pub mod inode;
pub mod dir;
pub mod extent;
pub mod checksum_tree;
pub mod journal;
pub mod volume;
pub mod neural_fs_agent;
pub mod tests;
// API: neural_fs::neural_fs_agent::allow_usb_format(true) — opt-in format USB.








