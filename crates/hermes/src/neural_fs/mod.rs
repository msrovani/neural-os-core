//! NeuralFS — canônico em k_nano (fonte única; consolidação SESSION_243).
//! F12: extent/checksum_tree removidos (dead code) — reexport atualizado.
pub use k_nano::neural_fs::{checksum, superblock, btree, inode, dir, journal, volume, neural_fs_agent, tests};
