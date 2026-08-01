# crates/hermes/src/fs/

## Responsibility

Agent-backed filesystem layer. Defines the `FilesystemAgent` trait (read/
write/list/mount_point) and concrete agents that the VFS delegates to after
path resolution. Also provides the generic `RingBufStore` (bounded in-memory
store with oldest-eviction) and the `MhiScheduler` (MHI tier promotion/demotion
by access patterns).

## Key symbols

`mod.rs`: `trait FilesystemAgent`, `FS_AGENTS`, `register_fs_agent`,
`read_vfs`/`write_vfs`/`list_vfs` (resolve via `crate::vfs::VFS` then dispatch
to the owning agent), `RingBufStore`, `init_fs_agents`. Agents: `ata_agent`
(/mnt/hdd), `dev_fs_agent` (/dev), `proc_fs_agent` (/proc), `inference_fs_agent`
(/inference), `hermes_fs_agent` (/chat — conversation as FS), `ram_fs_agent`
(/mnt/ram), `log_fs_agent` (/logs), `neural_fs_agent` (from `crate::neural_fs`,
/mnt/neural), `mhi_scheduler`.

## Integration

`init_fs_agents()` registers all eight mounts at boot; `vfs::VFS` resolves
paths and `fs::read_vfs`/`write_vfs`/`list_vfs` route to the matching agent.
`globals::install_vfs_bridge` can override these with the bin's own VFS
implementations.
