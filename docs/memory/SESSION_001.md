# Session 001 — Sprint 1: Chassi Básico (Basic Chassis)

**Date:** 2026-06-21
**Goal:** Establish bare-metal Rust toolchain, bootloader integration, and first QEMU boot.

---

## Accomplished

### 1. Project Scaffolding
- `rust-toolchain.toml` — pinned **nightly** (1.98.0) with `x86_64-unknown-none` target and `llvm-tools-preview`
- `.cargo/config.toml` — target + `bootimage runner` for QEMU automation + `relocation-model=static` (critical fix)
- `Cargo.toml` — `bootloader` v0.9.34 as build-dependency with `binary` feature
- `AGENTS.md` — system rules for AI-assisted IDEs (Cursor, Windsurf, opencode, Claude Code)
- `.gitignore` — Rust/OS dev patterns

### 2. Microkernel Chassis (`src/main.rs`)
- `#![no_std]` + `#![no_main]` bare-metal environment
- Custom `panic_handler` (infinite loop placeholder)
- `_start` entry point with serial initialization (16550 UART at `0x3F8`)
- Serial output: `[SYSTEM] Neural Microkernel Iniciado. Aguardando integracao NPU/Ring 0.`
- Accepts `_boot_info: &()` parameter (required by bootloader's `context_switch` ABI)

### 3. Documentation Protocol
- `docs/architecture/0001-initial-architecture-and-toolchain.md` — ADR-0001
- `docs/memory/STATE.md` — current project state tracking
- `docs/memory/SESSION_001.md` — this file

### 4. Toolchain & Environment Setup
- **MSYS2 + MinGW-w64** installed at `C:\msys64` (required to compile `bootimage` — MSVC tools not available)
- **`bootimage`** v0.10.4 installed via `nightly-x86_64-pc-windows-gnu` toolchain
- **QEMU** added to user PATH (`C:\Program Files\qemu`)
- **Git** initialized and pushed to `github.com/msrovani/neural-os-core`

---

## Problems Encountered & Resolutions

| # | Problem | Root Cause | Resolution |
|---|---|---|---|
| 1 | `cargo install bootimage` fails — `link.exe` not found | No MSVC/Visual Studio on system | Installed MSYS2 + MinGW-w64; used GNU toolchain for bootimage compilation |
| 2 | Bootloader panics on kernel ELF | Rust nightly generates `SharedObject` (ET_DYN/PIE) by default; bootloader v0.9 expects `Executable` (ET_EXEC) | Added `-C relocation-model=static` to `rustflags` |
| 3 | Kernel page-faults on VGA write | Bootloader v0.9 does not identity-map `0xB8000` in kernel page tables | Migrated to serial output (COM1, port `0x3F8`) |
| 4 | Serial output not visible | UART not initialized; QEMU launched without `-serial stdio` | Added 16550 init sequence + `run-args = ["-serial", "stdio"]` |
| 5 | `unknown key 'target'` in kernel manifest | Bootloader v0.9.34 deprecated `[package.metadata.bootloader] target` | Removed the key; bootimage infers target automatically |

---

## Key Architectural Decisions

1. **Serial over VGA** — VGA text mode buffer (`0xB8000`) requires explicit page table mapping. Serial port is simpler, scrollable, and better for debugging.
2. **`bootloader` v0.9 (not v0.11)** — v0.9 is the battle-tested version with extensive community documentation. v0.11 has a different API and less community support.
3. **MinGW over MSVC** — Since VS Build Tools were not available, MinGW-w64 via MSYS2 provides the C linker. This affects only host-side tools (bootimage), not the kernel itself.
4. **`relocation-model=static`** — This is required for the foreseeable future until bootloader crate updates to handle ET_DYN ELF files.

---

## Current Boot Sequence

1. `cargo run` → compiles kernel for `x86_64-unknown-none`
2. `bootimage runner` → compiles bootloader, combines with kernel, creates disk image
3. QEMU boots with: `-m 2G -serial stdio`
4. Bootloader sets up long mode, page tables, kernel stack
5. Bootloader parses kernel ELF, maps segments, calls `_start(boot_info)`
6. Kernel initializes serial port, prints boot message, loops

---

## Commands

```powershell
# Build + boot in QEMU
cargo run

# Build only
cargo build

# Create boot image only
cargo bootimage

# Manual QEMU launch (if bootimage exists)
qemu-system-x86_64 -drive format=raw,file=target/x86_64-unknown-none/debug/bootimage-neural-os-core.bin -m 2G -serial stdio
```
