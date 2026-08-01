# crates/neural-kernel/src/neural_fs/

NeuralFS CoW filesystem facade: re-exports the canonical `k_nano::neural_fs` modules
(`checksum`, `superblock`, `btree`, `inode`, `dir`, `extent`, `checksum_tree`, `journal`,
`volume`), plus two local diverged files: `neural_fs_agent.rs` (`NeuralFsAgent` — VFS
`/mnt/neural`; ATA GPT-NeuralFS → USB-MSC → 4 MB RAM, opt-in USB format via
`USB_FORMAT_ALLOWED`) and `tests.rs`.

## Key symbols

`NeuralVolume`, `MemoryDisk`, `MBR_TYPE_NEURALFS`, `NeuralFsAgent`.

## Integration

Registered as a `FilesystemAgent` in `fs::init_fs_agents()`; mounted at `/mnt/neural`.
**Note:** legacy mirror — main.rs does not declare `mod neural_fs`; the compiled
`crate::neural_fs` is `hermes_crate::neural_fs` (E1b). Keep edits mirrored to
`crates/hermes/src/neural_fs/`.
