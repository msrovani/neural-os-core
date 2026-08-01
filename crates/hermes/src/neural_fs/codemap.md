# crates/hermes/src/neural_fs/

## Responsibility

NeuralFS — the native Copy-on-Write filesystem of neural-os-core: CRC32C
checksums, B-tree indexes, WAL journal recovery, multi-level extents, and a
VFS agent exposing it at `/mnt/neural`. SGDB path = cognitive data (HANR,
audit, package meta, skills, episodic, RAG); FAT keeps blobs/firmware.

## Key symbols

`mod.rs` wires the submodules; `neural_fs_agent.rs` exposes
`NeuralFsAgent` (mount order: ATA MBR 0x7F/GPT → USB-MSC mount-always
(format opt-in via `allow_usb_format`) → RAM 4MB) and `volume.rs` (format/
mount + file API + reclaim). Core on-disk pieces: `superblock.rs` (4096-byte
superblock, primary+backup), `btree.rs` (CoW B-tree, leaf items 48B,
key(17)+value(31)), `inode.rs`/`dir.rs`/`extent.rs` (metadata layout),
`checksum.rs`/`checksum_tree.rs` (CRC32C integrity), `journal.rs` (WAL
header(512)+data blocks, crash recovery), `tests.rs`.

## Integration

Registered as a `FilesystemAgent` by `fs::init_fs_agents()`; the
`package_hub` stores ecosystem packages under `/mnt/neural/ecosystem/`.
In RAM fallback mode it formats/mounts a 4MB in-memory volume without
touching the FAT boot disk.
