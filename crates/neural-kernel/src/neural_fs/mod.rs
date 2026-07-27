//! NeuralFS — sistema de arquivos nativo CoW para neural-os-core.
//! Copy-on-Write, CRC32C checksums, B-tree indexes, journal recovery.
//! Módulos canônicos re-exportados de k_nano; cópias locais mantidas
//! apenas para arquivos que divergiram (neural_fs_agent, tests).

pub use k_nano::neural_fs::checksum;
pub use k_nano::neural_fs::superblock;
pub use k_nano::neural_fs::btree;
pub use k_nano::neural_fs::inode;
pub use k_nano::neural_fs::dir;
pub use k_nano::neural_fs::extent;
pub use k_nano::neural_fs::checksum_tree;
pub use k_nano::neural_fs::journal;
pub use k_nano::neural_fs::volume;

// Diverged — mantidos como cópias locais
pub mod neural_fs_agent;
pub mod tests;
