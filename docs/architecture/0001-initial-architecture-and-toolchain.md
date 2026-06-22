# ADR-0001: Initial Architecture and Toolchain

## Status
Accepted

## Context
We are building "neural-os-core", an AI Operating System (AIOS) from scratch. The project requires a bare-metal Rust environment with no standard library (`no_std`, `no_main`). The initial sprint must establish the toolchain, boot sequence, and minimal microkernel chassis.

## Decision

### Toolchain
- **Rust channel:** `nightly` — required for bare-metal targets (`x86_64-unknown-none`), inline assembly, and custom targets.
- **Target:** `x86_64-unknown-none` — a bare-metal target with no underlying OS.
- **Bootloader:** `bootloader` crate v0.9 — handles UEFI/BIOS handoff, enters long mode, and jumps to our `_start` entry point.
- **Boot image tool:** `bootimage` — combines bootloader + kernel into a bootable disk image.
- **Emulator:** QEMU (`qemu-system-x86_64`) — primary test harness before physical AMD APU deployment.

### Microkernel Entry Point
- `src/main.rs` defines `#![no_std]` and `#![no_main]`.
- The `_start` function is the entry point (C ABI, `#[no_mangle]`).
- A custom `panic_handler` catches panics with an infinite loop (placeholder).
- The kernel writes directly to the VGA text mode buffer (`0xB8000`) for early boot output.

### Rings Abstraction (Planned)
| Ring | Hardware | Component |
|------|----------|-----------|
| 0    | NPU      | Neural Microkernel, intent routing |
| 1    | GPU      | Tensor execution engine |
| 2    | CPU      | Wasmtime daemon/agent runtime |

This sprint only implements the Ring 0 chassis.

## Consequences
1. All code must compile for `x86_64-unknown-none` — no `std` usage is permitted.
2. Debugging requires QEMU; no native OS binary execution is possible.
3. Adding a new hardware interaction requires an ADR first (ADR protocol).
4. The boot sequence dependency on `bootloader` v0.9 ties us to its stability and update cadence.
5. VGA text mode output is a temporary primitive — will be replaced by a proper framebuffer/console abstraction.

## Prerequisites
- Rust nightly toolchain installed
- `cargo install bootimage`
- `rustup component add llvm-tools-preview`
- QEMU installed and in PATH
