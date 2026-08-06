//! NeuralFS — sistema de arquivos nativo CoW para neural-os-core.
//! Copy-on-Write, CRC32C checksums, B-tree indexes, journal recovery.

pub mod checksum;
pub mod superblock;
pub mod btree;
pub mod inode;
pub mod dir;
pub mod journal;
pub mod volume;
pub mod neural_fs_agent;
pub mod tests;
