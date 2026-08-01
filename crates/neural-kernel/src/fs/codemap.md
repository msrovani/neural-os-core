# crates/neural-kernel/src/fs/

Filesystem agents: the `FilesystemAgent` trait (name/read/write/list/mount_point), the
global `FS_AGENTS` registry, VFS bridge helpers `read_vfs`/`write_vfs`/`list_vfs` (resolve
path → delegate to agent), and `RingBufStore` (evict-oldest byte-quota store backing
ram/log filesystems). Eight agents: `ata_agent` (re-export of `k_nano::fs::ata_agent`),
`dev_fs_agent` (`/dev`), `proc_fs_agent` (`/proc`), `inference_fs_agent` (`/inference`),
`hermes_fs_agent` (`/chat`), `ram_fs_agent` (`/mnt/ram`), `log_fs_agent` (`/logs`),
`mhi_scheduler` (`mhi_scheduler_tick`: MHI tier promote/demote by access pattern).

## Key symbols

`FilesystemAgent`, `FS_AGENTS`, `register_fs_agent`, `init_fs_agents()`, `read_vfs`,
`write_vfs`, `list_vfs`, `RingBufStore`, `mhi_scheduler::mhi_scheduler_tick`.

## Integration

`init_fs_agents()` called in `kernel_boot()` after VFS mounts; `install_vfs_bridge` hands
`read_vfs`/`write_vfs`/`list_vfs` to hermes. **Note:** this directory is a legacy mirror —
main.rs does not declare `mod fs`; the compiled `crate::fs` is `hermes_crate::fs`
(E1b). Keep edits mirrored to `crates/hermes/src/fs/`.
