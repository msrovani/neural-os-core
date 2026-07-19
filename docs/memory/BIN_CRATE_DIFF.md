# BIN ↔ Crate Diff — Emagrecer neural-kernel (Onda 0)

Gerado por `tools/diff_bin_crate.py`. Status:

- `identical` / `identical_norm` — seguro `pub use`
- `stub` — já cutover
- `bin_ahead` — **promover bin→crate antes** de apagar
- `crate_ahead` — absorver extras no bin, depois stub
- `diverged` — diff manual
- `role_diff` — não cutover cego

| Módulo | Crate | Onda | LOC bin | LOC crate | Status | Cutover OK | Notas |
|--------|-------|------|--------:|----------:|--------|:----------:|-------|
| `identity.rs` | `crates/k_nano/src/identity.rs` | 0 | 2 | 184 | `stub` | yes | already stub · already pub use |
| `memory.rs` | `crates/k_nano/src/memory.rs` | 0 | 3 | 288 | `stub` | yes | already stub · already pub use |
| `mhi.rs` | `crates/k_nano/src/mhi.rs` | 0 | 4 | 388 | `stub` | yes | already stub · already pub use |
| `agency.rs` | `crates/k_ai/src/agency.rs` | 0 | 3 | 135 | `stub` | yes | already stub · already pub use |
| `audit.rs` | `crates/k_ai/src/audit.rs` | 0 | 2 | 118 | `stub` | yes | already stub · already pub use |
| `sync` | `crates/k_nano/src/sync` | 1 | 0 | 111 | `missing` | NO | dir · empty tree |
| `gpt.rs` | `crates/k_nano/src/gpt.rs` | 1 | 2 | 162 | `stub` | yes | already pub use |
| `exfat.rs` | `crates/k_nano/src/exfat.rs` | 1 | 2 | 443 | `stub` | yes | already pub use |
| `exfat_write.rs` | `crates/k_nano/src/exfat_write.rs` | 1 | 2 | 318 | `stub` | yes | already pub use |
| `tpm.rs` | `crates/k_nano/src/tpm.rs` | 1 | 2 | 279 | `stub` | yes | already pub use |
| `hw_rng.rs` | `crates/k_nano/src/hw_rng.rs` | 1 | 2 | 59 | `stub` | yes | already pub use |
| `slip.rs` | `crates/k_nano/src/slip.rs` | 1 | 2 | 74 | `stub` | yes | already pub use |
| `dma.rs` | `crates/k_nano/src/dma.rs` | 1 | 2 | 81 | `stub` | yes | already pub use |
| `slab.rs` | `crates/k_nano/src/slab.rs` | 1 | 2 | 152 | `stub` | yes | already pub use |
| `io_scheduler.rs` | `crates/k_nano/src/io_scheduler.rs` | 1 | 2 | 96 | `stub` | yes | already pub use |
| `fs_driver.rs` | `crates/k_nano/src/fs_driver.rs` | 1 | 2 | 30 | `stub` | yes | already pub use |
| `storage_manager.rs` | `crates/k_nano/src/storage_manager.rs` | 1 | 2 | 30 | `stub` | yes | already pub use |
| `rtl8139.rs` | `crates/k_nano/src/rtl8139.rs` | 1 | 2 | 326 | `stub` | yes | already pub use |
| `ahci.rs` | `crates/k_nano/src/ahci.rs` | 1 | 2 | 289 | `stub` | yes | AHCI_DRIVER in bin main · already pub use |
| `conversation.rs` | `crates/k_ai/src/conversation.rs` | 2 | 2 | 106 | `stub` | yes | already pub use |
| `hw_agents.rs` | `crates/k_ai/src/hw_agents.rs` | 2 | 2 | 133 | `stub` | yes | already pub use |
| `cognitive.rs` | `crates/k_ai/src/cognitive.rs` | 2 | 2 | 658 | `stub` | yes | already pub use |
| `chunker.rs` | `crates/k_ai/src/chunker.rs` | 2 | 2 | 100 | `stub` | yes | already pub use |
| `usage.rs` | `crates/k_ai/src/usage.rs` | 2 | 2 | 73 | `stub` | yes | already pub use |
| `profile.rs` | `crates/k_ai/src/profile.rs` | 2 | 2 | 199 | `stub` | yes | already pub use |
| `context_window.rs` | `crates/k_ai/src/context_window.rs` | 2 | 2 | 98 | `stub` | yes | already pub use |
| `training_agent.rs` | `crates/k_ai/src/training_agent.rs` | 2 | 2 | 89 | `stub` | yes | already pub use |
| `memory_agent.rs` | `crates/k_ai/src/memory_agent.rs` | 2 | 171 | 162 | `diverged` | NO | loc bin=171 crate=162 |
| `boot_log_agent.rs` | `crates/k_ai/src/boot_log_agent.rs` | 2 | 290 | 147 | `bin_ahead` | NO | loc 290>147 |
| `shutdown.rs` | `crates/k_ai/src/shutdown.rs` | 2 | 360 | 233 | `bin_ahead` | NO | split: HW stays bin · loc 360>233 |
| `inventory.rs` | `crates/k_ai/src/inventory.rs` | 2 | 2 | 96 | `stub` | yes | align HAL bridge · already pub use |
| `gguf.rs` | `crates/k_ai/src/gguf.rs` | 2 | 965 | 591 | `bin_ahead` | NO | promote bin first · loc 965>591 |
| `pci.rs` | `crates/k_nano/src/pci.rs` | 3 | 2 | 296 | `stub` | yes | already pub use |
| `serial.rs` | `crates/k_nano/src/serial.rs` | 3 | 192 | 211 | `diverged` | NO | loc bin=192 crate=211 |
| `vga_buffer.rs` | `crates/k_nano/src/vga_buffer.rs` | 3 | 216 | 196 | `diverged` | NO | macros · loc bin=216 crate=196 |
| `xhci.rs` | `crates/k_nano/src/xhci.rs` | 3 | 2 | 401 | `stub` | yes | already pub use |
| `usb_msc.rs` | `crates/k_nano/src/usb_msc.rs` | 3 | 194 | 168 | `bin_ahead` | NO | loc 194>168 |
| `virtio_net.rs` | `crates/k_nano/src/virtio_net.rs` | 3 | 466 | 439 | `bin_ahead` | NO | loc 466>439 |
| `block_dev.rs` | `crates/k_nano/src/block_dev.rs` | 3 | 40 | 44 | `diverged` | NO | loc bin=40 crate=44 |
| `simd.rs` | `crates/k_nano/src/simd.rs` | 3 | 2 | 67 | `stub` | yes | already pub use |
| `fat32.rs` | `crates/k_nano/src/fat32.rs` | 4 | 2 | 1036 | `stub` | yes | promote helpers · already pub use |
| `ata.rs` | `crates/k_nano/src/ata.rs` | 4 | 2 | 262 | `stub` | yes | unify ATA_DRIVER · already pub use |
| `e1000.rs` | `crates/k_nano/src/e1000.rs` | 4 | 2 | 654 | `stub` | yes | promote prove_rx · already pub use |
| `neural_fs` | `crates/k_nano/src/neural_fs` | 4 | 2690 | 2304 | `bin_ahead` | NO | dir; promote agent · only_bin=[] loc 2690>2304 |
| `acpi.rs` | `crates/k_nano/src/acpi.rs` | 5 | 2 | 412 | `stub` | yes | RSDP unique · already pub use |
| `apic.rs` | `crates/k_nano/src/apic.rs` | 5 | 2 | 587 | `stub` | yes | LAPIC unique · already pub use |
| `smp` | `crates/k_nano/src/smp` | 5 | 708 | 1161 | `bin_ahead` | NO | dir · only_bin=['parallel_matmul.rs'] loc 708>1161 |
| `interrupts.rs` | `crates/k_nano/src/interrupts.rs` | 5 | 436 | 497 | `crate_ahead` | yes | TIMER_TICKS unique · loc 497>436 |
| `boot_logger.rs` | `crates/k_nano/src/boot_logger.rs` | 5 | 520 | 117 | `bin_ahead` | NO | promote bin · loc 520>117 |
| `global_arena.rs` | `crates/cortex/src/global_arena.rs` | 6 | 2 | 125 | `stub` | yes | pending_route · already pub use |
| `model_hub.rs` | `crates/cortex/src/model_hub.rs` | 6 | 240 | 233 | `diverged` | NO | bin truth · loc bin=240 crate=233 |
| `bpe.rs` | `crates/cortex/src/bpe.rs` | 6 | 1022 | 154 | `bin_ahead` | NO | API incompatible · loc 1022>154 |
| `cortex.rs` | `crates/cortex/src/cortex.rs` | 6 | 2723 | 2098 | `bin_ahead` | NO | boot LLM path · loc 2723>2098 |
| `agents.rs` | `crates/hermes/src/agents.rs` | 6 | 2659 | 2262 | `bin_ahead` | NO | fleet · loc 2659>2262 |
| `aios_api.rs` | `crates/hermes/src/aios_api.rs` | 6 | 92 | 77 | `diverged` | NO | loc bin=92 crate=77 |
| `micropython_wasm.rs` | `crates/hermes/src/micropython_wasm.rs` | 6 | 284 | 286 | `diverged` | NO | loc bin=284 crate=286 |
| `net.rs` | `crates/k_nano/src/net.rs` | 6 | 681 | 3 | `role_diff` | NO | role_diff; keep bridge · bin=stack; k_nano=nic_globals |

## Gate checklist (por cutover)

1. `python tools/diff_bin_crate.py --onda N` — sem `bin_ahead` nos alvos
2. `cargo clean -p neural-kernel && cargo nk` = 0 erros
3. Boot WHPX curto: 8 fases + `[TIMER] tick=`
4. Se disco: ATA/FAT no serial; se net: não obrigatório ondas 1–3

