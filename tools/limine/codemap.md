# tools/limine/

## Responsibility

Canonical Limine bootloader assets + ESP builder for the UEFI boot path (ADR-0065 / Labor 17 cutover). Produces the FAT32 ESP image that `crates/boot` turns into `target/uefi.img`. Files: `mk_esp_fat.py`, `limine.conf`, `vendor/` (Limine binaries), `esp/` (legacy reference tree), `README.md`.

## Design

- **`mk_esp_fat.py`** — builds a GPT FAT32 ESP image from an ESP directory (`--esp-dir`, `--output`, `--size-mb`). FAT32-**only**: refuses volumes with < 65 525 clusters (`FAT32_MIN_CLUSTERS`, per Microsoft rule — below that Windows treats it as FAT16), so `--size-mb 128` is the default for the boot crate. Generates unique 8.3 names (`short83`) plus full LFN entries (UTF-16LE, checksum, last-entry-first ordering) so `limine.conf`/`kernel.elf` survive on real hardware. Called by `crates/boot/build.rs`.
- **`limine.conf`** — boot entry `/Neural-OS`: `protocol: limine`, `path: boot():/kernel.elf`, `timeout: 0`, `serial: yes`, `verbose: yes`.
- **`vendor/`** — Limine binary release: `BOOTX64.EFI` (the only file copied into the ESP by `boot/build.rs`) + `extract/` full release (BOOTIA32/AA64/RISCV64/LOONGARCH64 EFIs, `limine-bios-*.bin`, `limine-tool`, Makefile). `build_esp.ps1` downloads the release zip if `BOOTX64.EFI` is missing.
- **`esp/`** — legacy checked-in ESP tree (kernel.elf + limine.conf copies under `EFI/BOOT/` and `boot/`) kept as a reference layout; `mk_esp_fat.py` runs against the tree assembled at build time in `target/limine-esp-tree`.
- **`README.md`** — documents the Limine path (`run-qemu-limine.ps1`) and marks bootloader 0.11 / `run-qemu-whpx.ps1` as legacy opt-in.

## Flow

`crates/boot/build.rs` (or `build_esp.ps1`) assembles an ESP tree → `mk_esp_fat.py` → `target/limine-esp.img` → copied to `target/uefi.img` → OVMF loads `EFI/BOOT/BOOTX64.EFI` → Limine parses `limine.conf` → loads `boot():/kernel.elf` → Limine protocol handoff to `neural-kernel/src/limine_boot.rs::_start`.

## Integration

Consumed by `crates/boot` (bindeps kernel + Limine binary + conf). Kernel-side counterpart: `k_nano::limine` request structs and the `.requests` linker section in `neural-kernel/src/limine_boot.rs`, linked by `crates/neural-kernel/limine.ld`. Downstream: `build_image.py --hw --unified` / `build_usb_unified.py` embed `target/uefi.img` into the USB image.
