# crates/k_nano/src/neural_fs/ — Native CoW Filesystem

**Responsibility**: NeuralFS — the OS's own copy-on-write filesystem: superblock, B-tree
indexes, inodes, directory hash lookup, extents, CRC32C checksum tree, write-ahead
journal, volume API (format/mount/file API/reclaim), and the `NeuralFsAgent` that mounts
it at `/mnt/neural` (ATA type 0x7F or 4MB RAM fallback).

**Key symbols**: `volume::{Volume, probe_magic, format/mount...}`, `superblock`,
`btree`, `inode`, `dir`, `extent`, `checksum_tree`, `checksum` (CRC32C),
`journal`, `neural_fs_agent::NeuralFsAgent`, `tests` (MemoryDisk under `cfg(test)`).

**Integration**: disk_agent mounts it into `vfs::VFS`; the agent implements
`agent_core::Agent`; the bin routes VFS I/O through `fs::{read_vfs, write_vfs}` when a
NeuralFS volume is mounted.
