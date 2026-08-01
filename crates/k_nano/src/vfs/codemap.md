# crates/k_nano/src/vfs/ — Virtual Filesystem

**Responsibility**: VFS layer — global mount table (`VFS` static), path resolution
(`path::{split_path, join_path, canonicalize, parent}`), and a process fd table
(`fd::{open, read, close}`). Each mount is an agent-backed scheme (Redox/Plan 9 style).

**Key symbols**: `VFS: Mutex<Option<VfsRegistry>>`, `init_standard_mounts()` (mounts
/mnt/ram, /mnt/hdd, /mnt/sdhc, /mnt/neural, /chat, /dev, /proc, /system, /inference,
/logs), `VfsNode`/`FileMode`.

**Integration**: `disk_agent` mounts discovered partitions under `/mnt/<dev>/p<n>`;
hermes shell uses `vfs::VFS` and `vfs::fd`; the bin bridges VFS I/O to filesystems via
`fs::{read_vfs, write_vfs}`.
