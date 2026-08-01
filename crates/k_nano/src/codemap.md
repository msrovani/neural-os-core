# crates/k_nano/src/ — Source Tree

141 `.rs` files: ~80 root-level modules (memory, allocator, interrupts, pci, acpi, apic,
serial, NIC/storage drivers, FS readers, boot logs, sync, misc) + 12 submodules
(`smp/`, `disk_agent/`, `hardware/`, `net/`, `neural_fs/`, `xhci/`, `storage/`, `vfs/`,
`scheduler/`, `sync/`, `ipc/`, `fs/`). `lib.rs` declares all modules `pub` and re-exports
`globals::{ATA_DRIVER, AHCI_DRIVER, EVENT_BUS, LATENT_BUS, SKILL_REGISTRY}`. Macros
(`serial_println!`, `println!`, `kjson!`, `klogc!`, `slog_bin!`, `slog_*!`) are
`#[macro_export]` from their source files.

Full crate map, patterns, boot flow, and integration points: see
[`crates/k_nano/codemap.md`](../codemap.md).
