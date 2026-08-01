# crates/neural-kernel/src/vfs/

Virtual filesystem layer: `VfsRegistry` (mount table with longest-prefix resolution,
`VfsNode` tree, inode counter), `VfsMount`, `FileMode`, path utilities (`path.rs`),
`VectorFs` (384-dim vector store backed by the global `VECTOR_FS`), and the `VFS` static.

## Key symbols

`VFS`, `VfsRegistry`, `VfsNode`, `VfsMount`, `init_standard_mounts()` (10 mounts:
`/mnt/ram`, `/mnt/hdd`, `/mnt/sdhc`, `/mnt/neural`, `/chat`, `/dev`, `/proc`, `/system`,
`/inference`, `/logs`).

## Integration

`VFS` initialized in `kernel_boot()` (K28) then `init_standard_mounts()`; fs agents
resolve paths through it (`fs::read_vfs` etc.). **Note:** legacy mirror — main.rs does not
declare `mod vfs`; the compiled `crate::vfs` is `hermes_crate::vfs` (E1b). Keep edits
mirrored to `crates/hermes/src/vfs/`.
