# crates/neural-kernel/src/

~106 root-level `.rs` files + 9 submodule directories. This is the **source tree of
the boot binary** — see the full map at [`../codemap.md`](../codemap.md).

## Summary

- **Entry & boot**: `limine_boot.rs` (Limine requests + `_start`/`limine_entry`),
  `main.rs` (3800 LOC: `kernel_boot`, 8-phase `BootPhase` publishes, agent registry
  assembly, model loading ladders, N1–N5 gates, scheduler hand-off).
- **Root-level residuals** (bin-only): platform (`pci`, `acpi`, `apic`, `smp/`,
  `interrupts`, `memory`, `allocator`, `arena`, `global_arena`, `simd`, `tpm`, `identity`),
  storage (`ata`, `ahci`, `fat32`, `exfat`, `exfat_write`, `gpt`, `ntfs_reader`,
  `ext2_reader`, `block_dev`, `fs_driver`, `io_scheduler`, `storage_manager`, `disk_power`,
  `netfs`), net (`net.rs` NIC statics + drivers + smoltcp wire, `netstack`, `network_agent`,
  `tls_client`, `tls_trust`, `slip`, `netdiag`, `rtl8139`, `e1000`, `i225`, `virtio_net`),
  intelligence (`cortex.rs` model wrapper, `trinity`, `tensor`, `bpe`, `gguf`,
  `gguf_mmap`, `gguf_streaming`, `model_hub`, `cognitive`, `memory_systems`, `hnsw`,
  `chunker`, `conversation`, `context_window`, `multi_user`, `training_agent`,
  `micropython_wasm`, `kv_h2o`, `ngram_spec`, `nn`, `arena`), security/isolation
  (`capability_gate`, `address_space`, `user_mode`, `isolation_ring`, `syscall`,
  `exec_arena`, `demand_page`, `k_ia_dma`, `cortex_mmap`, `jarbas_fb`, `jarbas_bridge`,
  `usb_trust`, `tpm`, `verify`, `audit`, `sync`), boot logging (`boot_logger`,
  `boot_log_agent`, `load_status`, `labor_smokes`, `bei_init`), misc (`agency`, `hw_agents`,
  `inventory`, `mhi`, `memory_agent`, `env`, `shutdown`, `profile`, `tracer`, `bench`,
  `usage`, `serial`, `vga_buffer`, `hw_rng`, `link_watcher`, `demo_flags`, `process`,
  `elf_loader`, `r3`, `neuos_probe`, `allocator`).
- **Compiled submodules**: `agents/`, `audio/`, `disk_agent/`, `smp/`, `task/`, `ipc/`.
- **Legacy (not compiled)**: `fs/`, `vfs/`, `neural_fs/` are stale mirrors — the compiled
  `crate::{fs, vfs, neural_fs}` come from `pub use hermes_crate::{fs, vfs, neural_fs}` (E1b);
  do not edit these without mirroring to `crates/hermes/src/`.

## Integration

Everything here is reached from `kernel_boot()`; nothing is reachable without going
through the boot sequence first. Re-exports of the K³CHJ crates happen at the crate root
(`main.rs` lines 82–100) — those are the only link points between bin and crates.
