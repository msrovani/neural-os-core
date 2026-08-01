# crates/boot/

## Responsibility

Workspace **default-member** boot-image builder. `cargo build --release` at the workspace root builds this crate, whose `build.rs` produces the bootable UEFI image `target/uefi.img` (intermediate `target/limine-esp.img`). It is a build-time orchestration crate — the real kernel binary lives in `neural-kernel` and is consumed via bindeps. 2 files: `Cargo.toml` + `build.rs` (plus a stub `src/main.rs`).

## Design

- **Bindeps artifact**: `Cargo.toml` declares only a build-dependency — `neural-kernel = { path = "../neural-kernel", artifact = "bin", target = "x86_64-unknown-none", features = ["fat-boot-log"] }`. Cargo compiles the kernel ELF for the bare-metal target and exposes its path to `build.rs` via `CARGO_BIN_FILE_NEURAL_KERNEL_neural-kernel`.
- **`build.rs` freshness**: `cargo:rerun-if-changed` on `build.rs`, `../neural-kernel/limine.ld`, and the kernel binary — mandatory so the image regenerates when the kernel changes (a prior boot can reformat the ESP as NeuralFS, leaving a stale `uefi.img` → OVMF "Not Found").
- **ESP assembly**: copies `kernel.elf` (bindeps) + `tools/limine/vendor/BOOTX64.EFI` (Limine binary; hard-fails with a warning if missing) + `tools/limine/limine.conf` into `target/limine-esp-tree/` (`EFI/BOOT/`, `boot/`), then runs `python tools/limine/mk_esp_fat.py --size-mb 128` → `target/limine-esp.img` → copied to `target/uefi.img`; sets `cargo:rustc-env=LIMINE_IMG` for `src/main.rs` (a host stub that merely prints the image path).
- **Entry point**: the kernel boots via the **Limine protocol** — `neural-kernel/src/limine_boot.rs` `#[no_mangle] _start` (feature `limine-boot`), requests in the `.requests` linker section, handoff collected by `k_nano::limine`. The legacy bootloader-0.11 `kernel_main` path and `bios.img` builder were removed in SESSION_232 (BIOS image was already unusable on QEMU/TCG).

## Flow

`cargo build --release` (default-members = boot) → cargo builds `neural-kernel` for `x86_64-unknown-none` → `build.rs` receives the ELF path → assembles ESP tree → `mk_esp_fat.py` (FAT32-only, ≥65525 clusters, LFN) → `target/uefi.img` → booted by OVMF/QEMU (`run-qemu-*.ps1`) or flashed to USB via `build_usb_unified.py` / `build_image.py --hw --unified`.

## Integration

Consumes `neural-kernel` (bin artifact, `fat-boot-log` feature) and `tools/limine/{mk_esp_fat.py, limine.conf, vendor/BOOTX64.EFI}`; linked by `crates/neural-kernel/limine.ld`. Downstream consumers: `tools/build_image.py --hw --unified` (requires `target/uefi.img`), `tools/build_usb_unified.py`, `inspect_usb_layout.py`.
